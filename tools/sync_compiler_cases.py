#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SOURCE_DIR = Path("/Users/aricsu/Database/Compiler/benchmarks/cases")
DEFAULT_TARGET_DIR = ROOT / "cases" / "regression"
DEFAULT_COMPILER_BIN = ROOT / "target" / "debug" / "compiler-rs"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Sync regression cases from /Users/aricsu/Database/Compiler benchmark cases."
    )
    parser.add_argument("--source-dir", type=Path, default=DEFAULT_SOURCE_DIR)
    parser.add_argument("--target-dir", type=Path, default=DEFAULT_TARGET_DIR)
    parser.add_argument("--compiler-bin", type=Path, default=DEFAULT_COMPILER_BIN)
    args = parser.parse_args()

    source_dir = args.source_dir.resolve()
    target_dir = args.target_dir.resolve()
    compiler_bin = args.compiler_bin.resolve()

    if not source_dir.exists():
        raise SystemExit(f"source dir not found: {source_dir}")
    if not compiler_bin.exists():
        raise SystemExit(f"compiler bin not found: {compiler_bin}")

    generated = build_regression_cases(source_dir, compiler_bin)
    write_regression_cases(target_dir, generated)
    print(f"synced {len(generated)} regression cases into {target_dir}")
    return 0


def build_regression_cases(source_dir: Path, compiler_bin: Path) -> list[dict[str, Any]]:
    generated: list[dict[str, Any]] = []
    for source_path in sorted(source_dir.glob("*.json")):
        source_case = json.loads(source_path.read_text(encoding="utf-8"))
        request = convert_source_case(source_case)
        response = analyze_request(request, compiler_bin)
        base_name = sanitize_name(source_case["case_id"])

        if request["scope"] == "line":
            outputs = sorted(response["outputs"], key=lambda item: item["canonical"]["subject_id"])
            for output in outputs:
                subject_id = output["canonical"]["subject_id"]
                generated.append(
                    {
                        "name": sanitize_name(subject_id),
                        "scope": "line",
                        "subject_id": subject_id,
                        "series": request["series"],
                        "expected_top_events": top_event_kinds(output),
                    }
                )
            continue

        outputs = sorted(response["outputs"], key=lambda item: item["canonical"]["subject_id"])
        for output in outputs:
            subject_id = output["canonical"]["subject_id"]
            case_name = base_name if len(outputs) == 1 else f"{base_name}__{sanitize_name(subject_id)}"
            generated.append(
                {
                    "name": case_name,
                    "scope": "group",
                    "subject_id": subject_id,
                    "groups": request["groups"],
                    "expected_top_events": top_event_kinds(output),
                }
            )

    return generated


def convert_source_case(source_case: dict[str, Any]) -> dict[str, Any]:
    collection = source_case["collection"]
    group_by = list(collection.get("group_by", []))
    metric_id = derive_metric_id(source_case, collection)

    if not group_by:
        series = []
        for index, item in enumerate(collection["series_list"]):
            entity_id = source_case["case_id"] if len(collection["series_list"]) == 1 else f"{source_case['case_id']}/{index + 1}"
            series.append(
                {
                    "metric_id": metric_id,
                    "entity_id": entity_id,
                    "group_id": source_case["case_id"],
                    "labels": normalize_labels(item.get("labels", {})),
                    "points": item["points"],
                }
            )
        return {"scope": "line", "series": series}

    group_keys = group_by[:-1] if len(group_by) > 1 else group_by
    grouped_members: dict[str, list[dict[str, Any]]] = {}
    for index, item in enumerate(collection["series_list"]):
        labels = item.get("labels", {})
        group_id = "/".join(labels.get(key, "") for key in group_keys) or source_case["case_id"]
        entity_id = "/".join(labels.get(key, str(index + 1)) for key in group_by) or f"member-{index + 1}"
        grouped_members.setdefault(group_id, []).append(
            {
                "metric_id": metric_id,
                "entity_id": entity_id,
                "group_id": group_id,
                "labels": normalize_labels(labels),
                "points": item["points"],
            }
        )

    groups = [
        {
            "metric_id": metric_id,
            "group_id": group_id,
            "members": members,
        }
        for group_id, members in sorted(grouped_members.items())
    ]
    return {"scope": "group", "groups": groups}


def derive_metric_id(source_case: dict[str, Any], collection: dict[str, Any]) -> str:
    context = collection.get("context", {})
    return context.get("source_id") or collection.get("panel_id") or source_case["case_id"]


def normalize_labels(labels: dict[str, str]) -> list[list[str]]:
    return [[key, value] for key, value in sorted(labels.items())]


def analyze_request(request: dict[str, Any], compiler_bin: Path) -> dict[str, Any]:
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as handle:
        json.dump(request, handle, ensure_ascii=False)
        request_path = Path(handle.name)

    try:
        completed = subprocess.run(
            [str(compiler_bin), "analyze-file", str(request_path)],
            check=True,
            capture_output=True,
            text=True,
            cwd=ROOT,
        )
        return json.loads(completed.stdout)
    finally:
        request_path.unlink(missing_ok=True)


def top_event_kinds(output: dict[str, Any]) -> list[str]:
    return [event["kind"] for event in output["canonical"].get("top_events", [])]


def write_regression_cases(target_dir: Path, cases: list[dict[str, Any]]) -> None:
    if target_dir.exists():
        shutil.rmtree(target_dir)
    target_dir.mkdir(parents=True, exist_ok=True)

    for index, case in enumerate(cases, start=1):
        filename = f"{index:03d}-{case['name']}.json"
        path = target_dir / filename
        path.write_text(json.dumps(case, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def sanitize_name(value: str) -> str:
    normalized = re.sub(r"[^a-zA-Z0-9_-]+", "_", value).strip("_")
    return normalized or "case"


if __name__ == "__main__":
    raise SystemExit(main())
