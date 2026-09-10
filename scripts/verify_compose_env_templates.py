#!/usr/bin/env python3
import sys
import re
from pathlib import Path

def extract_env_vars_from_example(env_path: Path) -> set:
    vars_found = set()
    content = env_path.read_text(encoding="utf-8")
    for line in content.splitlines():
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            var_name = line.split("=", 1)[0].strip()
            vars_found.add(var_name)
    return vars_found

def extract_env_vars_from_compose(compose_path: Path) -> set:
    vars_found = set()
    content = compose_path.read_text(encoding="utf-8")
    # Matches ${VAR_NAME} or ${VAR_NAME:-default}
    pattern = re.compile(r'\$\{([A-Z0-9_]+)(?::-[^}]*)?\}')
    for match in pattern.finditer(content):
        vars_found.add(match.group(1))
    return vars_found

def main():
    print("Verifying Docker Compose & Env templates...")
    compose_file = Path("docker-compose.yml")
    env_example = Path(".env.example")

    if not compose_file.exists():
        print("ERROR: docker-compose.yml missing")
        return 1

    if not env_example.exists():
        print("ERROR: .env.example missing")
        return 1

    example_vars = extract_env_vars_from_example(env_example)
    compose_vars = extract_env_vars_from_compose(compose_file)

    print(f"  Found {len(example_vars)} variables in .env.example")
    print(f"  Found {len(compose_vars)} distinct variables referenced in docker-compose.yml")

    missing_in_example = compose_vars - example_vars
    # Allow explicit aliases or lane-specific prefixes if needed
    known_lane_aliases = {"SOVEREIGN_API_TOKEN", "BUSINESS_API_TOKEN", "ENTERPRISE_API_TOKEN"}
    unresolved = missing_in_example - known_lane_aliases

    if unresolved:
        print(f"ERROR: Variables referenced in docker-compose.yml missing from .env.example: {unresolved}")
        return 1

    print("Env template verification passed successfully.")
    return 0

if __name__ == "__main__":
    sys.exit(main())
