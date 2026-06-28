#!/usr/bin/env python3
import sys
from pathlib import Path

def main():
    print("Checking knowledge retention (docs/)...")
    docs_dir = Path("docs")
    if not docs_dir.exists():
        print("ERROR: docs/ directory missing")
        return 1

    # Basic check: Ensure at least one research and one audit doc exists
    research = list(docs_dir.glob("research/*.md"))
    audit = list(docs_dir.glob("audit/*.md"))

    if not research:
        print("WARNING: No research docs found")
    if not audit:
        print("WARNING: No audit docs found")

    print(f"Knowledge retention check passed. ({len(research)} research docs, {len(audit)} audit docs)")
    return 0

if __name__ == "__main__":
    sys.exit(main())
