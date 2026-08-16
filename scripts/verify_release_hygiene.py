#!/usr/bin/env python3
import sys
import re
from pathlib import Path

def main():
    print("Verifying release hygiene...")
    cargo_toml = Path("Cargo.toml")
    if not cargo_toml.exists():
        print("ERROR: Cargo.toml missing")
        return 1

    try:
        if sys.version_info >= (3, 11):
            import tomllib
            with open(cargo_toml, "rb") as f:
                data = tomllib.load(f)
        else:
            # Fallback regex parsing if tomllib not built-in (Python < 3.11)
            content = cargo_toml.read_text(encoding="utf-8")
            match = re.search(r'\[workspace\.package\][\s\S]*?version\s*=\s*"([^"]+)"', content)
            if not match:
                print("ERROR: Could not find workspace.package.version in Cargo.toml")
                return 1
            data = {"workspace": {"package": {"version": match.group(1)}}}

        version = data.get("workspace", {}).get("package", {}).get("version")
        if not version:
            print("ERROR: workspace.package.version missing in Cargo.toml")
            return 1
    except Exception as e:
        print(f"ERROR: Failed to parse Cargo.toml: {e}")
        return 1

    changelog = Path("CHANGELOG.md")
    if not changelog.exists():
        print("ERROR: CHANGELOG.md missing")
        return 1

    changelog_content = changelog.read_text(encoding="utf-8")
    version_header = f"## [v{version}]"
    if version_header not in changelog_content:
        print(f"ERROR: CHANGELOG.md does not contain required version entry '{version_header}'")
        return 1

    print(f"Release hygiene verification passed (version v{version} verified in CHANGELOG.md).")
    return 0

if __name__ == "__main__":
    sys.exit(main())
