use std::{
    collections::{HashMap, HashSet},
    env,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};

#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::{fs::PermissionsExt, process::CommandExt};

use crate::{
    error::EvalError,
    model::{
        ArtifactReport, ArtifactSpec, EnvironmentReport, ExitStatusReport, FixtureReport,
        IntegrityReport, Manifest, OutputReport, Report, RevisionReport, BACKEND,
        DEFAULT_MAX_ARTIFACT_BYTES, DEFAULT_MAX_OUTPUT_BYTES, DEFAULT_MAX_RSS_BYTES,
        DEFAULT_MAX_TOTAL_ARTIFACT_BYTES, DEFAULT_SCALED_TIMEOUT_SECONDS,
        DEFAULT_SMALL_TIMEOUT_SECONDS, HARD_MAX_ARTIFACT_BYTES, LINUX_RESOURCE_SCOPE,
        MANIFEST_SCHEMA_VERSION, MAX_HEX_INPUT_BYTES, REPORT_SCHEMA_VERSION, UPSTREAM_REVISION,
        UPSTREAM_REVISION_LINE, WARNING,
    },
};

const PROOF_SIZE_REASON: &str = "not_applicable_cpu_backend";
const SANDBOX_ACTIVE_ENV: &str = "BITVMX_EVAL_SANDBOX_ACTIVE";
const SANDBOX_MODE_ENV: &str = "BITVMX_EVAL_SANDBOX_MODE";
const SANDBOX_ACTIVE_VALUE: &str = "1";
const SANDBOX_MODE_VALUE: &str = "network-deny";
const MAX_REVISION_BYTES: u64 = 128;
const READER_POLL_INTERVAL: Duration = Duration::from_millis(5);
const CLEANUP_GRACE_PERIOD: Duration = Duration::from_millis(750);
const REPORT_TEMP_ATTEMPTS: usize = 128;

static REPORT_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy)]
struct EffectiveLimits {
    max_rss_bytes: u64,
    timeout_seconds: u64,
    max_output_bytes: u64,
    max_artifact_bytes: u64,
    max_total_artifact_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileIdentity {
    canonical: PathBuf,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

#[derive(Debug, Clone)]
struct ValidatedFile {
    original: PathBuf,
    canonical: PathBuf,
    identity: FileIdentity,
}

#[derive(Debug, Clone)]
struct ReportDestination {
    path: PathBuf,
    canonical_target: PathBuf,
    identity: Option<FileIdentity>,
}

#[derive(Debug, Clone)]
struct ArtifactTarget {
    spec: ArtifactSpec,
    original: PathBuf,
    canonical_target: PathBuf,
    existing: Option<ValidatedFile>,
    pre_error: Option<String>,
}

#[derive(Debug, Clone)]
struct FileSnapshot {
    sha256: String,
    size_bytes: u64,
    identity: FileIdentity,
}

#[derive(Debug, Clone)]
struct RevisionObservation {
    observed_bytes_hex: Option<String>,
    exact: bool,
    identity: Option<FileIdentity>,
    error: Option<String>,
}

#[derive(Debug)]
struct ProcessOutcome {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    status: Option<ExitStatus>,
    wall_time_ms: u64,
    cpu_user_time_us: Option<u64>,
    cpu_system_time_us: Option<u64>,
    maximum_rss_bytes: Option<u64>,
    timed_out: bool,
    rss_exceeded: bool,
    output_exceeded: bool,
    descendant_process_detected: bool,
    reader_error: Option<String>,
    cleanup_error: Option<String>,
}

#[derive(Debug, Default)]
struct CaptureState {
    total_bytes: AtomicU64,
    output_exceeded: AtomicBool,
    stop_readers: AtomicBool,
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

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
struct ProcEntry {
    parent_pid: u32,
    start_time_ticks: u64,
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
struct ObservedProcess {
    pid: u32,
    start_time_ticks: u64,
}

#[derive(Debug)]
struct ParsedResult {
    result_class: String,
    executed_steps: Option<u64>,
    return_value: Option<u32>,
}

struct ReportBuilder {
    report: Report,
}

impl ReportBuilder {
    fn new(manifest: &Manifest, report_path: &Path) -> Self {
        Self {
            report: Report {
                schema_version: REPORT_SCHEMA_VERSION.to_string(),
                warning: WARNING.to_string(),
                experimental: true,
                production_supported: false,
                cryptographic_verification: false,
                backend: BACKEND.to_string(),
                upstream_revision: manifest.upstream_revision.clone(),
                report_path: report_path.display().to_string(),
                resource_scope: manifest.sandbox.resource_scope.clone(),
                descendant_process_detected: false,
                executable: empty_integrity_report(manifest.executable.path.clone()),
                fixture: FixtureReport {
                    id: manifest.fixture.id.clone(),
                    kind: manifest.fixture.kind.clone(),
                    integrity: empty_integrity_report(manifest.fixture.path.clone()),
                },
                revision: RevisionReport {
                    path: manifest.executable.revision_file.clone(),
                    expected_bytes_hex: bytes_to_hex(UPSTREAM_REVISION_LINE.as_bytes()),
                    pre_observed_bytes_hex: None,
                    pre_exact: false,
                    pre_identity: None,
                    pre_error: None,
                    post_observed_bytes_hex: None,
                    post_exact: false,
                    post_identity: None,
                    post_error: None,
                },
                expected_result_class: manifest.fixture.expected_result_class.clone(),
                expected_return_value: manifest.fixture.expected_return_value,
                actual_result_class: None,
                actual_return_value: None,
                executed_command: manifest.executable.path.clone(),
                arguments: Vec::new(),
                started_at_unix_ms: unix_time_ms(),
                wall_time_ms: None,
                cpu_user_time_us: None,
                cpu_system_time_us: None,
                maximum_rss_bytes: None,
                executed_steps: None,
                outputs: Vec::new(),
                artifacts: manifest
                    .artifacts
                    .iter()
                    .map(empty_artifact_report)
                    .collect(),
                environment: environment_report(),
                exit_status: None,
                proof_size_bytes: None,
                proof_size_reason: PROOF_SIZE_REASON.to_string(),
                failure: None,
                failure_details: Vec::new(),
            },
        }
    }

    fn fail(&mut self, class: impl Into<String>, detail: impl Into<String>) {
        let class = class.into();
        if self.report.failure.is_none() {
            self.report.failure = Some(class);
        }
        self.report.failure_details.push(detail.into());
    }

    fn set_executable_path(&mut self, file: &ValidatedFile) {
        self.report.executable.path = file.canonical.display().to_string();
        self.report.executed_command = file.canonical.display().to_string();
    }

    fn set_fixture_path(&mut self, file: &ValidatedFile) {
        self.report.fixture.integrity.path = file.canonical.display().to_string();
    }

    fn set_revision_path(&mut self, file: &ValidatedFile) {
        self.report.revision.path = file.canonical.display().to_string();
    }

    fn set_executable_pre(&mut self, snapshot: &FileSnapshot) {
        set_integrity_pre(&mut self.report.executable, snapshot);
    }

    fn set_executable_post(&mut self, snapshot: &FileSnapshot) {
        set_integrity_post(&mut self.report.executable, snapshot);
    }

    fn set_fixture_pre(&mut self, snapshot: &FileSnapshot) {
        set_integrity_pre(&mut self.report.fixture.integrity, snapshot);
    }

    fn set_fixture_post(&mut self, snapshot: &FileSnapshot) {
        set_integrity_post(&mut self.report.fixture.integrity, snapshot);
    }

    fn set_executable_pre_error(&mut self, error: impl Into<String>) {
        self.report.executable.pre_error = Some(error.into());
    }

    fn set_executable_post_error(&mut self, error: impl Into<String>) {
        self.report.executable.post_error = Some(error.into());
    }

    fn set_fixture_pre_error(&mut self, error: impl Into<String>) {
        self.report.fixture.integrity.pre_error = Some(error.into());
    }

    fn set_fixture_post_error(&mut self, error: impl Into<String>) {
        self.report.fixture.integrity.post_error = Some(error.into());
    }

    fn set_revision_pre(&mut self, observation: &RevisionObservation) {
        self.report.revision.pre_observed_bytes_hex = observation.observed_bytes_hex.clone();
        self.report.revision.pre_exact = observation.exact;
        self.report.revision.pre_identity = observation.identity.as_ref().map(identity_label);
        self.report.revision.pre_error = observation.error.clone();
    }

    fn set_revision_post(&mut self, observation: &RevisionObservation) {
        self.report.revision.post_observed_bytes_hex = observation.observed_bytes_hex.clone();
        self.report.revision.post_exact = observation.exact;
        self.report.revision.post_identity = observation.identity.as_ref().map(identity_label);
        self.report.revision.post_error = observation.error.clone();
    }

    fn persist_failure(self, report_path: &Path) -> EvalError {
        let reason = self
            .report
            .failure
            .clone()
            .unwrap_or_else(|| "evaluation_rejected".to_string());
        match write_report(report_path, &self.report) {
            Ok(()) => EvalError::execution_rejected(reason, report_path.to_path_buf()),
            Err(error) => error,
        }
    }

    fn persist_success(self, report_path: &Path) -> Result<Report, EvalError> {
        write_report(report_path, &self.report)?;
        if let Some(reason) = &self.report.failure {
            return Err(EvalError::execution_rejected(
                reason.clone(),
                report_path.to_path_buf(),
            ));
        }
        Ok(self.report)
    }
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

    let manifest_dir = resolve_manifest_dir(manifest_path)?;
    let report_destination = prepare_report_destination(report_path)?;
    let mut builder = ReportBuilder::new(&manifest, &report_destination.path);

    let executable =
        match resolve_existing_path(&manifest_dir, &manifest.executable.path, "executable") {
            Ok(file) => file,
            Err(error) => {
                builder.fail("executable_path_invalid", error.to_string());
                return Err(builder.persist_failure(&report_destination.path));
            }
        };
    let revision = match resolve_existing_path(
        &manifest_dir,
        &manifest.executable.revision_file,
        "revision sidecar",
    ) {
        Ok(file) => file,
        Err(error) => {
            builder.fail("revision_path_invalid", error.to_string());
            return Err(builder.persist_failure(&report_destination.path));
        }
    };
    let fixture = match resolve_existing_path(&manifest_dir, &manifest.fixture.path, "fixture") {
        Ok(file) => file,
        Err(error) => {
            builder.fail("fixture_path_invalid", error.to_string());
            return Err(builder.persist_failure(&report_destination.path));
        }
    };
    if let Err(error) = validate_executable(&executable) {
        builder.fail("executable_path_invalid", error.to_string());
        return Err(builder.persist_failure(&report_destination.path));
    }

    builder.set_executable_path(&executable);
    builder.set_revision_path(&revision);
    builder.set_fixture_path(&fixture);

    let artifact_targets = prepare_artifact_targets(&manifest_dir, &manifest.artifacts);
    for (index, target) in artifact_targets.iter().enumerate() {
        builder.report.artifacts[index].path = target.canonical_target.display().to_string();
        if let Some(error) = &target.pre_error {
            builder.report.artifacts[index].error = Some(error.clone());
            builder.fail(
                "artifact_path_invalid",
                format!("artifact {}: {error}", target.spec.name),
            );
        }
    }
    if builder.report.failure.is_some() {
        return Err(builder.persist_failure(&report_destination.path));
    }

    if let Some(alias) = find_report_alias(
        &report_destination,
        [&executable, &revision, &fixture]
            .into_iter()
            .collect::<Vec<_>>(),
        &artifact_targets,
    ) {
        return Err(EvalError::Preflight(format!(
            "report path aliases {alias}; choose a distinct report path"
        )));
    }
    if let Some(alias) = find_artifact_alias(&artifact_targets, [&executable, &revision, &fixture])
    {
        builder.fail(
            "artifact_path_alias",
            format!("artifact path aliases protected input {alias}"),
        );
        return Err(builder.persist_failure(&report_destination.path));
    }

    let executable_pre = match capture_file_snapshot(&executable, "executable") {
        Ok(snapshot) => snapshot,
        Err(error) => {
            builder.set_executable_pre_error(error.clone());
            builder.fail("executable_preflight_integrity", error);
            return Err(builder.persist_failure(&report_destination.path));
        }
    };
    builder.set_executable_pre(&executable_pre);
    if executable_pre.sha256 != manifest.executable.sha256 {
        builder.fail(
            "executable_hash_mismatch",
            format!(
                "executable SHA-256 mismatch: expected {}, got {}",
                manifest.executable.sha256, executable_pre.sha256
            ),
        );
    }

    let fixture_pre = match capture_file_snapshot(&fixture, "fixture") {
        Ok(snapshot) => snapshot,
        Err(error) => {
            builder.set_fixture_pre_error(error.clone());
            builder.fail("fixture_preflight_integrity", error);
            return Err(builder.persist_failure(&report_destination.path));
        }
    };
    builder.set_fixture_pre(&fixture_pre);
    if fixture_pre.sha256 != manifest.fixture.sha256 {
        builder.fail(
            "fixture_hash_mismatch",
            format!(
                "fixture SHA-256 mismatch: expected {}, got {}",
                manifest.fixture.sha256, fixture_pre.sha256
            ),
        );
    }

    let revision_pre = observe_revision(&revision);
    builder.set_revision_pre(&revision_pre);
    if let Some(error) = &revision_pre.error {
        builder.fail("revision_preflight_integrity", error.clone());
    } else if !revision_pre.exact {
        builder.fail(
            "revision_mismatch",
            format!(
                "revision sidecar must contain exact bytes {:?}",
                UPSTREAM_REVISION_LINE.as_bytes()
            ),
        );
    }
    if builder.report.failure.is_some() {
        return Err(builder.persist_failure(&report_destination.path));
    }

    if let Err(error) = revalidate_file(&executable, "executable") {
        builder.fail("executable_changed_before_execution", error.to_string());
    }
    if let Err(error) = revalidate_file(&revision, "revision sidecar") {
        builder.fail("revision_changed_before_execution", error.to_string());
    }
    if let Err(error) = revalidate_file(&fixture, "fixture") {
        builder.fail("fixture_changed_before_execution", error.to_string());
    }
    match capture_file_snapshot(&executable, "executable") {
        Ok(snapshot) if snapshot.sha256 == executable_pre.sha256 => {}
        Ok(snapshot) => builder.fail(
            "executable_changed_before_execution",
            format!(
                "executable preflight hash {} differs from immediate pre-execution hash {}",
                executable_pre.sha256, snapshot.sha256
            ),
        ),
        Err(error) => builder.fail("executable_changed_before_execution", error),
    }
    match capture_file_snapshot(&fixture, "fixture") {
        Ok(snapshot) if snapshot.sha256 == fixture_pre.sha256 => {}
        Ok(snapshot) => builder.fail(
            "fixture_changed_before_execution",
            format!(
                "fixture preflight hash {} differs from immediate pre-execution hash {}",
                fixture_pre.sha256, snapshot.sha256
            ),
        ),
        Err(error) => builder.fail("fixture_changed_before_execution", error),
    }
    let revision_before_execution = observe_revision(&revision);
    if revision_before_execution.error.is_some() || !revision_before_execution.exact {
        builder.fail(
            "revision_changed_before_execution",
            revision_before_execution
                .error
                .unwrap_or_else(|| "revision sidecar changed before execution".to_string()),
        );
    }
    if builder.report.failure.is_some() {
        return Err(builder.persist_failure(&report_destination.path));
    }

    let arguments = build_arguments(&manifest, &fixture.canonical);
    builder.report.arguments = arguments.clone();

    let process = match run_process(&executable.canonical, &arguments, limits) {
        Ok(process) => process,
        Err(error) => {
            builder.fail("process_setup_failure", error.to_string());
            return Err(builder.persist_failure(&report_destination.path));
        }
    };
    builder.report.descendant_process_detected = process.descendant_process_detected;
    builder.report.outputs = vec![
        output_report("stdout", None, &process.stdout, !process.output_exceeded),
        output_report("stderr", None, &process.stderr, !process.output_exceeded),
    ];
    builder.report.wall_time_ms = Some(process.wall_time_ms);
    builder.report.cpu_user_time_us = process.cpu_user_time_us;
    builder.report.cpu_system_time_us = process.cpu_system_time_us;
    builder.report.maximum_rss_bytes = process.maximum_rss_bytes;
    builder.report.exit_status = process.status.as_ref().map(exit_status_report);

    if process.timed_out {
        builder.fail("timeout", "direct child wall-time limit reached");
    }
    if process.rss_exceeded {
        builder.fail("rss_limit", "direct child RSS limit reached");
    }
    if process.output_exceeded {
        builder.fail(
            "output_limit",
            "aggregate stdout/stderr capture limit reached",
        );
    }
    if process.descendant_process_detected {
        builder.fail(
            "descendant_process_detected",
            "Linux process-tree monitoring found a descendant; evaluation failed closed",
        );
    }
    if let Some(error) = &process.reader_error {
        builder.fail("output_read_error", error.clone());
    }
    if let Some(error) = &process.cleanup_error {
        builder.fail("child_cleanup_failure", error.clone());
    }
    if process
        .status
        .as_ref()
        .is_none_or(|status| !status.success())
    {
        builder.fail("nonzero_exit", "evaluator did not exit successfully");
    }

    match capture_file_snapshot(&executable, "executable") {
        Ok(snapshot) => {
            builder.set_executable_post(&snapshot);
            if snapshot.sha256 != executable_pre.sha256 {
                builder.fail(
                    "executable_changed_during_execution",
                    format!(
                        "executable pre-hash {} differs from post-hash {}",
                        executable_pre.sha256, snapshot.sha256
                    ),
                );
            }
            if !same_identity(&snapshot.identity, &executable_pre.identity) {
                builder.fail(
                    "executable_identity_changed_during_execution",
                    "executable file identity changed during execution",
                );
            }
        }
        Err(error) => {
            builder.set_executable_post_error(error.clone());
            builder.fail("executable_postrun_integrity", error);
        }
    }

    match capture_file_snapshot(&fixture, "fixture") {
        Ok(snapshot) => {
            builder.set_fixture_post(&snapshot);
            if snapshot.sha256 != fixture_pre.sha256 {
                builder.fail(
                    "fixture_changed_during_execution",
                    format!(
                        "fixture pre-hash {} differs from post-hash {}",
                        fixture_pre.sha256, snapshot.sha256
                    ),
                );
            }
            if !same_identity(&snapshot.identity, &fixture_pre.identity) {
                builder.fail(
                    "fixture_identity_changed_during_execution",
                    "fixture file identity changed during execution",
                );
            }
        }
        Err(error) => {
            builder.set_fixture_post_error(error.clone());
            builder.fail("fixture_postrun_integrity", error);
        }
    }

    let revision_post = observe_revision(&revision);
    builder.set_revision_post(&revision_post);
    if let Some(error) = &revision_post.error {
        builder.fail("revision_postrun_integrity", error.clone());
    } else if !revision_post.exact {
        builder.fail(
            "upstream_revision_changed_during_execution",
            "revision sidecar post-run bytes were not the exact pinned line",
        );
    }
    if revision_post.identity.is_some()
        && revision_pre.identity.is_some()
        && revision_post.identity != revision_pre.identity
    {
        builder.fail(
            "revision_identity_changed_during_execution",
            "revision sidecar file identity changed during execution",
        );
    }

    collect_artifacts(
        &artifact_targets,
        &mut builder,
        limits.max_artifact_bytes,
        limits.max_total_artifact_bytes,
        &report_destination,
        [&executable, &revision, &fixture],
    );

    match parse_evaluator_output(&process.stdout, &process.stderr) {
        Ok(parsed) => {
            builder.report.actual_result_class = Some(parsed.result_class.clone());
            builder.report.actual_return_value = parsed.return_value;
            builder.report.executed_steps = parsed.executed_steps;
            if parsed.result_class != manifest.fixture.expected_result_class {
                builder.fail(
                    "unexpected_result_class",
                    format!(
                        "expected {}, got {}",
                        manifest.fixture.expected_result_class, parsed.result_class
                    ),
                );
            } else if let Some(expected_return_value) = manifest.fixture.expected_return_value {
                if parsed.return_value != Some(expected_return_value) {
                    builder.fail(
                        "unexpected_return_value",
                        format!(
                            "expected return value {expected_return_value}, got {:?}",
                            parsed.return_value
                        ),
                    );
                }
            } else if manifest.fixture.expected_result_class == "limit_reached"
                && parsed.executed_steps != manifest.execution.limit_steps
            {
                builder.fail(
                    "unexpected_limit_step_count",
                    format!(
                        "expected limit step count {:?}, got {:?}",
                        manifest.execution.limit_steps, parsed.executed_steps
                    ),
                );
            }
        }
        Err(error) => builder.fail("malformed_or_unrecognized_output", error),
    }

    builder.persist_success(&report_destination.path)
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
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION || manifest.manifest_version != 2 {
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
            if manifest.fixture.expected_return_value.is_some()
                || manifest
                    .execution
                    .limit_steps
                    .is_none_or(|value| value == 0)
            {
                return Err(EvalError::Manifest(
                    "limit_reached requires a positive execution.limit_steps and no expected_return_value"
                        .to_string(),
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
    let max_artifact_bytes = manifest
        .limits
        .max_artifact_bytes
        .unwrap_or(DEFAULT_MAX_ARTIFACT_BYTES);
    if max_artifact_bytes == 0 || max_artifact_bytes > HARD_MAX_ARTIFACT_BYTES {
        return Err(EvalError::Manifest(format!(
            "max_artifact_bytes must be between 1 and {HARD_MAX_ARTIFACT_BYTES}"
        )));
    }
    let max_total_artifact_bytes = manifest
        .limits
        .max_total_artifact_bytes
        .unwrap_or(DEFAULT_MAX_TOTAL_ARTIFACT_BYTES);
    if max_total_artifact_bytes == 0 || max_total_artifact_bytes > HARD_MAX_ARTIFACT_BYTES {
        return Err(EvalError::Manifest(format!(
            "max_total_artifact_bytes must be between 1 and {HARD_MAX_ARTIFACT_BYTES}"
        )));
    }
    validate_input_hex(manifest.execution.input_hex.as_deref())?;

    if manifest.sandbox.mode != "external-preflight" || manifest.sandbox.network_policy != "deny" {
        return Err(EvalError::Manifest(
            "sandbox must require external-preflight with network_policy=deny".to_string(),
        ));
    }
    validate_resource_scope(&manifest.sandbox.resource_scope)?;

    let mut names = HashSet::new();
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
        validate_sha256(&artifact.sha256, "artifact SHA-256")?;
        if artifact.max_size_bytes == 0
            || artifact.max_size_bytes > max_artifact_bytes
            || artifact.max_size_bytes > HARD_MAX_ARTIFACT_BYTES
        {
            return Err(EvalError::Manifest(format!(
                "artifact {} max_size_bytes must be between 1 and {max_artifact_bytes}",
                artifact.name
            )));
        }
    }

    Ok(EffectiveLimits {
        max_rss_bytes,
        timeout_seconds,
        max_output_bytes,
        max_artifact_bytes,
        max_total_artifact_bytes,
    })
}

fn validate_resource_scope(scope: &str) -> Result<(), EvalError> {
    #[cfg(target_os = "linux")]
    {
        if scope != LINUX_RESOURCE_SCOPE {
            return Err(EvalError::Manifest(format!(
                "Linux requires resource_scope={LINUX_RESOURCE_SCOPE}"
            )));
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        if scope != NON_LINUX_RESOURCE_SCOPE {
            return Err(EvalError::Manifest(format!(
                "unsupported platform requires resource_scope={NON_LINUX_RESOURCE_SCOPE}"
            )));
        }
        Err(EvalError::Preflight(
            "non-Linux direct-child-only mode is explicitly unavailable; use a Linux evaluation host"
                .to_string(),
        ))
    }
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

fn resolve_manifest_dir(manifest_path: &Path) -> Result<PathBuf, EvalError> {
    let parent = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let absolute_parent = absolute_path(parent)?;
    validate_directory_components(&absolute_parent, "manifest directory")?;
    absolute_parent.canonicalize().map_err(|error| {
        EvalError::Preflight(format!(
            "manifest directory {} is unavailable: {error}",
            absolute_parent.display()
        ))
    })
}

fn absolute_path(path: &Path) -> Result<PathBuf, EvalError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()
            .map_err(|error| EvalError::Preflight(format!("resolve current directory: {error}")))?
            .join(path))
    }
}

fn prepare_report_destination(path: &Path) -> Result<ReportDestination, EvalError> {
    let absolute = absolute_path(path)?;
    reject_parent_components(&absolute, "report path")?;
    let parent = absolute.parent().ok_or_else(|| {
        EvalError::Preflight(format!("report path {} has no parent", absolute.display()))
    })?;
    validate_directory_components(parent, "report parent")?;
    let canonical_parent = parent.canonicalize().map_err(|error| {
        EvalError::Preflight(format!(
            "report parent {} is unavailable: {error}",
            parent.display()
        ))
    })?;
    let file_name = absolute
        .file_name()
        .ok_or_else(|| {
            EvalError::Preflight(format!(
                "report path {} has no file name",
                absolute.display()
            ))
        })?
        .to_os_string();
    match fs::symlink_metadata(&absolute) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
                return Err(EvalError::Preflight(format!(
                    "report path {} is not a non-symlink regular file",
                    absolute.display()
                )));
            }
            let canonical_target = absolute.canonicalize().map_err(|error| {
                EvalError::Preflight(format!(
                    "canonicalize report path {}: {error}",
                    absolute.display()
                ))
            })?;
            let identity = file_identity(&canonical_target, &metadata);
            Ok(ReportDestination {
                path: absolute,
                canonical_target,
                identity: Some(identity),
            })
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(ReportDestination {
            path: absolute,
            canonical_target: canonical_parent.join(&file_name),
            identity: None,
        }),
        Err(error) => Err(EvalError::Preflight(format!(
            "inspect report path {}: {error}",
            absolute.display()
        ))),
    }
}

fn resolve_existing_path(
    manifest_dir: &Path,
    raw_path: &str,
    label: &str,
) -> Result<ValidatedFile, EvalError> {
    let path = manifest_relative_path(manifest_dir, raw_path, label)?;
    let metadata = validate_path_components(&path, label, true)?.ok_or_else(|| {
        EvalError::Preflight(format!("{label} {} is unavailable", path.display()))
    })?;
    if !metadata.file_type().is_file() {
        return Err(EvalError::Preflight(format!(
            "{label} {} is not a regular file",
            path.display()
        )));
    }
    let canonical = path.canonicalize().map_err(|error| {
        EvalError::Preflight(format!("canonicalize {label} {}: {error}", path.display()))
    })?;
    Ok(ValidatedFile {
        original: path,
        canonical: canonical.clone(),
        identity: file_identity(&canonical, &metadata),
    })
}

fn manifest_relative_path(
    manifest_dir: &Path,
    raw_path: &str,
    label: &str,
) -> Result<PathBuf, EvalError> {
    let raw = PathBuf::from(raw_path);
    if raw.as_os_str().is_empty() {
        return Err(EvalError::Preflight(format!("{label} path is empty")));
    }
    reject_parent_components(&raw, label)?;
    Ok(if raw.is_absolute() {
        raw
    } else {
        manifest_dir.join(raw)
    })
}

fn validate_path_components(
    path: &Path,
    label: &str,
    final_must_exist: bool,
) -> Result<Option<fs::Metadata>, EvalError> {
    reject_parent_components(path, label)?;
    let components: Vec<Component<'_>> = path.components().collect();
    if components.is_empty() {
        return Err(EvalError::Preflight(format!("{label} path is empty")));
    }
    let mut current = PathBuf::new();
    for (index, component) in components.iter().enumerate() {
        current.push(component.as_os_str());
        let final_component = index + 1 == components.len();
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(EvalError::Preflight(format!(
                        "{label} path component {} is a symlink",
                        current.display()
                    )));
                }
                if final_component {
                    return Ok(Some(metadata));
                }
                if !metadata.file_type().is_dir() {
                    return Err(EvalError::Preflight(format!(
                        "{label} path component {} is not a directory",
                        current.display()
                    )));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound && final_component => {
                if final_must_exist {
                    return Err(EvalError::Preflight(format!(
                        "{label} {} is unavailable: {error}",
                        path.display()
                    )));
                }
                return Ok(None);
            }
            Err(error) => {
                return Err(EvalError::Preflight(format!(
                    "inspect {label} path component {}: {error}",
                    current.display()
                )))
            }
        }
    }
    Ok(None)
}

fn validate_directory_components(path: &Path, label: &str) -> Result<(), EvalError> {
    let metadata = validate_path_components(path, label, true)?.ok_or_else(|| {
        EvalError::Preflight(format!("{label} {} is unavailable", path.display()))
    })?;
    if !metadata.file_type().is_dir() {
        return Err(EvalError::Preflight(format!(
            "{label} {} is not a directory",
            path.display()
        )));
    }
    Ok(())
}

fn reject_parent_components(path: &Path, label: &str) -> Result<(), EvalError> {
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(EvalError::Preflight(format!(
            "{label} must not contain '..' path components"
        )));
    }
    Ok(())
}

fn validate_executable(file: &ValidatedFile) -> Result<(), EvalError> {
    let metadata = revalidate_file(file, "executable")?;
    #[cfg(unix)]
    {
        let mode = metadata.permissions().mode();
        if mode & 0o111 == 0 {
            return Err(EvalError::Preflight(format!(
                "executable {} has no execute permission",
                file.canonical.display()
            )));
        }
    }
    Ok(())
}

fn revalidate_file(file: &ValidatedFile, label: &str) -> Result<fs::Metadata, EvalError> {
    let metadata = validate_path_components(&file.original, label, true)?.ok_or_else(|| {
        EvalError::Preflight(format!(
            "{label} {} is unavailable",
            file.original.display()
        ))
    })?;
    if !metadata.file_type().is_file() {
        return Err(EvalError::Preflight(format!(
            "{label} {} is not a regular file",
            file.original.display()
        )));
    }
    let canonical = file.original.canonicalize().map_err(|error| {
        EvalError::Preflight(format!(
            "canonicalize {label} {}: {error}",
            file.original.display()
        ))
    })?;
    let current_identity = file_identity(&canonical, &metadata);
    if canonical != file.canonical || !same_identity(&current_identity, &file.identity) {
        return Err(EvalError::Preflight(format!(
            "{label} {} changed identity or canonical path",
            file.original.display()
        )));
    }
    Ok(metadata)
}

fn file_identity(canonical: &Path, metadata: &fs::Metadata) -> FileIdentity {
    FileIdentity {
        canonical: canonical.to_path_buf(),
        #[cfg(unix)]
        device: std::os::unix::fs::MetadataExt::dev(metadata),
        #[cfg(unix)]
        inode: std::os::unix::fs::MetadataExt::ino(metadata),
    }
}

fn same_identity(left: &FileIdentity, right: &FileIdentity) -> bool {
    #[cfg(unix)]
    {
        left.device == right.device && left.inode == right.inode
    }
    #[cfg(not(unix))]
    {
        left.canonical == right.canonical
    }
}

fn identity_label(identity: &FileIdentity) -> String {
    #[cfg(unix)]
    {
        format!("dev:{}:ino:{}", identity.device, identity.inode)
    }
    #[cfg(not(unix))]
    {
        identity.canonical.display().to_string()
    }
}

fn capture_file_snapshot(file: &ValidatedFile, label: &str) -> Result<FileSnapshot, String> {
    let metadata = revalidate_file(file, label).map_err(|error| error.to_string())?;
    let sha256 = sha256_file(&file.canonical)
        .map_err(|error| format!("hash {label} {}: {error}", file.canonical.display()))?;
    let after = revalidate_file(file, label).map_err(|error| error.to_string())?;
    let after_identity = file_identity(&file.canonical, &after);
    let before_identity = file_identity(&file.canonical, &metadata);
    if !same_identity(&before_identity, &after_identity) {
        return Err(format!(
            "{label} changed identity while it was being hashed"
        ));
    }
    Ok(FileSnapshot {
        sha256,
        size_bytes: metadata.len(),
        identity: after_identity,
    })
}

fn observe_revision(file: &ValidatedFile) -> RevisionObservation {
    let _metadata = match revalidate_file(file, "revision sidecar") {
        Ok(metadata) => metadata,
        Err(error) => {
            return RevisionObservation {
                observed_bytes_hex: None,
                exact: false,
                identity: None,
                error: Some(error.to_string()),
            }
        }
    };
    let bytes = match read_bounded(&file.canonical, MAX_REVISION_BYTES) {
        Ok(bytes) => bytes,
        Err(error) => {
            return RevisionObservation {
                observed_bytes_hex: None,
                exact: false,
                identity: Some(file.identity.clone()),
                error: Some(format!(
                    "read revision sidecar {}: {error}",
                    file.canonical.display()
                )),
            }
        }
    };
    let after = match revalidate_file(file, "revision sidecar") {
        Ok(metadata) => metadata,
        Err(error) => {
            return RevisionObservation {
                observed_bytes_hex: Some(bytes_to_hex(&bytes)),
                exact: false,
                identity: Some(file.identity.clone()),
                error: Some(error.to_string()),
            }
        }
    };
    let after_identity = file_identity(&file.canonical, &after);
    let exact = bytes == UPSTREAM_REVISION_LINE.as_bytes()
        && same_identity(&after_identity, &file.identity);
    RevisionObservation {
        observed_bytes_hex: Some(bytes_to_hex(&bytes)),
        exact,
        identity: Some(after_identity),
        error: None,
    }
}

fn read_bounded(path: &Path, maximum: u64) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        if bytes.len() as u64 + read as u64 > maximum {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "file exceeds hard read bound",
            ));
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    Ok(bytes)
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
        .env("PATH", "/usr/bin:/bin")
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
    let stdout = child.stdout.take().ok_or_else(|| {
        let _ = child.kill();
        let _ = child.wait();
        EvalError::Io("evaluator stdout pipe unavailable".to_string())
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        let _ = child.kill();
        let _ = child.wait();
        EvalError::Io("evaluator stderr pipe unavailable".to_string())
    })?;

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
    #[cfg(target_os = "linux")]
    let mut observed_descendants = HashMap::new();
    let mut initial_sample = match sample_process(pid) {
        Ok(sample) => sample,
        Err(error) => {
            #[cfg(target_os = "linux")]
            {
                return Err(abort_after_spawn(
                    &mut child,
                    &capture,
                    stdout_thread,
                    stderr_thread,
                    &observed_descendants,
                    format!("Linux direct-child RSS monitoring unavailable: {error}"),
                ));
            }
            #[cfg(not(target_os = "linux"))]
            {
                return Err(abort_after_spawn(
                    &mut child,
                    &capture,
                    stdout_thread,
                    stderr_thread,
                    format!("direct-child monitoring unavailable: {error}"),
                ));
            }
        }
    };
    let mut last_sample = initial_sample;
    let mut maximum_rss_bytes = initial_sample.and_then(|sample| sample.high_water_rss_bytes);
    let mut timed_out = false;
    let mut rss_exceeded = false;
    let mut output_exceeded = false;
    let mut descendant_process_detected = false;
    let mut status = None;
    loop {
        match sample_process(pid) {
            Ok(Some(sample)) => {
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
            Ok(None) => {}
            Err(error) => {
                #[cfg(target_os = "linux")]
                {
                    return Err(abort_after_spawn(
                        &mut child,
                        &capture,
                        stdout_thread,
                        stderr_thread,
                        &observed_descendants,
                        format!("Linux direct-child RSS monitoring unavailable: {error}"),
                    ));
                }
                #[cfg(not(target_os = "linux"))]
                {
                    return Err(abort_after_spawn(
                        &mut child,
                        &capture,
                        stdout_thread,
                        stderr_thread,
                        format!("direct-child monitoring unavailable: {error}"),
                    ));
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            match scan_descendants(pid) {
                Ok(descendants) => {
                    for descendant in descendants {
                        observed_descendants.insert(descendant.pid, descendant.start_time_ticks);
                    }
                    if !observed_descendants.is_empty() {
                        descendant_process_detected = true;
                    }
                }
                Err(error) => {
                    return Err(abort_after_spawn(
                        &mut child,
                        &capture,
                        stdout_thread,
                        stderr_thread,
                        &observed_descendants,
                        format!("Linux process-tree monitoring unavailable: {error}"),
                    ));
                }
            }
        }

        if capture.output_exceeded.load(Ordering::Relaxed) {
            output_exceeded = true;
        }
        let reader_error = match capture.reader_error.lock() {
            Ok(reader_error) => reader_error.clone(),
            Err(_) => Some("reader error state lock poisoned".to_string()),
        };
        if let Some(error) = reader_error {
            #[cfg(target_os = "linux")]
            {
                return Err(abort_after_spawn(
                    &mut child,
                    &capture,
                    stdout_thread,
                    stderr_thread,
                    &observed_descendants,
                    format!("output reader failed: {error}"),
                ));
            }
            #[cfg(not(target_os = "linux"))]
            {
                return Err(abort_after_spawn(
                    &mut child,
                    &capture,
                    stdout_thread,
                    stderr_thread,
                    format!("output reader failed: {error}"),
                ));
            }
        }
        let exit_status = match child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                #[cfg(target_os = "linux")]
                {
                    return Err(abort_after_spawn(
                        &mut child,
                        &capture,
                        stdout_thread,
                        stderr_thread,
                        &observed_descendants,
                        format!("poll evaluator: {error}"),
                    ));
                }
                #[cfg(not(target_os = "linux"))]
                {
                    return Err(abort_after_spawn(
                        &mut child,
                        &capture,
                        stdout_thread,
                        stderr_thread,
                        format!("poll evaluator: {error}"),
                    ));
                }
            }
        };
        if let Some(exit_status) = exit_status {
            status = Some(exit_status);
            #[cfg(target_os = "linux")]
            if !descendant_process_detected {
                match scan_descendants(pid) {
                    Ok(descendants) => {
                        for descendant in descendants {
                            observed_descendants
                                .insert(descendant.pid, descendant.start_time_ticks);
                        }
                        descendant_process_detected = !observed_descendants.is_empty();
                    }
                    Err(error) => {
                        return Err(abort_after_spawn(
                            &mut child,
                            &capture,
                            stdout_thread,
                            stderr_thread,
                            &observed_descendants,
                            format!("Linux process-tree monitoring unavailable: {error}"),
                        ));
                    }
                }
            }
            break;
        }

        if output_exceeded
            || rss_exceeded
            || start.elapsed() >= Duration::from_secs(limits.timeout_seconds)
            || descendant_process_detected
        {
            timed_out = !output_exceeded && !rss_exceeded && !descendant_process_detected;
            capture.stop_readers.store(true, Ordering::Relaxed);
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    let needs_cleanup = timed_out || rss_exceeded || output_exceeded || descendant_process_detected;
    let cleanup_error = if needs_cleanup {
        capture.stop_readers.store(true, Ordering::Relaxed);
        #[cfg(target_os = "linux")]
        let cleanup = cleanup_child(&mut child, &observed_descendants);
        #[cfg(not(target_os = "linux"))]
        let cleanup = cleanup_child(&mut child);
        let cleanup_status = cleanup.as_ref().ok().and_then(|result| *result);
        if status.is_none() {
            status = cleanup_status;
        }
        cleanup.err().map(|error| error.to_string())
    } else {
        None
    };

    if !needs_cleanup && status.is_none() {
        status = Some(
            child
                .wait()
                .map_err(|error| EvalError::Io(format!("reap evaluator: {error}")))?,
        );
    }
    capture.stop_readers.store(true, Ordering::Relaxed);
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();
    output_exceeded |= capture.output_exceeded.load(Ordering::Relaxed);

    if let Ok(Some(sample)) = sample_process(pid) {
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
        descendant_process_detected,
        reader_error,
        cleanup_error,
    })
}

#[cfg(target_os = "linux")]
fn abort_after_spawn(
    child: &mut Child,
    capture: &Arc<CaptureState>,
    stdout_thread: thread::JoinHandle<()>,
    stderr_thread: thread::JoinHandle<()>,
    observed_descendants: &HashMap<u32, u64>,
    message: String,
) -> EvalError {
    capture.stop_readers.store(true, Ordering::Relaxed);
    let cleanup_error = cleanup_child(child, observed_descendants)
        .err()
        .map(|error| error.to_string());
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();
    match cleanup_error {
        Some(error) => EvalError::Io(format!("{message}; cleanup failed: {error}")),
        None => EvalError::Io(message),
    }
}

#[cfg(not(target_os = "linux"))]
fn abort_after_spawn(
    child: &mut Child,
    capture: &Arc<CaptureState>,
    stdout_thread: thread::JoinHandle<()>,
    stderr_thread: thread::JoinHandle<()>,
    message: String,
) -> EvalError {
    capture.stop_readers.store(true, Ordering::Relaxed);
    let cleanup_error = cleanup_child(child).err().map(|error| error.to_string());
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();
    match cleanup_error {
        Some(error) => EvalError::Io(format!("{message}; cleanup failed: {error}")),
        None => EvalError::Io(message),
    }
}

#[cfg(target_os = "linux")]
fn read_stream<R: Read + AsRawFd>(
    mut reader: R,
    capture: Arc<CaptureState>,
    stdout: bool,
    maximum: u64,
) {
    if let Err(error) = set_nonblocking(reader.as_raw_fd()) {
        record_reader_error(&capture, format!("set output pipe nonblocking: {error}"));
        return;
    }
    read_stream_loop(&mut reader, capture, stdout, maximum);
}

#[cfg(not(target_os = "linux"))]
fn read_stream<R: Read>(mut reader: R, capture: Arc<CaptureState>, stdout: bool, maximum: u64) {
    read_stream_loop(&mut reader, capture, stdout, maximum);
}

fn read_stream_loop<R: Read>(
    reader: &mut R,
    capture: Arc<CaptureState>,
    stdout: bool,
    maximum: u64,
) {
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
                    capture.stop_readers.store(true, Ordering::Relaxed);
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
                    record_reader_error(&capture, "capture lock poisoned".to_string());
                    break;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            #[cfg(target_os = "linux")]
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if capture.stop_readers.load(Ordering::Relaxed) {
                    break;
                }
                thread::sleep(READER_POLL_INTERVAL);
            }
            Err(error) => {
                record_reader_error(&capture, error.to_string());
                break;
            }
        }
    }
}

fn record_reader_error(capture: &CaptureState, message: String) {
    if let Ok(mut reader_error) = capture.reader_error.lock() {
        *reader_error = Some(message);
    }
}

#[cfg(target_os = "linux")]
fn set_nonblocking(fd: std::os::fd::RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn scan_descendants(root_pid: u32) -> io::Result<Vec<ObservedProcess>> {
    let mut entries = HashMap::new();
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        if let Some(proc_entry) = read_proc_entry(pid)? {
            entries.insert(pid, proc_entry);
        }
    }
    let mut descendants = Vec::new();
    for (&pid, proc_entry) in &entries {
        if pid == root_pid {
            continue;
        }
        let mut current = proc_entry.parent_pid;
        let mut seen = HashSet::new();
        while current != 0 && seen.insert(current) {
            if current == root_pid {
                descendants.push(ObservedProcess {
                    pid,
                    start_time_ticks: proc_entry.start_time_ticks,
                });
                break;
            }
            current = entries.get(&current).map_or(0, |entry| entry.parent_pid);
        }
    }
    Ok(descendants)
}

#[cfg(target_os = "linux")]
fn read_proc_entry(pid: u32) -> io::Result<Option<ProcEntry>> {
    let stat = match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let end = stat.rfind(')').ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "malformed /proc stat command name",
        )
    })?;
    let fields: Vec<&str> = stat[end + 1..].split_whitespace().collect();
    let parent_pid = fields
        .get(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing parent pid"))?
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid parent pid"))?;
    let start_time_ticks = fields
        .get(19)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing process start time"))?
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid process start time"))?;
    Ok(Some(ProcEntry {
        parent_pid,
        start_time_ticks,
    }))
}

#[cfg(target_os = "linux")]
fn cleanup_child(
    child: &mut Child,
    observed_descendants: &HashMap<u32, u64>,
) -> io::Result<Option<ExitStatus>> {
    let pid = child.id();
    let status_before = match child.try_wait() {
        Ok(status) => status,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                error.kind(),
                format!("poll child during cleanup: {error}"),
            ));
        }
    };
    let mut cleanup_error = None;
    if status_before.is_none() {
        if let Err(error) = kill_owned_process_group(pid) {
            cleanup_error = Some(error);
            let _ = child.kill();
        }
    }
    for (&descendant_pid, &start_time_ticks) in observed_descendants {
        if let Err(error) = kill_if_same_process(descendant_pid, start_time_ticks) {
            cleanup_error.get_or_insert(error);
        }
    }
    let status = match status_before {
        Some(status) => Some(status),
        None => Some(child.wait()?),
    };

    let deadline = Instant::now() + CLEANUP_GRACE_PERIOD;
    loop {
        let remaining = live_observed_descendants(observed_descendants)?;
        if remaining.is_empty() {
            return cleanup_error.map_or(Ok(status), Err);
        }
        for (&descendant_pid, &start_time_ticks) in &remaining {
            if let Err(error) = kill_if_same_process(descendant_pid, start_time_ticks) {
                cleanup_error.get_or_insert(error);
            }
        }
        if Instant::now() >= deadline {
            let remaining_pids = remaining.keys().copied().collect::<Vec<_>>();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("descendant processes survived cleanup: {remaining_pids:?}"),
            ));
        }
        thread::sleep(READER_POLL_INTERVAL);
    }
}

#[cfg(not(target_os = "linux"))]
fn cleanup_child(child: &mut Child) -> io::Result<Option<ExitStatus>> {
    if child.try_wait()?.is_none() {
        child.kill()?;
    }
    Ok(Some(child.wait()?))
}

#[cfg(target_os = "linux")]
fn kill_owned_process_group(pid: u32) -> io::Result<()> {
    let process_group = unsafe { libc::getpgid(pid as libc::pid_t) };
    if process_group == -1 {
        return Ok(());
    }
    if process_group != pid as libc::pid_t {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing to kill a process group not owned by the evaluator child",
        ));
    }
    if unsafe { libc::kill(-process_group, libc::SIGKILL) } == -1 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn kill_if_same_process(pid: u32, start_time_ticks: u64) -> io::Result<()> {
    let Some(entry) = read_proc_entry(pid)? else {
        return Ok(());
    };
    if entry.start_time_ticks != start_time_ticks {
        return Ok(());
    }
    if unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) } == -1 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn live_observed_descendants(
    observed_descendants: &HashMap<u32, u64>,
) -> io::Result<HashMap<u32, u64>> {
    let mut live = HashMap::new();
    for (&pid, &start_time_ticks) in observed_descendants {
        if let Some(entry) = read_proc_entry(pid)? {
            if entry.start_time_ticks == start_time_ticks {
                live.insert(pid, start_time_ticks);
            }
        }
    }
    Ok(live)
}

fn sample_process(pid: u32) -> io::Result<Option<ProcSample>> {
    #[cfg(target_os = "linux")]
    {
        let stat = match fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => stat,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let end = stat.rfind(')').ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "malformed /proc stat command name",
            )
        })?;
        let fields: Vec<&str> = stat[end + 1..].split_whitespace().collect();
        let user_ticks = fields
            .get(11)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing user ticks"))?
            .parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid user ticks"))?;
        let system_ticks = fields
            .get(12)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing system ticks"))?
            .parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid system ticks"))?;
        let mut status = match fs::read_to_string(format!("/proc/{pid}/status")) {
            Ok(status) => status,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let high_water_rss_bytes = if let Some(high_water_rss_bytes) = parse_high_water_rss(&status)
        {
            Some(high_water_rss_bytes)
        } else if fields.first() == Some(&"Z") || process_status_is_zombie(&status) {
            None
        } else {
            let mut recovered = None;
            let mut zombie_seen = false;
            for _ in 0..8 {
                thread::sleep(Duration::from_millis(1));
                status = match fs::read_to_string(format!("/proc/{pid}/status")) {
                    Ok(status) => status,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
                    Err(error) => return Err(error),
                };
                if let Some(high_water_rss_bytes) = parse_high_water_rss(&status) {
                    recovered = Some(high_water_rss_bytes);
                    break;
                }
                if process_status_is_zombie(&status) {
                    zombie_seen = true;
                    break;
                }
            }
            match recovered {
                Some(high_water_rss_bytes) => Some(high_water_rss_bytes),
                None if zombie_seen => None,
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "VmHWM missing from live /proc status (stat_state={:?}, status_state={:?}, status_len={})",
                            fields.first(),
                            status.lines().find(|line| line.starts_with("State:")),
                            status.len()
                        ),
                    ));
                }
            }
        };
        Ok(Some(ProcSample {
            user_ticks,
            system_ticks,
            high_water_rss_bytes,
        }))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        Ok(None)
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

#[cfg(target_os = "linux")]
fn process_status_is_zombie(status: &str) -> bool {
    status.lines().any(|line| {
        line.strip_prefix("State:")
            .is_some_and(|state| state.trim_start().starts_with('Z'))
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
        if line.contains("Execution result:") {
            if parsed.is_some() {
                return Err("multiple evaluator result candidates".to_string());
            }
            parsed = Some(parse_result_line(line)?);
        }
    }
    parsed.ok_or_else(|| "no recognized evaluator result".to_string())
}

fn parse_result_line(line: &str) -> Result<ParsedResult, String> {
    let value = line
        .strip_prefix("INFO Execution result: ")
        .or_else(|| line.strip_prefix("Execution result: "))
        .ok_or_else(|| format!("unrecognized evaluator result line shape: {line:?}"))?;
    parse_result_value(value)
}

fn parse_result_value(value: &str) -> Result<ParsedResult, String> {
    if let Some(contents) = value
        .strip_prefix("Halt(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let (return_value, steps) = contents
            .split_once(", ")
            .ok_or_else(|| "Halt result must use Halt(<u32>, <u64>)".to_string())?;
        let return_value = return_value
            .parse::<u32>()
            .map_err(|_| "Halt return value is not a u32".to_string())?;
        let steps = steps
            .parse::<u64>()
            .map_err(|_| "Halt step count is not a u64".to_string())?;
        return Ok(ParsedResult {
            result_class: if return_value == 0 {
                "halt_success".to_string()
            } else {
                "halt_failure".to_string()
            },
            executed_steps: Some(steps),
            return_value: Some(return_value),
        });
    }
    if let Some(contents) = value
        .strip_prefix("LimitStepReached(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let steps = contents
            .parse::<u64>()
            .map_err(|_| "LimitStepReached step count is not a u64".to_string())?;
        return Ok(ParsedResult {
            result_class: "limit_reached".to_string(),
            executed_steps: Some(steps),
            return_value: None,
        });
    }
    Err(format!("unsupported evaluator result: {value}"))
}

fn prepare_artifact_targets(manifest_dir: &Path, specs: &[ArtifactSpec]) -> Vec<ArtifactTarget> {
    specs
        .iter()
        .cloned()
        .map(|spec| {
            let original = match manifest_relative_path(manifest_dir, &spec.path, "artifact") {
                Ok(path) => path,
                Err(error) => {
                    return ArtifactTarget {
                        spec,
                        original: PathBuf::from("<invalid>"),
                        canonical_target: PathBuf::from("<invalid>"),
                        existing: None,
                        pre_error: Some(error.to_string()),
                    }
                }
            };
            let parent = original.parent().unwrap_or_else(|| Path::new("."));
            let parent_result = validate_directory_components(parent, "artifact parent");
            if let Err(error) = parent_result {
                return ArtifactTarget {
                    spec,
                    original: original.clone(),
                    canonical_target: original,
                    existing: None,
                    pre_error: Some(error.to_string()),
                };
            }
            let canonical_parent = match parent.canonicalize() {
                Ok(path) => path,
                Err(error) => {
                    return ArtifactTarget {
                        spec,
                        original: original.clone(),
                        canonical_target: original,
                        existing: None,
                        pre_error: Some(format!("canonicalize artifact parent: {error}")),
                    }
                }
            };
            match validate_path_components(&original, "artifact", false) {
                Ok(Some(metadata)) if metadata.file_type().is_file() => {
                    let canonical = match original.canonicalize() {
                        Ok(path) => path,
                        Err(error) => {
                            return ArtifactTarget {
                                spec,
                                original: original.clone(),
                                canonical_target: canonical_parent
                                    .join(original.file_name().unwrap_or_default()),
                                existing: None,
                                pre_error: Some(format!("canonicalize artifact: {error}")),
                            }
                        }
                    };
                    let existing = ValidatedFile {
                        original: original.clone(),
                        canonical: canonical.clone(),
                        identity: file_identity(&canonical, &metadata),
                    };
                    ArtifactTarget {
                        spec,
                        original,
                        canonical_target: canonical,
                        existing: Some(existing),
                        pre_error: None,
                    }
                }
                Ok(Some(_)) => ArtifactTarget {
                    spec,
                    original: original.clone(),
                    canonical_target: canonical_parent
                        .join(original.file_name().unwrap_or_default()),
                    existing: None,
                    pre_error: Some("artifact is not a regular file".to_string()),
                },
                Ok(None) => ArtifactTarget {
                    spec,
                    original: original.clone(),
                    canonical_target: canonical_parent
                        .join(original.file_name().unwrap_or_default()),
                    existing: None,
                    pre_error: None,
                },
                Err(error) => ArtifactTarget {
                    spec,
                    original: original.clone(),
                    canonical_target: canonical_parent
                        .join(original.file_name().unwrap_or_default()),
                    existing: None,
                    pre_error: Some(error.to_string()),
                },
            }
        })
        .collect()
}

fn find_report_alias(
    report: &ReportDestination,
    protected: Vec<&ValidatedFile>,
    artifacts: &[ArtifactTarget],
) -> Option<String> {
    for file in protected {
        if report.canonical_target == file.canonical
            || report
                .identity
                .as_ref()
                .is_some_and(|identity| same_identity(identity, &file.identity))
        {
            return Some(file.canonical.display().to_string());
        }
    }
    for artifact in artifacts {
        if report.canonical_target == artifact.canonical_target {
            return Some(format!("artifact {}", artifact.spec.name));
        }
        if let (Some(report_identity), Some(file)) = (&report.identity, &artifact.existing) {
            if same_identity(report_identity, &file.identity) {
                return Some(format!("artifact {}", artifact.spec.name));
            }
        }
    }
    None
}

fn find_artifact_alias(
    artifacts: &[ArtifactTarget],
    protected: [&ValidatedFile; 3],
) -> Option<String> {
    for artifact in artifacts {
        for protected_file in protected {
            if artifact.canonical_target == protected_file.canonical
                || artifact
                    .existing
                    .as_ref()
                    .is_some_and(|file| same_identity(&file.identity, &protected_file.identity))
            {
                return Some(protected_file.canonical.display().to_string());
            }
        }
    }
    None
}

fn collect_artifacts(
    artifacts: &[ArtifactTarget],
    builder: &mut ReportBuilder,
    max_artifact_bytes: u64,
    max_total_artifact_bytes: u64,
    report_destination: &ReportDestination,
    protected: [&ValidatedFile; 3],
) {
    let mut total_bytes = 0_u64;
    for (index, artifact) in artifacts.iter().enumerate() {
        let report = collect_one_artifact(
            artifact,
            max_artifact_bytes,
            max_total_artifact_bytes,
            &mut total_bytes,
            report_destination,
            protected,
        );
        if let Some(error) = &report.error {
            builder.fail(
                if error.contains("hash mismatch") {
                    "artifact_hash_mismatch"
                } else if error.contains("size limit") {
                    "artifact_size_limit"
                } else if error.contains("read") {
                    "artifact_read_failure"
                } else {
                    "artifact_collection_failure"
                },
                format!("artifact {}: {error}", artifact.spec.name),
            );
        }
        builder.report.artifacts[index] = report;
    }
}

fn collect_one_artifact(
    artifact: &ArtifactTarget,
    max_artifact_bytes: u64,
    max_total_artifact_bytes: u64,
    total_bytes: &mut u64,
    report_destination: &ReportDestination,
    protected: [&ValidatedFile; 3],
) -> ArtifactReport {
    let mut report = empty_artifact_report(&artifact.spec);
    report.path = artifact.canonical_target.display().to_string();
    if let Some(error) = &artifact.pre_error {
        report.error = Some(error.clone());
        return report;
    }

    let current = match artifact.existing.clone() {
        Some(file) => file,
        None => match resolve_existing_absolute_file(&artifact.original, "artifact") {
            Ok(file) => file,
            Err(error) => {
                report.error = Some(error.to_string());
                return report;
            }
        },
    };
    if current.canonical != artifact.canonical_target {
        report.error = Some("artifact canonical path changed".to_string());
        return report;
    }
    let report_identity = report_destination.identity.clone().or_else(|| {
        let metadata = fs::symlink_metadata(&report_destination.path).ok()?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return None;
        }
        let canonical = report_destination.path.canonicalize().ok()?;
        Some(file_identity(&canonical, &metadata))
    });
    if current.canonical == report_destination.canonical_target
        || report_identity
            .as_ref()
            .is_some_and(|identity| same_identity(identity, &current.identity))
    {
        report.error = Some("artifact aliases the report path".to_string());
        return report;
    }
    if protected
        .into_iter()
        .any(|protected_file| same_identity(&current.identity, &protected_file.identity))
    {
        report.error = Some("artifact aliases a protected input".to_string());
        return report;
    }
    if let Err(error) = revalidate_file(&current, "artifact") {
        report.error = Some(error.to_string());
        return report;
    }

    let metadata = match fs::metadata(&current.canonical) {
        Ok(metadata) => metadata,
        Err(error) => {
            report.error = Some(format!("read artifact metadata: {error}"));
            return report;
        }
    };
    let remaining = max_total_artifact_bytes.saturating_sub(*total_bytes);
    let effective_limit = artifact.spec.max_size_bytes.min(max_artifact_bytes);
    if metadata.len() > effective_limit || metadata.len() > remaining {
        report.size_bytes = Some(metadata.len());
        report.error = Some(format!(
            "artifact size limit exceeded: {} bytes (per-artifact limit {}, aggregate remaining {})",
            metadata.len(), effective_limit, remaining
        ));
        *total_bytes = total_bytes.saturating_add(metadata.len());
        return report;
    }

    let mut file = match File::open(&current.canonical) {
        Ok(file) => file,
        Err(error) => {
            report.error = Some(format!(
                "read artifact {}: {error}",
                current.canonical.display()
            ));
            return report;
        }
    };
    let opened_metadata = match file.metadata() {
        Ok(metadata) => metadata,
        Err(error) => {
            report.error = Some(format!(
                "read artifact metadata {}: {error}",
                current.canonical.display()
            ));
            return report;
        }
    };
    if !opened_metadata.file_type().is_file() {
        report.error = Some("artifact is no longer a regular file".to_string());
        return report;
    }
    let opened_identity = file_identity(&current.canonical, &opened_metadata);
    if !same_identity(&opened_identity, &current.identity) {
        report.error = Some("artifact identity changed before reading".to_string());
        return report;
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    let mut size = 0_u64;
    loop {
        let read = match file.read(&mut buffer) {
            Ok(read) => read,
            Err(error) => {
                report.size_bytes = Some(size);
                report.error = Some(format!(
                    "read artifact {}: {error}",
                    current.canonical.display()
                ));
                return report;
            }
        };
        if read == 0 {
            break;
        }
        size = size.saturating_add(read as u64);
        if size > effective_limit || size.saturating_add(*total_bytes) > max_total_artifact_bytes {
            report.size_bytes = Some(size);
            report.error = Some("artifact size limit exceeded while streaming".to_string());
            *total_bytes = total_bytes.saturating_add(size);
            return report;
        }
        hasher.update(&buffer[..read]);
    }
    *total_bytes = total_bytes.saturating_add(size);
    let sha256 = format!("{:x}", hasher.finalize());
    report.size_bytes = Some(size);
    report.sha256 = Some(sha256.clone());
    if sha256 != artifact.spec.sha256 {
        report.error = Some(format!(
            "artifact hash mismatch: expected {}, got {}",
            artifact.spec.sha256, sha256
        ));
        return report;
    }
    report.complete = true;
    report
}

fn resolve_existing_absolute_file(path: &Path, label: &str) -> Result<ValidatedFile, EvalError> {
    let metadata = validate_path_components(path, label, true)?.ok_or_else(|| {
        EvalError::Preflight(format!("{label} {} is unavailable", path.display()))
    })?;
    if !metadata.file_type().is_file() {
        return Err(EvalError::Preflight(format!(
            "{label} {} is not a regular file",
            path.display()
        )));
    }
    let canonical = path.canonicalize().map_err(|error| {
        EvalError::Preflight(format!("canonicalize {label} {}: {error}", path.display()))
    })?;
    Ok(ValidatedFile {
        original: path.to_path_buf(),
        canonical: canonical.clone(),
        identity: file_identity(&canonical, &metadata),
    })
}

fn empty_integrity_report(path: String) -> IntegrityReport {
    IntegrityReport {
        path,
        pre_sha256: None,
        post_sha256: None,
        pre_size_bytes: None,
        post_size_bytes: None,
        pre_identity: None,
        post_identity: None,
        pre_error: None,
        post_error: None,
    }
}

fn set_integrity_pre(report: &mut IntegrityReport, snapshot: &FileSnapshot) {
    report.pre_sha256 = Some(snapshot.sha256.clone());
    report.pre_size_bytes = Some(snapshot.size_bytes);
    report.pre_identity = Some(identity_label(&snapshot.identity));
}

fn set_integrity_post(report: &mut IntegrityReport, snapshot: &FileSnapshot) {
    report.post_sha256 = Some(snapshot.sha256.clone());
    report.post_size_bytes = Some(snapshot.size_bytes);
    report.post_identity = Some(identity_label(&snapshot.identity));
}

fn empty_artifact_report(spec: &ArtifactSpec) -> ArtifactReport {
    ArtifactReport {
        name: spec.name.clone(),
        path: spec.path.clone(),
        expected_sha256: spec.sha256.clone(),
        max_size_bytes: spec.max_size_bytes,
        size_bytes: None,
        sha256: None,
        complete: false,
        error: None,
    }
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

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_report(path: &Path, report: &Report) -> Result<(), EvalError> {
    let parent = path
        .parent()
        .ok_or_else(|| EvalError::Io(format!("report path {} has no parent", path.display())))?;
    validate_directory_components(parent, "report parent")
        .map_err(|error| EvalError::Io(error.to_string()))?;
    let contents = serde_json::to_vec_pretty(report)
        .map_err(|error| EvalError::Io(format!("serialize report: {error}")))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("report");
    let nonce = unix_time_ms();
    let mut temporary = None;
    let mut file = None;
    for attempt in 0..REPORT_TEMP_ATTEMPTS {
        let counter = REPORT_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{file_name}.tmp-{}-{nonce}-{counter}-{attempt}",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(opened) => {
                temporary = Some(candidate);
                file = Some(opened);
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(EvalError::Io(format!(
                    "create unique temporary report in {}: {error}",
                    parent.display()
                )))
            }
        }
    }
    let temporary = temporary.ok_or_else(|| {
        EvalError::Io(format!(
            "could not allocate a unique temporary report beside {}",
            path.display()
        ))
    })?;
    let mut file = file.expect("temporary report file set with path");
    let write_result = (|| {
        file.write_all(&contents)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok::<(), io::Error>(())
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(EvalError::Io(format!(
            "publish report {}: {error}",
            path.display()
        )));
    }
    Ok(())
}
