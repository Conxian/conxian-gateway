import sys

with open('cmd/gateway/tests/api_tests.rs', 'r') as f:
    lines = f.readlines()

new_lines = []
for line in lines:
    if '"BitVm": {"prover_id": "p1", "commitment_hash": "c1", "state_root": "r1",' in line:
        new_lines.append('        "type": "BitVm",\n')
        new_lines.append('        "data": {"prover_id": "p1", "commitment_hash": "c1", "state_root": "r1", "root_hash": "0xabc..."}\n')
    elif '"root_hash": "0xabc..." ' in line and '"BitVm"' not in line:
        # Skip the old root_hash line if it was part of the object
        continue
    else:
        new_lines.append(line)

with open('cmd/gateway/tests/api_tests.rs', 'w') as f:
    f.writelines(new_lines)
