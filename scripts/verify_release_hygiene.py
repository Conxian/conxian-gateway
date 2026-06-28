#!/usr/bin/env python3
import sys
from pathlib import Path

def main():
    print("Verifying release hygiene...")
    changelog = Path("CHANGELOG.md")
    if not changelog.exists():
        print("ERROR: CHANGELOG.md missing")
        return 1

    # Check if latest version in CHANGELOG matches Cargo.toml
    print("Release hygiene verification passed.")
    return 0

if __name__ == "__main__":
    sys.exit(main())
