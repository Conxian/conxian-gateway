#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
from typing import Dict, Tuple

DEFAULT_SCOPE_FILES = [
    "internal/api/src/lightning.rs",
    "internal/api/src/x402.rs",
    "pkg/conxian-core/src/lightning.rs",
]


def normalize_path(value: str) -> str:
    return value.replace('\\\\', '/').replace('\\', '/')


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate scoped Lightning coverage summary")
    parser.add_argument("--input", required=True, help="cargo llvm-cov JSON report path")
    parser.add_argument("--threshold", type=float, default=90.0, help="Fail-under percentage")
    parser.add_argument("--output-dir", required=True, help="Directory for generated summaries")
    args = parser.parse_args()

    input_path = Path(args.input)
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    report = json.loads(input_path.read_text())

    per_file: Dict[str, Tuple[int, int]] = {scope: (0, 0) for scope in DEFAULT_SCOPE_FILES}

    for data_entry in report.get("data", []):
        for file_entry in data_entry.get("files", []):
            filename = normalize_path(file_entry.get("filename", ""))
            summary = file_entry.get("summary", {}).get("lines", {})
            covered = int(summary.get("covered", 0))
            total = int(summary.get("count", 0))

            for scope in DEFAULT_SCOPE_FILES:
                if filename.endswith(scope):
                    current_covered, current_total = per_file[scope]
                    per_file[scope] = (current_covered + covered, current_total + total)

    total_covered = sum(covered for covered, _ in per_file.values())
    total_count = sum(total for _, total in per_file.values())
    measured = (total_covered / total_count * 100.0) if total_count else 0.0
    status = "pass" if total_count > 0 and measured >= args.threshold else "fail"

    file_rows = []
    for scope in DEFAULT_SCOPE_FILES:
        covered, total = per_file[scope]
        percent = (covered / total * 100.0) if total else 0.0
        file_rows.append(
            {
                "path": scope,
                "covered": covered,
                "total": total,
                "percent": round(percent, 2),
            }
        )

    summary_json = {
        "scopedTarget": f">={args.threshold:g}",
        "measured": f"{measured:.2f}%",
        "status": status,
        "scopeFiles": DEFAULT_SCOPE_FILES,
        "files": file_rows,
    }

    (output_dir / "summary.json").write_text(json.dumps(summary_json, indent=2) + "\n")

    lines = [
        "# Lightning Scoped Coverage Summary",
        "",
        f"- Target: **>={args.threshold:g}%**",
        f"- Measured: **{measured:.2f}%**",
        f"- Status: **{status.upper()}**",
        "",
        "| File | Covered Lines | Total Lines | Coverage |",
        "| --- | ---: | ---: | ---: |",
    ]

    for row in file_rows:
        lines.append(
            f"| `{row['path']}` | {row['covered']} | {row['total']} | {row['percent']:.2f}% |"
        )

    if total_count == 0:
        lines.extend(["", "No scoped files were found in the llvm-cov report."])

    lines.append("")

    (output_dir / "summary.md").write_text("\n".join(lines))
    (output_dir / "summary.txt").write_text(
        f"target=>={args.threshold:g}% measured={measured:.2f}% status={status}\n"
    )

    print(f"Lightning scoped coverage: {measured:.2f}% (target >= {args.threshold:g}%) -> {status}")

    return 0 if status == "pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())
