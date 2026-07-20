#![cfg(feature = "bitvmx-eval")]

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        OnceLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::fs::hard_link;
#[cfg(unix)]
use std::os::unix::fs::symlink;

use sha2::{Digest, Sha256};

use conxian_bitvmx_eval::model::{
    BACKEND, LINUX_RESOURCE_SCOPE, MANIFEST_SCHEMA_VERSION, REPORT_SCHEMA_VERSION,
    UPSTREAM_REVISION,
};
use conxian_bitvmx_eval::{
    run_manifest, sha256_file, ArtifactSpec, ExecutableSpec, ExecutionSpec, FixtureSpec,
    LimitsSpec, Manifest, SandboxSpec, WARNING,
};

struct Harness {
    root: PathBuf,
    helper: PathBuf,
}

static HARNESS: OnceLock<Harness> = OnceLock::new();
static CASE_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn harness() -> &'static Harness {
    HARNESS.get_or_init(|| {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let root = env::temp_dir().join(format!("conxian-bitvmx-eval-{nonce}"));
        fs::create_dir_all(&root).expect("create test root");
        let helper = root.join("synthetic-helper");
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/helper.rs");
        let status = Command::new("rustc")
            .args([
                "--edition=2021",
                source.to_str().expect("fixture path"),
                "-O",
                "-o",
            ])
            .arg(&helper)
            .status()
            .expect("invoke rustc for source fixture");
        assert!(status.success(), "synthetic helper compilation failed");
        Harness { root, helper }
    })
}

fn activate_external_sandbox_marker() {
    env::set_var("BITVMX_EVAL_SANDBOX_ACTIVE", "1");
    env::set_var("BITVMX_EVAL_SANDBOX_MODE", "network-deny");
}

fn base_manifest(helper: &Path, revision_file: &Path, fixture: &Path, expected: &str) -> Manifest {
    Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        manifest_version: 2,
        warning: WARNING.to_string(),
        experimental: true,
        production_supported: false,
        cryptographic_verification: false,
        backend: BACKEND.to_string(),
        upstream_revision: UPSTREAM_REVISION.to_string(),
        executable: ExecutableSpec {
            path: helper.display().to_string(),
            sha256: sha256_file(helper).expect("hash helper"),
            revision_file: revision_file.display().to_string(),
        },
        fixture: FixtureSpec {
            id: "synthetic-source-helper".to_string(),
            path: fixture.display().to_string(),
            sha256: sha256_file(fixture).expect("hash fixture"),
            kind: "synthetic-source".to_string(),
            expected_result_class: expected.to_string(),
            expected_return_value: match expected {
                "halt_success" => Some(0),
                "halt_failure" => Some(7),
                "limit_reached" => None,
                other => panic!("unsupported test expectation: {other}"),
            },
        },
        execution: ExecutionSpec {
            workload: "small".to_string(),
            input_hex: Some("00ff".to_string()),
            limit_steps: Some(100),
            trace: false,
            no_hash: true,
        },
        limits: LimitsSpec {
            max_rss_bytes: None,
            timeout_seconds: None,
            max_output_bytes: None,
            max_artifact_bytes: None,
            max_total_artifact_bytes: None,
        },
        sandbox: SandboxSpec {
            mode: "external-preflight".to_string(),
            network_policy: "deny".to_string(),
            resource_scope: LINUX_RESOURCE_SCOPE.to_string(),
        },
        artifacts: Vec::new(),
    }
}

fn case(name: &str, scenario: &str, expected: &str) -> (PathBuf, PathBuf, PathBuf) {
    activate_external_sandbox_marker();
    let harness = harness();
    let index = CASE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let directory = harness.root.join(format!("{name}-{index}"));
    fs::create_dir_all(&directory).expect("create case directory");
    let helper = directory.join("synthetic-helper");
    fs::copy(&harness.helper, &helper).expect("copy helper into case");
    let fixture = directory.join("fixture.source");
    fs::write(&fixture, scenario).expect("write fixture");
    let revision_file = directory.join("synthetic-helper.revision");
    fs::write(&revision_file, format!("{UPSTREAM_REVISION}\n")).expect("write revision sidecar");
    let manifest_path = directory.join("manifest.json");
    let report_path = directory.join("report.json");
    let manifest = base_manifest(&helper, &revision_file, &fixture, expected);
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("serialize manifest"),
    )
    .expect("write manifest");
    (manifest_path, report_path, fixture)
}

fn rewrite_manifest(path: &Path, update: impl FnOnce(&mut Manifest)) {
    let contents = fs::read_to_string(path).expect("read manifest");
    let mut manifest: Manifest = serde_json::from_str(&contents).expect("parse manifest");
    update(&mut manifest);
    fs::write(
        path,
        serde_json::to_vec_pretty(&manifest).expect("serialize manifest"),
    )
    .expect("rewrite manifest");
}

fn manifest_value(path: &Path) -> Manifest {
    serde_json::from_slice(&fs::read(path).expect("read manifest")).expect("parse manifest")
}

fn artifact_path(fixture: &Path) -> PathBuf {
    fixture
        .parent()
        .expect("fixture parent")
        .join("artifact.bin")
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn add_artifact(manifest: &Path, artifact: &Path, expected_sha256: String, max_size_bytes: u64) {
    rewrite_manifest(manifest, |manifest| {
        manifest.artifacts.push(ArtifactSpec {
            name: "artifact".to_string(),
            path: artifact.display().to_string(),
            sha256: expected_sha256,
            max_size_bytes,
        });
    });
}

fn report_json(report: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(report).expect("read report")).expect("parse report")
}

fn rejected(manifest: &Path, report: &Path, expected: &str) -> serde_json::Value {
    let error = run_manifest(manifest, report).expect_err("evaluation should reject");
    let message = error.to_string();
    assert!(
        message.contains(expected),
        "{message} does not contain {expected}"
    );
    assert!(
        report.is_file(),
        "failure report was not published: {message}"
    );
    let json = report_json(report);
    assert_eq!(json["experimental"], true);
    assert_eq!(json["production_supported"], false);
    assert_eq!(json["cryptographic_verification"], false);
    assert!(
        json["failure"]
            .as_str()
            .is_some_and(|failure| failure.contains(expected)),
        "report failure does not contain {expected}: {json}"
    );
    json
}

#[test]
fn success_report_has_non_production_contract_and_integrity_pairs() {
    let (manifest, report, _) = case("success", "success", "halt_success");
    let result = run_manifest(&manifest, &report).expect("success fixture should run");
    assert_eq!(result.schema_version, REPORT_SCHEMA_VERSION);
    assert_eq!(result.warning, WARNING);
    assert!(result.experimental);
    assert!(!result.production_supported);
    assert!(!result.cryptographic_verification);
    assert_eq!(result.resource_scope, LINUX_RESOURCE_SCOPE);
    assert_eq!(result.expected_result_class, "halt_success");
    assert_eq!(result.actual_result_class.as_deref(), Some("halt_success"));
    assert_eq!(result.executed_steps, Some(7));
    assert!(result.failure.is_none());
    assert_eq!(
        result.executable.pre_sha256, result.executable.post_sha256,
        "executable pre/post hash must be recorded"
    );
    assert_eq!(
        result.fixture.integrity.pre_sha256, result.fixture.integrity.post_sha256,
        "fixture pre/post hash must be recorded"
    );
    assert!(result.revision.pre_exact);
    assert!(result.revision.post_exact);
    assert_eq!(
        result.revision.pre_observed_bytes_hex,
        result.revision.post_observed_bytes_hex
    );
    assert!(result.proof_size_bytes.is_none());
    assert_eq!(result.proof_size_reason, "not_applicable_cpu_backend");
    let report_json = report_json(&report);
    assert_eq!(report_json["schema_version"], REPORT_SCHEMA_VERSION);
    assert_eq!(report_json["proof_size_bytes"], serde_json::Value::Null);
}

#[test]
fn short_lived_child_exit_race_is_not_process_setup_failure() {
    for iteration in 0..8 {
        let (manifest, report, _) = case(
            &format!("short-lived-child-{iteration}"),
            "short-lived",
            "halt_success",
        );
        let result = run_manifest(&manifest, &report)
            .unwrap_or_else(|error| panic!("short-lived iteration {iteration} failed: {error}"));
        assert_eq!(result.actual_result_class.as_deref(), Some("halt_success"));
        assert_eq!(result.actual_return_value, Some(0));
        assert_eq!(result.executed_steps, Some(7));
        assert!(
            result.failure.is_none(),
            "short-lived iteration {iteration} unexpectedly failed: {:?}",
            result.failure
        );

        let json = report_json(&report);
        assert_eq!(json["actual_result_class"], "halt_success");
        assert_eq!(json["actual_return_value"], 0);
        assert_eq!(json["exit_status"]["success"], true);
        assert_eq!(json["exit_status"]["code"], 0);
        assert_eq!(json["failure"], serde_json::Value::Null);
        assert!(
            !json["failure_details"]
                .as_array()
                .expect("failure details array")
                .iter()
                .any(|detail| detail
                    .as_str()
                    .is_some_and(|detail| detail.contains("process_setup_failure"))),
            "short-lived iteration {iteration} recorded process setup failure: {json}"
        );
    }
}

#[test]
fn expected_halt_failure_is_classified_without_verification() {
    let (manifest, report, _) = case("expected-failure", "failure", "halt_failure");
    let result = run_manifest(&manifest, &report).expect("expected failure class is valid");
    assert_eq!(result.actual_result_class.as_deref(), Some("halt_failure"));
    assert!(result.failure.is_none());
}

#[test]
fn positive_limit_reached_result_is_accepted() {
    let (manifest, report, _) = case("limit", "limit", "limit_reached");
    let result = run_manifest(&manifest, &report).expect("limit result should be accepted");
    assert_eq!(result.actual_result_class.as_deref(), Some("limit_reached"));
    assert_eq!(result.executed_steps, Some(100));
}

#[test]
fn missing_binary_publishes_failure_report() {
    let (manifest, report, _) = case("missing-binary", "success", "halt_success");
    rewrite_manifest(&manifest, |manifest| {
        manifest.executable.path = manifest
            .executable
            .path
            .replace("synthetic-helper", "missing-helper");
    });
    rejected(&manifest, &report, "executable_path_invalid");
}

#[test]
fn revision_requires_exact_pinned_bytes() {
    for (name, contents) in [
        ("revision-no-newline", UPSTREAM_REVISION.as_bytes().to_vec()),
        (
            "revision-whitespace",
            format!(" {UPSTREAM_REVISION}\n").into_bytes(),
        ),
    ] {
        let (manifest, report, _) = case(name, "success", "halt_success");
        let revision = manifest_value(&manifest).executable.revision_file;
        fs::write(revision, contents).expect("rewrite revision sidecar");
        rejected(&manifest, &report, "revision_mismatch");
        let json = report_json(&report);
        assert!(!json["revision"]["pre_exact"].as_bool().unwrap_or(true));
        assert!(json["revision"]["pre_observed_bytes_hex"].is_string());
    }
}

#[test]
fn executable_and_fixture_hash_mismatches_publish_reports() {
    let (manifest, report, _) = case("hash-mismatch", "success", "halt_success");
    rewrite_manifest(&manifest, |manifest| {
        manifest.executable.sha256 = "00".repeat(32);
    });
    rejected(&manifest, &report, "executable_hash_mismatch");

    let (manifest, report, fixture) = case("corrupted-fixture", "success", "halt_success");
    fs::write(fixture, "corrupted-after-manifest").expect("corrupt fixture");
    rejected(&manifest, &report, "fixture_hash_mismatch");
}

#[test]
fn malformed_spoofed_and_multiple_result_candidates_are_rejected() {
    let (manifest, report, _) = case("malformed-output", "malformed", "halt_success");
    rejected(&manifest, &report, "malformed_or_unrecognized_output");

    let (manifest, report, _) = case("spoofed-output", "spoof", "halt_success");
    rejected(&manifest, &report, "malformed_or_unrecognized_output");
}

#[test]
fn unexpected_return_value_with_same_result_class_is_rejected() {
    let (manifest, report, _) = case("same-class-return", "same-class-return", "halt_failure");
    rejected(&manifest, &report, "unexpected_return_value");
}

#[test]
fn process_failures_publish_reports() {
    let (manifest, report, _) = case("nonzero-exit", "nonzero", "halt_success");
    rejected(&manifest, &report, "nonzero_exit");

    let (manifest, report, _) = case("timeout", "timeout", "halt_success");
    rewrite_manifest(&manifest, |manifest| {
        manifest.limits.timeout_seconds = Some(1);
    });
    rejected(&manifest, &report, "timeout");

    let (manifest, report, _) = case("output-limit", "output", "halt_success");
    rewrite_manifest(&manifest, |manifest| {
        manifest.limits.max_output_bytes = Some(1024);
    });
    rejected(&manifest, &report, "output_limit");

    let (manifest, report, _) = case("rss-limit", "rss", "halt_success");
    rewrite_manifest(&manifest, |manifest| {
        manifest.limits.max_rss_bytes = Some(8 * 1024 * 1024);
        manifest.limits.timeout_seconds = Some(2);
    });
    rejected(&manifest, &report, "rss_limit");
}

#[test]
fn descendant_processes_fail_closed_and_are_reported() {
    let (manifest, report, _) = case("descendant", "descendant", "halt_success");
    let json = rejected(&manifest, &report, "descendant_process_detected");
    assert_eq!(json["descendant_process_detected"], true);
    assert_eq!(json["resource_scope"], LINUX_RESOURCE_SCOPE);
}

#[test]
fn post_run_mutation_and_deletion_publish_integrity_details() {
    let (manifest, report, _) = case("mutate-fixture", "mutate-fixture", "halt_success");
    let json = rejected(&manifest, &report, "fixture_changed_during_execution");
    assert!(json["fixture"]["integrity"]["pre_sha256"].is_string());
    assert!(json["fixture"]["integrity"]["post_sha256"].is_string());
    assert_ne!(
        json["fixture"]["integrity"]["pre_sha256"],
        json["fixture"]["integrity"]["post_sha256"]
    );

    let (manifest, report, _) = case("delete-fixture", "delete-fixture", "halt_success");
    let json = rejected(&manifest, &report, "fixture_postrun_integrity");
    assert!(json["fixture"]["integrity"]["post_error"].is_string());

    let (manifest, report, _) = case("mutate-revision", "mutate-revision", "halt_success");
    let json = rejected(
        &manifest,
        &report,
        "upstream_revision_changed_during_execution",
    );
    assert!(json["revision"]["pre_exact"].as_bool().unwrap_or(false));
    assert!(!json["revision"]["post_exact"].as_bool().unwrap_or(true));
    assert_ne!(
        json["revision"]["pre_observed_bytes_hex"],
        json["revision"]["post_observed_bytes_hex"]
    );
}

#[test]
fn executable_deletion_publishes_postrun_failure_report() {
    let (manifest, report, _) = case("delete-executable", "delete-executable", "halt_success");
    let json = rejected(&manifest, &report, "executable_postrun_integrity");
    assert!(json["executable"]["post_error"].is_string());
}

#[test]
fn artifact_hash_size_missing_and_read_failures_are_bounded_and_reported() {
    let (manifest, report, fixture) = case("artifact-hash", "success", "halt_success");
    let artifact = artifact_path(&fixture);
    fs::write(&artifact, b"artifact-content").expect("write artifact");
    add_artifact(&manifest, &artifact, "00".repeat(32), 1024);
    rejected(&manifest, &report, "artifact_hash_mismatch");

    let (manifest, report, fixture) = case("artifact-size", "success", "halt_success");
    let artifact = artifact_path(&fixture);
    fs::write(&artifact, b"oversized").expect("write artifact");
    add_artifact(
        &manifest,
        &artifact,
        sha256_file(&artifact).expect("hash artifact"),
        4,
    );
    rejected(&manifest, &report, "artifact_size_limit");

    let (manifest, report, fixture) = case("artifact-missing", "success", "halt_success");
    let artifact = artifact_path(&fixture);
    add_artifact(&manifest, &artifact, "00".repeat(32), 1024);
    rejected(&manifest, &report, "artifact_collection_failure");

    let (manifest, report, fixture) = case("artifact-directory", "success", "halt_success");
    let artifact = artifact_path(&fixture);
    fs::create_dir(&artifact).expect("create directory artifact");
    add_artifact(&manifest, &artifact, "00".repeat(32), 1024);
    rejected(&manifest, &report, "artifact_path_invalid");

    let (manifest, report, fixture) = case("artifact-delete", "delete-artifact", "halt_success");
    let artifact = artifact_path(&fixture);
    fs::write(&artifact, b"artifact-content").expect("write artifact");
    add_artifact(
        &manifest,
        &artifact,
        sha256_file(&artifact).expect("hash artifact"),
        1024,
    );
    rejected(&manifest, &report, "artifact_collection_failure");

    let (manifest, report, fixture) = case(
        "artifact-report-alias",
        "artifact-report-alias",
        "halt_success",
    );
    let artifact = artifact_path(&fixture);
    add_artifact(&manifest, &artifact, sha256_bytes(b"not-a-report"), 1024);
    rejected(&manifest, &report, "artifact_collection_failure");
}

#[test]
fn wrong_sandbox_markers_are_rejected_before_execution() {
    let (manifest, report, _) = case("wrong-sandbox", "success", "halt_success");
    rewrite_manifest(&manifest, |manifest| {
        manifest.sandbox.mode = "missing-enforcement".to_string();
    });
    let error = run_manifest(&manifest, &report).expect_err("sandbox must be rejected");
    assert!(error.to_string().contains("sandbox"));
    assert!(
        !report.exists(),
        "invalid preflight must not publish a report"
    );
}

#[test]
fn missing_sandbox_markers_are_rejected_before_report_creation() {
    let (manifest, report, _) = case("missing-sandbox", "success", "halt_success");
    let cli = env::current_exe()
        .expect("test executable path")
        .parent()
        .and_then(Path::parent)
        .expect("target debug directory")
        .join("bitvmx-eval");
    assert!(cli.is_file(), "CLI binary missing: {}", cli.display());
    let output = Command::new(cli)
        .args([
            "--manifest",
            manifest.to_str().expect("manifest path"),
            "--report",
            report.to_str().expect("report path"),
        ])
        .env_remove("BITVMX_EVAL_SANDBOX_ACTIVE")
        .env_remove("BITVMX_EVAL_SANDBOX_MODE")
        .output()
        .expect("run CLI without sandbox markers");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("external network-deny sandbox required"));
    assert!(!report.exists());
}

#[cfg(unix)]
#[test]
fn symlink_and_nonregular_path_inputs_are_rejected_with_failure_reports() {
    let (manifest, report, fixture) = case("symlink-executable", "success", "halt_success");
    let manifest_data = manifest_value(&manifest);
    let helper = PathBuf::from(manifest_data.executable.path);
    let link = fixture
        .parent()
        .expect("fixture parent")
        .join("helper-link");
    symlink(&helper, &link).expect("create executable symlink");
    rewrite_manifest(&manifest, |manifest| {
        manifest.executable.path = link.display().to_string();
    });
    rejected(&manifest, &report, "executable_path_invalid");

    let (manifest, report, fixture) = case("symlink-component", "success", "halt_success");
    let manifest_data = manifest_value(&manifest);
    let helper = PathBuf::from(manifest_data.executable.path);
    let target_dir = helper.parent().expect("helper parent");
    let linked_dir = fixture.parent().expect("fixture parent").join("linked-dir");
    symlink(target_dir, &linked_dir).expect("create directory symlink");
    rewrite_manifest(&manifest, |manifest| {
        manifest.executable.path = linked_dir.join("synthetic-helper").display().to_string();
    });
    rejected(&manifest, &report, "executable_path_invalid");

    let (manifest, report, fixture) = case("nonregular-executable", "success", "halt_success");
    rewrite_manifest(&manifest, |manifest| {
        manifest.executable.path = fixture
            .parent()
            .expect("fixture parent")
            .display()
            .to_string();
    });
    rejected(&manifest, &report, "executable_path_invalid");
}

#[cfg(unix)]
#[test]
fn report_aliases_are_rejected_without_overwriting_inputs() {
    let (manifest, report, _fixture) = case("report-alias", "success", "halt_success");
    let manifest_data = manifest_value(&manifest);
    let helper = PathBuf::from(manifest_data.executable.path);
    let helper_before = fs::read(&helper).expect("read helper");
    let error = run_manifest(&manifest, &helper).expect_err("report must not alias executable");
    assert!(error.to_string().contains("aliases"));
    assert!(!report.exists());
    assert_eq!(
        fs::read(&helper).expect("read helper after alias rejection"),
        helper_before
    );

    let (manifest, _report, fixture) = case("report-hardlink-alias", "success", "halt_success");
    let manifest_data = manifest_value(&manifest);
    let helper = PathBuf::from(manifest_data.executable.path);
    let hardlink_report = fixture
        .parent()
        .expect("fixture parent")
        .join("hardlink-report.json");
    hard_link(&helper, &hardlink_report).expect("create hard-link report alias");
    let error = run_manifest(&manifest, &hardlink_report).expect_err("hard-link alias must reject");
    assert!(error.to_string().contains("aliases"));
}

#[test]
fn artifact_alias_and_report_replacement_are_safe() {
    let (manifest, report, _fixture) = case("artifact-alias", "success", "halt_success");
    add_artifact(&manifest, &report, "00".repeat(32), 1024);
    let error = run_manifest(&manifest, &report).expect_err("artifact/report alias must reject");
    assert!(error.to_string().contains("aliases"));
    assert!(!report.exists());

    let (manifest, report, _) = case("report-replacement", "success", "halt_success");
    run_manifest(&manifest, &report).expect("first report write");
    run_manifest(&manifest, &report).expect("second report write");
    assert_eq!(report_json(&report)["failure"], serde_json::Value::Null);
}
