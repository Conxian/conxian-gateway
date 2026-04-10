import sys
import os

# Keywords that indicate non-production logic or placeholders
CONTAMINATION_KEYWORDS = ["CHANGEME", "stub", "placeholder", "testnet"]
# 'mock' is allowed if it's part of a feature flag or test, but we'll flag direct use
SENSITIVE_KEYWORDS = ["mock"]

EXCLUDE_DIRS = [".git", "target", "tests", "scripts", ".vscode", "docs"]
EXCLUDE_FILES = ["CONTRIBUTING.md", "ENHANCEMENT_PLAN.md", "PRD.md", "MIGRATION.md", "SAB_MIGRATION.md", ".env.example"]

def check_file(filepath):
    try:
        with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
            content = f.read()
            for kw in CONTAMINATION_KEYWORDS:
                if kw in content:
                    print(f"CONTAMINATION FAILURE: Found prohibited keyword '{kw}' in {filepath}")
                    return True

            # Special check for 'mock' - allowed in cfg(test) or cfg(feature = "mock-integrations")
            # but flagged if it looks like a hardcoded implementation fallback on prod path
            if "mock" in content.lower():
                # This is a bit complex for a simple script, so we'll just be conservative
                # and allow it if it's gated by #[cfg(...)
                lines = content.splitlines()
                for i, line in enumerate(lines):
                    if "mock" in line.lower():
                        # Rough check for gating
                        gated = False
                        if i > 0 and "#[cfg" in lines[i-1]:
                            gated = True
                        if "mock-integrations" in line:
                            gated = True

                        if not gated and not filepath.endswith("_tests.rs") and "mock" in line.lower():
                             # If it's in a string literal in code, it might be a problem
                             if '"' in line and "mock" in line.lower():
                                 print(f"SENSITIVE WARNING: Found 'mock' in non-test file {filepath}:{i+1} - {line.strip()}")
                                 # We won't fail CI for 'mock' yet to avoid too many false positives,
                                 # but 'CHANGEME' is a hard fail.
    except Exception:
        pass
    return False

def main():
    print("Running contamination guard...")
    failed = False
    for root, dirs, files in os.walk("."):
        dirs[:] = [d for d in dirs if d not in EXCLUDE_DIRS]
        for file in files:
            if file in EXCLUDE_FILES:
                continue

            filepath = os.path.join(root, file)
            if check_file(filepath):
                failed = True

    if failed:
        print("Contamination guard failed. Please remove stubs/placeholders from production paths.")
        return 1

    print("Production paths are clean.")
    return 0

if __name__ == "__main__":
    sys.exit(main())
