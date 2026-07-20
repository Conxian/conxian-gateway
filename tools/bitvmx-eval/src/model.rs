use serde::{Deserialize, Serialize};

pub const WARNING: &str =
    "Experimental BitVMX evaluation only; unaudited; not valid for settlement.";
pub const MANIFEST_SCHEMA_VERSION: &str = "bitvmx-eval-manifest-v2";
pub const REPORT_SCHEMA_VERSION: &str = "bitvmx-eval-report-v2";
pub const BACKEND: &str = "bitvmx-cpu";
pub const UPSTREAM_REVISION: &str = "d390832c8e0f2a01453e8ef4bf65dbe715fb9236";
pub const UPSTREAM_REVISION_LINE: &str = "d390832c8e0f2a01453e8ef4bf65dbe715fb9236\n";
pub const DEFAULT_MAX_RSS_BYTES: u64 = 2_684_354_560;
pub const DEFAULT_SMALL_TIMEOUT_SECONDS: u64 = 300;
pub const DEFAULT_SCALED_TIMEOUT_SECONDS: u64 = 600;
pub const DEFAULT_MAX_OUTPUT_BYTES: u64 = 1_073_741_824;
pub const DEFAULT_MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
pub const DEFAULT_MAX_TOTAL_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;
pub const HARD_MAX_ARTIFACT_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_HEX_INPUT_BYTES: usize = 16 * 1024 * 1024;
pub const LINUX_RESOURCE_SCOPE: &str =
    "linux-direct-child-only-with-descendant-detection-fail-closed";
pub const NON_LINUX_RESOURCE_SCOPE: &str = "non-linux-direct-child-only-weaker-mode";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: String,
    pub manifest_version: u32,
    pub warning: String,
    pub experimental: bool,
    pub production_supported: bool,
    pub cryptographic_verification: bool,
    pub backend: String,
    pub upstream_revision: String,
    pub executable: ExecutableSpec,
    pub fixture: FixtureSpec,
    pub execution: ExecutionSpec,
    pub limits: LimitsSpec,
    pub sandbox: SandboxSpec,
    #[serde(default)]
    pub artifacts: Vec<ArtifactSpec>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutableSpec {
    pub path: String,
    pub sha256: String,
    pub revision_file: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureSpec {
    pub id: String,
    pub path: String,
    pub sha256: String,
    pub kind: String,
    pub expected_result_class: String,
    pub expected_return_value: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionSpec {
    pub workload: String,
    pub input_hex: Option<String>,
    pub limit_steps: Option<u64>,
    pub trace: bool,
    pub no_hash: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LimitsSpec {
    pub max_rss_bytes: Option<u64>,
    pub timeout_seconds: Option<u64>,
    pub max_output_bytes: Option<u64>,
    pub max_artifact_bytes: Option<u64>,
    pub max_total_artifact_bytes: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxSpec {
    pub mode: String,
    pub network_policy: String,
    pub resource_scope: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactSpec {
    pub name: String,
    pub path: String,
    pub sha256: String,
    pub max_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema_version: String,
    pub warning: String,
    pub experimental: bool,
    pub production_supported: bool,
    pub cryptographic_verification: bool,
    pub backend: String,
    pub upstream_revision: String,
    pub report_path: String,
    pub resource_scope: String,
    pub descendant_process_detected: bool,
    pub executable: IntegrityReport,
    pub fixture: FixtureReport,
    pub revision: RevisionReport,
    pub expected_result_class: String,
    pub expected_return_value: Option<u32>,
    pub actual_result_class: Option<String>,
    pub actual_return_value: Option<u32>,
    pub executed_command: String,
    pub arguments: Vec<String>,
    pub started_at_unix_ms: u128,
    pub wall_time_ms: Option<u64>,
    pub cpu_user_time_us: Option<u64>,
    pub cpu_system_time_us: Option<u64>,
    pub maximum_rss_bytes: Option<u64>,
    pub executed_steps: Option<u64>,
    pub outputs: Vec<OutputReport>,
    pub artifacts: Vec<ArtifactReport>,
    pub environment: EnvironmentReport,
    pub exit_status: Option<ExitStatusReport>,
    pub proof_size_bytes: Option<u64>,
    pub proof_size_reason: String,
    pub failure: Option<String>,
    pub failure_details: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntegrityReport {
    pub path: String,
    pub pre_sha256: Option<String>,
    pub post_sha256: Option<String>,
    pub pre_size_bytes: Option<u64>,
    pub post_size_bytes: Option<u64>,
    pub pre_identity: Option<String>,
    pub post_identity: Option<String>,
    pub pre_error: Option<String>,
    pub post_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixtureReport {
    pub id: String,
    pub kind: String,
    pub integrity: IntegrityReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevisionReport {
    pub path: String,
    pub expected_bytes_hex: String,
    pub pre_observed_bytes_hex: Option<String>,
    pub pre_exact: bool,
    pub pre_identity: Option<String>,
    pub pre_error: Option<String>,
    pub post_observed_bytes_hex: Option<String>,
    pub post_exact: bool,
    pub post_identity: Option<String>,
    pub post_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutputReport {
    pub name: String,
    pub path: Option<String>,
    pub size_bytes: u64,
    pub sha256: String,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactReport {
    pub name: String,
    pub path: String,
    pub expected_sha256: String,
    pub max_size_bytes: u64,
    pub size_bytes: Option<u64>,
    pub sha256: Option<String>,
    pub complete: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnvironmentReport {
    pub os: String,
    pub architecture: String,
    pub kernel: Option<String>,
    pub rustc: Option<String>,
    pub cargo: Option<String>,
    pub wrapper_version: String,
    pub profile: String,
    pub cpu_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExitStatusReport {
    pub success: bool,
    pub code: Option<i32>,
    pub signal: Option<i32>,
}
