#!/usr/bin/env python3
import sys
import subprocess

def main():
    print("Checking for prohibited tracked artifacts...")
    # Check for common build artifacts that should not be tracked
    prohibited = ["target/", "node_modules/", ".next/", "*.bak", "*.tmp"]

    found = False
    for pattern in prohibited:
        result = subprocess.run(["git", "ls-files", pattern], capture_output=True, text=True)
        if result.stdout.strip():
            print(f"ERROR: Prohibited artifact tracked: {result.stdout.strip()}")
            found = True

    if found:
        return 1
    print("No prohibited artifacts tracked.")
    return 0

if __name__ == "__main__":
    sys.exit(main())
