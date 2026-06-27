import sys

with open('internal/api/src/handlers.rs', 'r') as f:
    lines = f.readlines()

new_lines = []
for line in lines:
    if 'StatusCode::UNPROCESSABLE_ENTITY' in line:
        new_lines.append(line.replace('StatusCode::UNPROCESSABLE_ENTITY', 'StatusCode::BAD_REQUEST'))
    else:
        new_lines.append(line)

with open('internal/api/src/handlers.rs', 'w') as f:
    f.writelines(new_lines)
