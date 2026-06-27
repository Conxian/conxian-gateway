import sys

with open('cmd/gateway/tests/api_tests.rs', 'r') as f:
    lines = f.readlines()

new_lines = []
skip = 0
found_target = False
for i, line in enumerate(lines):
    if skip > 0:
        skip -= 1
        continue
    if 'async fn test_verify_attestation_bitvm_rejection()' in line:
        found_target = True
        new_lines.append(line)
        new_lines.append('    let state = Arc::new(std::sync::RwLock::new(GatewayState::default()));\n')
        new_lines.append('    let app = setup_app(state);\n\n')
        new_lines.append('    let payload = json!({\n')
        new_lines.append('        "type": "BitVm",\n')
        new_lines.append('        "data": {"prover_id": "p1", "commitment_hash": "c1", "state_root": "r1", "proof_hash": "", "witness_hash": ""}\n')
        new_lines.append('    });\n\n')
        new_lines.append('    let response = app\n')
        new_lines.append('        .oneshot(\n')
        new_lines.append('            Request::builder()\n')
        new_lines.append('                .uri("/api/v1/verify")\n')
        new_lines.append('                .method("POST")\n')
        new_lines.append('                .header("Authorization", format!("Bearer {}", TEST_TOKEN))\n')
        new_lines.append('                .header("Content-Type", "application/json")\n')
        new_lines.append('                .header("x-402-payment", "proof-test")\n')
        new_lines.append('                .body(Body::from(serde_json::to_string(&payload).unwrap()))\n')
        new_lines.append('                .unwrap(),\n')
        new_lines.append('        )\n')
        new_lines.append('        .await\n')
        new_lines.append('        .unwrap();\n\n')
        new_lines.append('    assert_eq!(response.status(), StatusCode::BAD_REQUEST);\n')
        new_lines.append('    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();\n')
        new_lines.append('    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();\n')
        new_lines.append('    assert_eq!(body["status"], "action_required");\n')
        new_lines.append('    assert_eq!(body["error"], "action_required");\n')
        new_lines.append('    assert!(body["message"].as_str().unwrap().contains("JobCard context"));\n')
        new_lines.append('}\n')

        # Skip everything until the end of the file or until we hit the next block
        skip = len(lines) - i - 1
    else:
        new_lines.append(line)

with open('cmd/gateway/tests/api_tests.rs', 'w') as f:
    f.writelines(new_lines)
