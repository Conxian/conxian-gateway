use std::{
    env,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};

#[cfg(unix)]
use std::os::unix::{fs::PermissionsExt, process::CommandExt};

use crate::{
    error::EvalError,
    model::{
        ArtifactSpec, EnvironmentReport, ExitStatusReport, FixtureReport, Manifest, OutputReport,
        Report, BACKEND, DEFAULT_MAX_OUTPUT_BYTES, DEFAULT_MAX_RSS_BYTES,
        DEFAULT_SCALED_TIMEOUT_SECONDS, DEFAULT_SMALL_TIMEOUT_SECONDS, MANIFEST_SCHEMA_VERSION,
        MAX_HEX_INPUT_BYTES, REPORT_SCHEMA_VERSION, UPSTREAM_REVISION, WARNING,
    },
};

const PROOF_SIZE_REASON: &str = "not_applicable_cpu_backend";
const SANDBOX_ACTIVE_ENV: &str = "BITVMX_EVAL_SANDBOX_ACTIVE";
const SANDBOX_MODE_ENV: &str = "BITVMX_EVAL_SANDBOX_MODE";
const SANDBOX_ACTIVE_VALUE: &str = "1";
const SANDBOX_MODE_VALUE: &str = "network-deny";

#[derive(Debug, Clone, Copy)]
struct EffectiveLimits {
    max_rss_bytes: u64,
    timeout_seconds: u64,
    max_output_bytes: u64,
}

#[derive(Debug)]
struct ProcessOutcome {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    status: ExitStatus,
    wall_time_ms: u64,
    cpu_user_time_us: Option<u64>,
    cpu_system_time_us: Option<u64>,
    maximum_rss_bytes: Option<u64>,
    timed_out: bool,
    rss_exceeded: bool,
    output_exceeded: bool,
    reader_error: Option<String>,
}

#[derive(Debug, Default)]
struct CaptureState {
    total_bytes: AtomicU64,
    output_exceeded: AtomicBool,
    stdout: Mutex<Vec<u8>>,
    stderr: Mutex<Vec<u8>>,
    reader_error: Mutex<Option<String>>,
}

#[derive(Debug, Clone, Copy)]
struct ProcSample {
    user_ticks: u64,
    system_ticks: u64,
    high_water_rss_bytes: Option<u64>,
}

#[derive(Debug)]
struct ParsedResult {
    result_class: String,
    executed_steps: Option<u64>,
}

pub fn run_manifest(manifest_path: &Path, report_path: &Path) -> Result<Report, EvalError> {
    let manifest_text = fs::read_to_string(manifest_path).map_err(|error| {
        EvalError::Io(format!(
            "read manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    let manifest: Manifest = serde_json::from_str(&manifest_text)
        .map_err(|error| EvalError::Manifest(format!("invalid JSON: {error}")))?;
    let limits = validate_manifest(&manifest)?;
    validate_sandbox_preflight(&manifest)?;

    let manifest_dir = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .map_err(|error| {
            EvalError::Preflight(format!(
                "manifest directory {} is unavailable: {error}",
                manifest_path.display()
            ))
        })?;

    let executable_path =
        resolve_existing_path(&manifest_dir, &manifest.executable.path, "executable")?;
    validate_executable(&executable_path)?;

    let revision_path = resolve_existing_path(
        &manifest_dir,
        &manifest.executable.revision_file,
        "revision sidecar",
    )?;
    validate_regular_file(&revision_path, "revision sidecar")?;
    validate_revision(&revision_path, &manifest.upstream_revision)?;

    let fixture_path = resolve_existing_path(&manifest_dir, &manifest.fixture.path, "fixture")?;
    validate_regular_file(&fixture_path, "fixture")?;

    let executable_sha256 = sha256_file(&executable_path).map_err(|error| {
        EvalError::Preflight(format!(
            "hash executable {}: {error}",
            executable_path.display()
        ))
    })?;
    if executable_sha256 != manifest.executable.sha256 {
        return Err(EvalError::Preflight(format!(
            "executable SHA-256 mismatch: expected {}, got {}",
            manifest.executable.sha256, executable_sha256
        )));
    }

    let fixture_sha256 = sha256_file(&fixture_path).map_err(|error| {
        EvalError::Preflight(format!("hash fixture {}: {error}", fixture_path.display()))
    })?;
    if fixture_sha256 != manifest.fixture.sha256 {
        return Err(EvalError::Preflight(format!(
            "fixture SHA-256 mismatch: expected {}, got {}",
            manifest.fixture.sha256, fixture_sha256
        )));
    }

    let arguments = build_arguments(&manifest, &fixture_path);
    let started_at_unix_ms = unix_time_ms();
    let process = run_process(&executable_path, &arguments, limits)?;

    let executable_post_hash = sha256_file(&executable_path).map_err(|error| {
        EvalError::Preflight(format!(
            "re-hash executable {}: {error}",
            executable_path.display()
        ))
    })?;
    let fixture_post_hash = sha256_file(&fixture_path).map_err(|error| {
        EvalError::Preflight(format!(
            "re-hash fixture {}: {error}",
            fixture_path.display()
        ))
    })?;
    let revision_post = fs::read_to_string(&revision_path)
        .ok()
        .map(|value| value.trim().to_string());

    let outputs = vec![
        output_report("stdout", None, &process.stdout, !process.output_exceeded),
        output_report("stderr", None, &process.stderr, !process.output_exceeded),
    ];
    let artifacts = collect_artifacts(&manifest.artifacts, &manifest_dir)?;
    let parsed_result = parse_evaluator_output(&process.stdout, &process.stderr).ok();
    let actual_result_class = parsed_result
        .as_ref()
        .map(|result| result.result_class.clone());
    let executed_steps = parsed_result
        .as_ref()
        .and_then(|result| result.executed_steps);

    let mut failure = None;
    if process.timed_out {
        failure = Some("timeout".to_string());
    } else if process.rss_exceeded {
        failure = Some("rss_limit".to_string());
    } else if process.output_exceeded {
        failure = Some("output_limit".to_string());
    } else if let Some(error) = &process.reader_error {
        failure = Some(format!("output_read_error:{error}"));
    } else if !process.status.success() {
        failure = Some("nonzero_exit".to_string());
    } else if executable_post_hash != executable_sha256 {
        failure = Some("executable_changed_during_execution".to_string());
    } else if fixture_post_hash != fixture_sha256 {
        failure = Some("fixture_changed_during_execution".to_string());
    } else if revision_post.as_deref() != Some(manifest.upstream_revision.as_str()) {
        failure = Some("upstream_revision_changed_during_execution".to_string());
    } else if parsed_result.is_none() {
        failure = Some("malformed_or_unrecognized_output".to_string());
    } else if actual_result_class.as_deref() != Some(&manifest.fixture.expected_result_class) {
        failure = Some("unexpected_result_class".to_string());
    } else if let (Some(expected), Some(returned)) = (
        manifest.fixture.expected_return_value,
        halt_return_value(&process.stdout, &process.stderr),
    ) {
        if expected != returned {
            failure = Some("unexpected_return_value".to_string());
        }
    }

    let report = Report {
        schema_version: REPORT_SCHEMA_VERSION.to_string(),
        warning: WARNING.to_string(),
        experimental: true,
        production_supported: false,
        cryptographic_verification: false,
        backend: BACKEND.to_string(),
        upstream_revision: manifest.upstream_revision.clone(),
        executable_sha256: Some(executable_sha256),
        fixture: FixtureReport {
            id: manifest.fixture.id.clone(),
            kind: manifest.fixture.kind.clone(),
            path: fixture_path.display().to_string(),
            sha256: Some(fixture_sha256),
        },
        expected_result_class: manifest.fixture.expected_result_class.clone(),
        actual_result_class,
        executed_command: executable_path.display().to_string(),
        arguments,
        started_at_unix_ms,
        wall_time_ms: Some(process.wall_time_ms),
        cpu_user_time_us: process.cpu_user_time_us,
        cpu_system_time_us: process.cpu_system_time_us,
        maximum_rss_bytes: process.maximum_rss_bytes,
        executed_steps,
        outputs,
        artifacts,
        environment: environment_report(),
        exit_status: Some(exit_status_report(&process.status)),
        proof_size_bytes: None,
        proof_size_reason: PROOF_SIZE_REASON.to_string(),
        failure,
    };

    write_report(report_path, &report)?;

    if let Some(reason) = &report.failure {
        return Err(EvalError::execution_rejected(reason.clone(), report_path));
    }

    Ok(report)
}

pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_manifest(manifest: &Manifest) -> Result<EffectiveLimits, EvalError> {
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION || manifest.manifest_version != 1 {
        return Err(EvalError::Manifest(format!(
            "unsupported schema/version: {}/{}",
            manifest.schema_version, manifest.manifest_version
        )));
    }
    if manifest.warning != WARNING {
        return Err(EvalError::Manifest(
            "warning text does not match contract".to_string(),
        ));
    }
    if !manifest.experimental
        || manifest.production_supported
        || manifest.cryptographic_verification
    {
        return Err(EvalError::Manifest(
            "evaluation safety flags must be experimental=true, production_supported=false, and cryptographic_verification=false"
                .to_string(),
        ));
    }
    if manifest.backend != BACKEND {
        return Err(EvalError::Manifest(format!(
            "unsupported backend: {}",
            manifest.backend
        )));
    }
    if manifest.upstream_revision != UPSTREAM_REVISION {
        return Err(EvalError::Manifest(format!(
            "unsupported upstream revision: expected {UPSTREAM_REVISION}, got {}",
            manifest.upstream_revision
        )));
    }
    validate_sha256(&manifest.executable.sha256, "executable SHA-256")?;
    validate_sha256(&manifest.fixture.sha256, "fixture SHA-256")?;
    if manifest.executable.path.is_empty() || manifest.executable.revision_file.is_empty() {
        return Err(EvalError::Manifest(
            "executable path and revision_file are required".to_string(),
        ));
    }
    if manifest.fixture.id.is_empty() || manifest.fixture.path.is_empty() {
        return Err(EvalError::Manifest(
            "fixture id and path are required".to_string(),
        ));
    }
    if !matches!(manifest.fixture.kind.as_str(), "elf" | "synthetic-source") {
        return Err(EvalError::Manifest(format!(
            "unsupported fixture kind: {}",
            manifest.fixture.kind
        )));
    }
    match manifest.fixture.expected_result_class.as_str() {
        "halt_success" => {
            if manifest.fixture.expected_return_value != Some(0) {
                return Err(EvalError::Manifest(
                    "halt_success requires expected_return_value=0".to_string(),
                ));
            }
        }
        "halt_failure" => {
            if manifest
                .fixture
                .expected_return_value
                .is_none_or(|value| value == 0)
            {
                return Err(EvalError::Manifest(
                    "halt_failure requires a nonzero expected_return_value".to_string(),
                ));
            }
        }
        "limit_reached" => {
            if manifest.fixture.expected_return_value.is_some() {
                return Err(EvalError::Manifest(
                    "limit_reached cannot specify expected_return_value".to_string(),
                ));
            }
        }
        other => {
            return Err(EvalError::Manifest(format!(
                "unsupported expected_result_class: {other}"
            )));
        }
    }

    let max_rss_bytes = manifest
        .limits
        .max_rss_bytes
        .unwrap_or(DEFAULT_MAX_RSS_BYTES);
    if max_rss_bytes == 0 || max_rss_bytes > DEFAULT_MAX_RSS_BYTES {
        return Err(EvalError::Manifest(format!(
            "max_rss_bytes must be between 1 and {DEFAULT_MAX_RSS_BYTES}"
        )));
    }
    let default_timeout = match manifest.execution.workload.as_str() {
        "small" => DEFAULT_SMALL_TIMEOUT_SECONDS,
        "scaled" => DEFAULT_SCALED_TIMEOUT_SECONDS,
        other => {
            return Err(EvalError::Manifest(format!(
                "unsupported workload: {other}"
            )))
        }
    };
    let timeout_seconds = manifest.limits.timeout_seconds.unwrap_or(default_timeout);
    if timeout_seconds == 0 || timeout_seconds > default_timeout {
        return Err(EvalError::Manifest(format!(
            "timeout_seconds must be between 1 and {default_timeout} for {} workload",
            manifest.execution.workload
        )));
    }
    let max_output_bytes = manifest
        .limits
        .max_output_bytes
        .unwrap_or(DEFAULT_MAX_OUTPUT_BYTES);
    if max_output_bytes == 0 || max_output_bytes > DEFAULT_MAX_OUTPUT_BYTES {
        return Err(EvalError::Manifest(format!(
            "max_output_bytes must be between 1 and {DEFAULT_MAX_OUTPUT_BYTES}"
        )));
    }
    validate_input_hex(manifest.execution.input_hex.as_deref())?;

    if manifest.sandbox.mode != "external-preflight" || manifest.sandbox.network_policy != "deny" {
        return Err(EvalError::Manifest(
            "sandbox must require external-preflight with network_policy=deny".to_string(),
        ));
    }

    let mut names = std::collections::HashSet::new();
    for artifact in &manifest.artifacts {
        if artifact.name.is_empty() || artifact.path.is_empty() {
            return Err(EvalError::Manifest(
                "artifact name and path are required".to_string(),
            ));
        }
        if !names.insert(&artifact.name) {
            return Err(EvalError::Manifest(format!(
                "duplicate artifact name: {}",
                artifact.name
            )));
        }
    }

    Ok(EffectiveLimits {
        max_rss_bytes,
        timeout_seconds,
        max_output_bytes,
    })
}

fn validate_sandbox_preflight(manifest: &Manifest) -> Result<(), EvalError> {
    let active = env::var(SANDBOX_ACTIVE_ENV).ok();
    let mode = env::var(SANDBOX_MODE_ENV).ok();
    if active.as_deref() != Some(SANDBOX_ACTIVE_VALUE)
        || mode.as_deref() != Some(SANDBOX_MODE_VALUE)
    {
        return Err(EvalError::Preflight(format!(
            "external network-deny sandbox required: set {SANDBOX_ACTIVE_ENV}=1 and {SANDBOX_MODE_ENV}=network-deny in the already-active sandbox"
        )));
    }
    if manifest.sandbox.mode != "external-preflight" {
        return Err(EvalError::Preflight(
            "manifest does not request the supported external sandbox preflight".to_string(),
        ));
    }
    Ok(())
}

fn validate_input_hex(input: Option<&str>) -> Result<(), EvalError> {
    let Some(input) = input else { return Ok(()) };
    if input.len() % 2 != 0 || input.len() / 2 > MAX_HEX_INPUT_BYTES {
        return Err(EvalError::Manifest(
            "input_hex must contain an even number of hex digits within the input limit"
                .to_string(),
        ));
    }
    if !input.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(EvalError::Manifest(
            "input_hex contains a non-hex character".to_string(),
        ));
    }
    Ok(())
}

fn validate_sha256(value: &str, name: &str) -> Result<(), EvalError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(EvalError::Manifest(format!(
            "{name} must be a 64-character hexadecimal digest"
        )));
    }
    if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(EvalError::Manifest(format!(
            "{name} must use lowercase hexadecimal"
        )));
    }
    Ok(())
}

fn resolve_existing_path(
    manifest_dir: &Path,
    raw_path: &str,
    label: &str,
) -> Result<PathBuf, EvalError> {
    let path = PathBuf::from(raw_path);
    let path = if path.is_absolute() {
        path
    } else {
        manifest_dir.join(path)
    };
    path.canonicalize().map_err(|error| {
        EvalError::Preflight(format!(
            "{label} {} is unavailable: {error}",
            path.display()
        ))
    })
}

fn validate_regular_file(path: &Path, label: &str) -> Result<(), EvalError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        EvalError::Preflight(format!(
            "{label} {} is unavailable: {error}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(EvalError::Preflight(format!(
            "{label} {} is not a non-symlink regular file",
            path.display()
        )));
    }
    Ok(())
}

fn validate_executable(path: &Path) -> Result<(), EvalError> {
    validate_regular_file(path, "executable")?;
    #[cfg(unix)]
    {
        let mode = fs::metadata(path)
            .map_err(|error| EvalError::Preflight(format!("stat executable: {error}")))?
            .permissions()
            .mode();
        if mode & 0o111 == 0 {
            return Err(EvalError::Preflight(format!(
                "executable {} has no execute permission",
                path.display()
            )));
        }
    }
    Ok(())
}

fn validate_revision(path: &Path, expected: &str) -> Result<(), EvalError> {
    let actual = fs::read_to_string(path).map_err(|error| {
        EvalError::Preflight(format!("read revision sidecar {}: {error}", path.display()))
    })?;
    if actual.trim() != expected {
        return Err(EvalError::Preflight(format!(
            "upstream revision mismatch: expected {expected}, got {}",
            actual.trim()
        )));
    }
    Ok(())
}

fn build_arguments(manifest: &Manifest, fixture_path: &Path) -> Vec<String> {
    let mut arguments = vec![
        "execute".to_string(),
        "--elf".to_string(),
        fixture_path.display().to_string(),
        "--no-mapping".to_string(),
    ];
    if manifest.execution.no_hash {
        arguments.push("--no-hash".to_string());
    }
    if manifest.execution.trace {
        arguments.push("--trace".to_string());
    }
    if let Some(input_hex) = &manifest.execution.input_hex {
        arguments.push("--input".to_string());
        arguments.push(input_hex.clone());
    }
    if let Some(limit_steps) = manifest.execution.limit_steps {
        arguments.push("--limit".to_string());
        arguments.push(limit_steps.to_string());
    }
    arguments
}

fn run_process(
    executable_path: &Path,
    arguments: &[String],
    limits: EffectiveLimits,
) -> Result<ProcessOutcome, EvalError> {
    let mut command = Command::new(executable_path);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .env(
            "PATH",
            env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
        )
        .env("LANG", "C")
        .env("LC_ALL", "C");

    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let mut child = command.spawn().map_err(|error| {
        EvalError::Preflight(format!(
            "spawn evaluator {}: {error}",
            executable_path.display()
        ))
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| EvalError::Io("evaluator stdout pipe unavailable".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| EvalError::Io("evaluator stderr pipe unavailable".to_string()))?;

    let capture = Arc::new(CaptureState {
        stdout: Mutex::new(Vec::new()),
        stderr: Mutex::new(Vec::new()),
        ..CaptureState::default()
    });
    let stdout_capture = Arc::clone(&capture);
    let stderr_capture = Arc::clone(&capture);
    let stdout_thread =
        thread::spawn(move || read_stream(stdout, stdout_capture, true, limits.max_output_bytes));
    let stderr_thread =
        thread::spawn(move || read_stream(stderr, stderr_capture, false, limits.max_output_bytes));

    let start = Instant::now();
    let pid = child.id();
    let mut initial_sample = sample_process(pid);
    let mut last_sample = initial_sample;
    let mut maximum_rss_bytes = initial_sample.and_then(|sample| sample.high_water_rss_bytes);
    let mut timed_out = false;
    let mut rss_exceeded = false;
    let mut output_exceeded = false;
    let status = loop {
        if let Some(sample) = sample_process(pid) {
            if initial_sample.is_none() {
                initial_sample = Some(sample);
            }
            last_sample = Some(sample);
            if let Some(rss) = sample.high_water_rss_bytes {
                maximum_rss_bytes = Some(maximum_rss_bytes.unwrap_or(0).max(rss));
                if rss > limits.max_rss_bytes {
                    rss_exceeded = true;
                }
            }
        }

        if let Some(exit_status) = child
            .try_wait()
            .map_err(|error| EvalError::Io(format!("poll evaluator: {error}")))?
        {
            break exit_status;
        }

        if capture.output_exceeded.load(Ordering::Relaxed) {
            output_exceeded = true;
        }
        if output_exceeded
            || rss_exceeded
            || start.elapsed() >= Duration::from_secs(limits.timeout_seconds)
        {
            timed_out = !output_exceeded && !rss_exceeded;
            if output_exceeded || rss_exceeded || timed_out {
                kill_child(&mut child);
            }
        }
        thread::sleep(Duration::from_millis(10));
    };
    // Reap the direct child and close any pipes held by descendants. On Unix
    // the child is a dedicated process group; killing the already-finished
    // group's remainder is harmless and avoids an unbounded reader join.
    kill_child(&mut child);
    if timed_out || rss_exceeded || output_exceeded {
        let _ = child.wait();
    }

    let _ = stdout_thread.join();
    let _ = stderr_thread.join();
    output_exceeded |= capture.output_exceeded.load(Ordering::Relaxed);

    if let Some(sample) = sample_process(pid).or(last_sample) {
        if let Some(rss) = sample.high_water_rss_bytes {
            maximum_rss_bytes = Some(maximum_rss_bytes.unwrap_or(0).max(rss));
        }
        last_sample = Some(sample);
    }

    let stdout = capture
        .stdout
        .lock()
        .map_err(|_| EvalError::Io("stdout capture lock poisoned".to_string()))?
        .clone();
    let stderr = capture
        .stderr
        .lock()
        .map_err(|_| EvalError::Io("stderr capture lock poisoned".to_string()))?
        .clone();
    let reader_error = capture
        .reader_error
        .lock()
        .map_err(|_| EvalError::Io("reader error lock poisoned".to_string()))?
        .clone();

    let (cpu_user_time_us, cpu_system_time_us) =
        match (initial_sample, last_sample, clock_ticks_per_second()) {
            (Some(initial), Some(last), Some(ticks_per_second)) => (
                Some(ticks_to_us(
                    last.user_ticks.saturating_sub(initial.user_ticks),
                    ticks_per_second,
                )),
                Some(ticks_to_us(
                    last.system_ticks.saturating_sub(initial.system_ticks),
                    ticks_per_second,
                )),
            ),
            _ => (None, None),
        };

    Ok(ProcessOutcome {
        stdout,
        stderr,
        status,
        wall_time_ms: start.elapsed().as_millis().min(u64::MAX as u128) as u64,
        cpu_user_time_us,
        cpu_system_time_us,
        maximum_rss_bytes,
        timed_out,
        rss_exceeded,
        output_exceeded,
        reader_error,
    })
}

fn read_stream<R: Read>(mut reader: R, capture: Arc<CaptureState>, stdout: bool, maximum: u64) {
    let mut buffer = [0_u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                let total = capture
                    .total_bytes
                    .fetch_add(read as u64, Ordering::Relaxed)
                    .saturating_add(read as u64);
                if total > maximum {
                    capture.output_exceeded.store(true, Ordering::Relaxed);
                    break;
                }
                let target = if stdout {
                    &capture.stdout
                } else {
                    &capture.stderr
                };
                if let Ok(mut output) = target.lock() {
                    output.extend_from_slice(&buffer[..read]);
                } else {
                    if let Ok(mut error) = capture.reader_error.lock() {
                        *error = Some("capture lock poisoned".to_string());
                    }
                    break;
                }
            }
            Err(error) => {
                if let Ok(mut reader_error) = capture.reader_error.lock() {
                    *reader_error = Some(error.to_string());
                }
                break;
            }
        }
    }
}

fn kill_child(child: &mut Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as libc::pid_t;
        unsafe {
            let _ = libc::kill(-pid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

fn sample_process(pid: u32) -> Option<ProcSample> {
    #[cfg(target_os = "linux")]
    {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let end = stat.rfind(')')?;
        let fields: Vec<&str> = stat[end + 1..].split_whitespace().collect();
        let user_ticks = fields.get(11)?.parse().ok()?;
        let system_ticks = fields.get(12)?.parse().ok()?;
        let status = fs::read_to_string(format!("/proc/{pid}/status")).ok();
        let high_water_rss_bytes = status.as_deref().and_then(parse_high_water_rss);
        Some(ProcSample {
            user_ticks,
            system_ticks,
            high_water_rss_bytes,
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

#[cfg(target_os = "linux")]
fn parse_high_water_rss(status: &str) -> Option<u64> {
    status.lines().find_map(|line| {
        let value = line.strip_prefix("VmHWM:")?.trim();
        let kilobytes = value.strip_suffix("kB")?.trim().parse::<u64>().ok()?;
        Some(kilobytes.saturating_mul(1024))
    })
}

#[cfg(unix)]
fn clock_ticks_per_second() -> Option<u64> {
    let value = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    (value > 0).then_some(value as u64)
}

#[cfg(not(unix))]
fn clock_ticks_per_second() -> Option<u64> {
    None
}

fn ticks_to_us(ticks: u64, ticks_per_second: u64) -> u64 {
    ticks
        .saturating_mul(1_000_000)
        .checked_div(ticks_per_second.max(1))
        .unwrap_or(u64::MAX)
}

fn parse_evaluator_output(stdout: &[u8], stderr: &[u8]) -> Result<ParsedResult, String> {
    let stdout =
        String::from_utf8(stdout.to_vec()).map_err(|_| "stdout is not UTF-8".to_string())?;
    let stderr =
        String::from_utf8(stderr.to_vec()).map_err(|_| "stderr is not UTF-8".to_string())?;
    let mut parsed = None;
    for line in stdout.lines().chain(stderr.lines()) {
        let Some((_, result)) = line.split_once("Execution result:") else {
            continue;
        };
        if parsed.is_some() {
            return Err("multiple evaluator result lines".to_string());
        }
        parsed = Some(parse_result_value(result.trim())?);
    }
    parsed.ok_or_else(|| "no recognized evaluator result".to_string())
}

fn parse_result_value(value: &str) -> Result<ParsedResult, String> {
    if let Some(contents) = value
        .strip_prefix("Halt(")
        .and_then(|v| v.strip_suffix(')'))
    {
        let mut values = contents.split(',').map(str::trim);
        let return_value = values
            .next()
            .ok_or_else(|| "Halt result is missing return value".to_string())?
            .parse::<u32>()
            .map_err(|_| "Halt return value is not a u32".to_string())?;
        let steps = values
            .next()
            .ok_or_else(|| "Halt result is missing step count".to_string())?
            .parse::<u64>()
            .map_err(|_| "Halt step count is not a u64".to_string())?;
        if values.next().is_some() {
            return Err("Halt result has too many fields".to_string());
        }
        return Ok(ParsedResult {
            result_class: if return_value == 0 {
                "halt_success".to_string()
            } else {
                "halt_failure".to_string()
            },
            executed_steps: Some(steps),
        });
    }
    if let Some(contents) = value
        .strip_prefix("LimitStepReached(")
        .and_then(|v| v.strip_suffix(')'))
    {
        let steps = contents
            .trim()
            .parse::<u64>()
            .map_err(|_| "LimitStepReached step count is not a u64".to_string())?;
        return Ok(ParsedResult {
            result_class: "limit_reached".to_string(),
            executed_steps: Some(steps),
        });
    }
    Err(format!("unsupported evaluator result: {value}"))
}

fn halt_return_value(stdout: &[u8], stderr: &[u8]) -> Option<u32> {
    let stdout = String::from_utf8(stdout.to_vec()).ok()?;
    let stderr = String::from_utf8(stderr.to_vec()).ok()?;
    stdout
        .lines()
        .chain(stderr.lines())
        .find_map(|line| {
            line.split_once("Execution result:")
                .map(|(_, result)| result.trim())
        })
        .and_then(|value| value.strip_prefix("Halt("))
        .and_then(|value| value.split_once(',').map(|(return_value, _)| return_value))
        .and_then(|return_value| return_value.trim().parse().ok())
}

fn output_report(name: &str, path: Option<String>, bytes: &[u8], complete: bool) -> OutputReport {
    OutputReport {
        name: name.to_string(),
        path,
        size_bytes: bytes.len() as u64,
        sha256: sha256_bytes(bytes),
        complete,
    }
}

fn collect_artifacts(
    artifacts: &[ArtifactSpec],
    manifest_dir: &Path,
) -> Result<Vec<OutputReport>, EvalError> {
    artifacts
        .iter()
        .map(|artifact| {
            let path = resolve_existing_path(manifest_dir, &artifact.path, "artifact")?;
            validate_regular_file(&path, "artifact")?;
            let bytes = fs::read(&path).map_err(|error| {
                EvalError::Preflight(format!("read artifact {}: {error}", path.display()))
            })?;
            Ok(output_report(
                &artifact.name,
                Some(path.display().to_string()),
                &bytes,
                true,
            ))
        })
        .collect()
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn environment_report() -> EnvironmentReport {
    EnvironmentReport {
        os: env::consts::OS.to_string(),
        architecture: env::consts::ARCH.to_string(),
        kernel: fs::read_to_string("/proc/sys/kernel/osrelease")
            .ok()
            .map(|value| value.trim().to_string()),
        rustc: tool_version("rustc"),
        cargo: tool_version("cargo"),
        wrapper_version: env!("CARGO_PKG_VERSION").to_string(),
        profile: if cfg!(debug_assertions) {
            "debug".to_string()
        } else {
            "release".to_string()
        },
        cpu_count: std::thread::available_parallelism().ok().map(usize::from),
    }
}

fn tool_version(tool: &str) -> Option<String> {
    let mut command = Command::new(tool);
    command
        .arg("--version")
        .env_clear()
        .env(
            "PATH",
            env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
        )
        .env("LANG", "C")
        .env("LC_ALL", "C");
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let mut version = String::from_utf8(output.stdout).ok()?;
    version.truncate(256);
    Some(version.trim().to_string())
}

fn unix_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn exit_status_report(status: &ExitStatus) -> ExitStatusReport {
    ExitStatusReport {
        success: status.success(),
        code: status.code(),
        #[cfg(unix)]
        signal: std::os::unix::process::ExitStatusExt::signal(status),
        #[cfg(not(unix))]
        signal: None,
    }
}

fn write_report(path: &Path, report: &Report) -> Result<(), EvalError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            EvalError::Io(format!(
                "create report directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    let contents = serde_json::to_vec_pretty(report)
        .map_err(|error| EvalError::Io(format!("serialize report: {error}")))?;
    let temporary = path.with_file_name(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("report"),
        std::process::id()
    ));
    let mut file = File::create(&temporary).map_err(|error| {
        EvalError::Io(format!(
            "create temporary report {}: {error}",
            temporary.display()
        ))
    })?;
    file.write_all(&contents).map_err(|error| {
        EvalError::Io(format!(
            "write temporary report {}: {error}",
            temporary.display()
        ))
    })?;
    file.sync_all().map_err(|error| {
        EvalError::Io(format!(
            "sync temporary report {}: {error}",
            temporary.display()
        ))
    })?;
    fs::rename(&temporary, path)
        .map_err(|error| EvalError::Io(format!("publish report {}: {error}", path.display())))?;
    Ok(())
}
