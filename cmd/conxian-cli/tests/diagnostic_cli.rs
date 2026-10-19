use std::process::Command;

#[test]
fn default_doctor_reports_configuration_scope_and_exits_unsuccessfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .arg("doctor")
        .env_clear()
        .output()
        .unwrap();
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("connectivity not tested"));
    assert!(text.contains("not a strength test"));
    assert!(!text.contains("Ready for deployment"));
}

#[test]
fn invalid_status_base_fails_without_echoing_the_input() {
    let output = Command::new(env!("CARGO_BIN_EXE_conxian-cli"))
        .arg("status")
        .env_clear()
        .env("GATEWAY_URL", "file:///private-synthetic-input")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!combined.contains("private-synthetic-input"));
}
