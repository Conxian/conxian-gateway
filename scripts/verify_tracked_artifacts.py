#!/usr/bin/env python3
import sys
import subprocess
import fnmatch
from pathlib import Path

ALLOWLISTED_ENV_FILES = {".env.example"}

PROHIBITED_DIRS = {
    "node_modules",
    "test-results",
    "playwright-report",
    "dist",
    "build",
    "target",
    ".next",
    "coverage",
    "htmlcov",
    "__pycache__",
    ".pytest_cache",
}

PROHIBITED_FILE_PATTERNS = [
    "*.pyc",
    "*.pyo",
    "*.pyd",
    "*.lcov",
    "*.bak",
    "*.tmp",
    "*.key",
    "*.pem",
    "*.pfx",
    "*.p12",
    "*.secret",
    "*.keystore",
    "*.db",
    "*.sqlite",
    "gateway_state.json",
    ".gateway_state.json.transaction.lock",
    ".gateway_state.json.ownership.lock",
    "offline_queue.db",
    ".coverage",
]


def main():
    print("Checking for prohibited tracked artifacts...")
    ls_result = subprocess.run(["git", "ls-files"], capture_output=True, text=True, check=True)
    tracked_files = ls_result.stdout.strip().splitlines()

    found = False
    for file_path in tracked_files:
        path_obj = Path(file_path)
        parts = path_obj.parts
        filename = path_obj.name

        # 1. Check directory components across all nesting levels
        if any(part in PROHIBITED_DIRS for part in parts[:-1]):
            print(f"ERROR: Prohibited artifact tracked in directory: {file_path}")
            found = True
            continue

        # 2. Check environment file patterns at any nesting level
        if filename.startswith(".env"):
            if filename not in ALLOWLISTED_ENV_FILES:
                print(f"ERROR: Prohibited environment file tracked: {file_path}")
                found = True
                continue

        # 3. Check file patterns / extensions
        for pattern in PROHIBITED_FILE_PATTERNS:
            if fnmatch.fnmatch(filename, pattern):
                print(f"ERROR: Prohibited artifact tracked: {file_path}")
                found = True
                break

    if found:
        return 1
    print("No prohibited artifacts tracked.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
