import os
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

# Scan only production source roots (avoid docs and scripts).
INCLUDE_DIRS = ["cmd", "internal", "pkg"]

# Scan only production code/config file types.
INCLUDE_EXTENSIONS = [".rs", ".toml"]

# Keywords that indicate non-production logic or placeholders.
CONTAMINATION_KEYWORDS = ["changeme", "stub", "placeholder"]

# 'mock' is allowed if it's part of a feature flag or test, but we'll flag direct use.
SENSITIVE_KEYWORDS = ["mock"]

EXCLUDE_DIRS = [
    ".git",
    "target",
    "tests",
    "test",
    "scripts",
    "docs",
    ".vscode",
    "__pycache__",
]

def should_scan_file(filepath: Path) -> bool:
    if filepath.suffix.lower() not in INCLUDE_EXTENSIONS:
        return False

    for part in filepath.parts:
        if part in EXCLUDE_DIRS:
            return False

    return True


def check_file(filepath: Path) -> bool:
    try:
        content = filepath.read_text(encoding="utf-8", errors="ignore")
        content_lower = content.lower()

        for kw in CONTAMINATION_KEYWORDS:
            if kw in content_lower:
                print(
                    f"CONTAMINATION FAILURE: Found prohibited keyword '{kw}' in {filepath}"
                )
                return True

        for kw in SENSITIVE_KEYWORDS:
            if kw not in content_lower:
                continue

            # Special check for 'mock': allowed in cfg(test) or gated behind a feature,
            # but flagged if it looks like a hardcoded implementation fallback.
            lines = content.splitlines()
            for i, line in enumerate(lines):
                if kw not in line.lower():
                    continue

                if filepath.name.endswith("_tests.rs"):
                    continue

                gated = False
                for j in range(max(0, i - 15), i + 1):
                    if ("#[cfg" in lines[j] or "#[test]" in lines[j]) and (
                        "test" in lines[j] or "mock-integrations" in lines[j]
                    ):
                        gated = True
                        break

                if "mock-integrations" in line:
                    gated = True

                if not gated and '"' in line:
                    print(
                        f"SENSITIVE WARNING: Found '{kw}' in non-test file {filepath}:{i+1} - {line.strip()}"
                    )
    except Exception as e:
        print(f"CONTAMINATION FAILURE: Failed to scan file {filepath}: {e}", file=sys.stderr)
        return True
    return False

def main():
    print("Running contamination guard...")
    failed = False

    include_paths = [REPO_ROOT / p for p in INCLUDE_DIRS]
    include_paths = [p for p in include_paths if p.exists()]

    scanned_files = 0
    for start_dir in include_paths:
        for root, dirs, files in os.walk(start_dir):
            dirs[:] = sorted([d for d in dirs if d not in EXCLUDE_DIRS])
            for file in sorted(files):
                filepath = Path(root) / file
                if not should_scan_file(filepath):
                    continue

                scanned_files += 1
                if check_file(filepath):
                    failed = True

    if failed:
        print("Contamination guard failed. Please remove stubs/placeholders from production paths.")
        return 1

    if scanned_files == 0:
        print(
            "Contamination guard did not scan any files; check INCLUDE_DIRS/INCLUDE_EXTENSIONS configuration."
        )
        return 1

    print(f"Production paths are clean. ({scanned_files} files scanned)")
    return 0

if __name__ == "__main__":
    sys.exit(main())
