#!/usr/bin/env python3
import sys
import subprocess

ALLOWLISTED_ENV_FILES = {".env.example"}

def main():
    print("Checking for prohibited tracked artifacts...")
    prohibited_patterns = [
        "target/",
        "node_modules/",
        ".next/",
        "dist/",
        "build/",
        "__pycache__/",
        "*.pyc",
        "*.pyo",
        "*.pyd",
        ".pytest_cache/",
        "playwright-report/",
        "*.bak",
        "*.tmp",
        "gateway_state.json",
        ".gateway_state.json.transaction.lock",
        ".gateway_state.json.ownership.lock",
        "offline_queue.db",
        "*.sqlite",
        "test-results/",
        "coverage/",
        ".coverage",
        "htmlcov/",
        "*.lcov",
        "*.key",
        "*.pem",
        "*.pfx",
        "*.p12",
        "*.secret",
        "*.keystore",
    ]

    found = False
    for pattern in prohibited_patterns:
        result = subprocess.run(["git", "ls-files", pattern], capture_output=True, text=True)
        if result.stdout.strip():
            print(f"ERROR: Prohibited artifact tracked: {result.stdout.strip()}")
            found = True

    # Check environment files specifically, allowing .env.example
    env_result = subprocess.run(["git", "ls-files", ".env*"], capture_output=True, text=True)
    if env_result.stdout.strip():
        tracked_env_files = env_result.stdout.strip().splitlines()
        for env_file in tracked_env_files:
            file_name = env_file.split("/")[-1]
            if file_name not in ALLOWLISTED_ENV_FILES:
                print(f"ERROR: Prohibited environment file tracked: {env_file}")
                found = True

    if found:
        return 1
    print("No prohibited artifacts tracked.")
    return 0

if __name__ == "__main__":
    sys.exit(main())
