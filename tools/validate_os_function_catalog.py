#!/usr/bin/env python3
"""Validate the GlobusOS OS function audit catalog without claiming implementation."""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CATALOG_PATH = ROOT / "docs" / "OS_FUNCTION_CATALOG.json"
CONTRACT_PATH = ROOT / "docs" / "OS_FUNCTION_CONTRACT_SCHEMA.json"
MARKDOWN_PATH = ROOT / "docs" / "OS_FUNCTION_CATALOG.md"
ID_PATTERN = re.compile(r"^ATC-FUNC-OS-[A-Z0-9]+-[0-9]{3}$")
MARKDOWN_ID_PATTERN = re.compile(r"ATC-FUNC-OS-[A-Z0-9]+-[0-9]{3}")
REQUIRED_RECORD_FIELDS = {"id", "domain", "function", "priority", "status", "traceability"}
REQUIRED_CONTRACT_FIELDS = {
    "id", "domain", "function", "purpose", "owner_boundary", "priority", "inputs",
    "outputs", "preconditions", "postconditions", "invariants", "errors",
    "authorization", "concurrency", "resource_limits", "audit", "interfaces",
    "tests", "evidence", "source_binding", "status", "traceability", "residuals",
}


def validate_documents(catalog: dict[str, Any], contract: dict[str, Any], markdown: str) -> list[str]:
    errors: list[str] = []
    functions = catalog.get("functions")
    if not isinstance(functions, list) or not functions:
        return ["catalog.functions must be a non-empty array"]

    status_model = catalog.get("status_model")
    traceability_states = catalog.get("traceability_states")
    if not isinstance(status_model, list) or not status_model:
        errors.append("catalog.status_model must be a non-empty array")
        status_model = []
    if not isinstance(traceability_states, list) or not traceability_states:
        errors.append("catalog.traceability_states must be a non-empty array")
        traceability_states = []

    seen: set[str] = set()
    for index, record in enumerate(functions):
        label = f"catalog.functions[{index}]"
        if not isinstance(record, dict):
            errors.append(f"{label} must be an object")
            continue
        missing = REQUIRED_RECORD_FIELDS - record.keys()
        if missing:
            errors.append(f"{label} missing required fields: {', '.join(sorted(missing))}")
        function_id = record.get("id")
        if not isinstance(function_id, str) or not ID_PATTERN.fullmatch(function_id):
            errors.append(f"{label}.id is not a canonical ATC-FUNC-OS identifier")
        elif function_id in seen:
            errors.append(f"duplicate function ID: {function_id}")
        else:
            seen.add(function_id)
        if record.get("priority") not in {"P0", "P1", "P2"}:
            errors.append(f"{label}.priority must be P0, P1, or P2")
        if record.get("status") not in status_model:
            errors.append(f"{label}.status is not declared in catalog.status_model")
        if record.get("traceability") not in traceability_states:
            errors.append(f"{label}.traceability is not declared in catalog.traceability_states")

    contract_fields = contract.get("required_function_fields")
    if not isinstance(contract_fields, dict):
        errors.append("contract.required_function_fields must be an object")
    else:
        missing = REQUIRED_CONTRACT_FIELDS - contract_fields.keys()
        if missing:
            errors.append(f"contract schema missing required fields: {', '.join(sorted(missing))}")
    for field in ("contract_status_values", "traceability_values", "source_binding_identity"):
        value = contract.get(field)
        if not isinstance(value, list) or not value or len(value) != len(set(value)):
            errors.append(f"contract.{field} must be a non-empty list of unique values")
    if contract.get("implementation_claim") is not False:
        errors.append("contract.implementation_claim must remain false")

    markdown_ids = MARKDOWN_ID_PATTERN.findall(markdown)
    if len(markdown_ids) != len(set(markdown_ids)):
        errors.append("Markdown catalog contains duplicate function IDs")
    if set(markdown_ids) != seen:
        missing_from_md = sorted(seen - set(markdown_ids))
        missing_from_json = sorted(set(markdown_ids) - seen)
        if missing_from_md:
            errors.append(f"IDs missing from Markdown catalog: {', '.join(missing_from_md)}")
        if missing_from_json:
            errors.append(f"IDs missing from JSON catalog: {', '.join(missing_from_json)}")
    return errors


def main() -> int:
    try:
        catalog = json.loads(CATALOG_PATH.read_text(encoding="utf-8"))
        contract = json.loads(CONTRACT_PATH.read_text(encoding="utf-8"))
        markdown = MARKDOWN_PATH.read_text(encoding="utf-8")
    except (OSError, json.JSONDecodeError) as exc:
        print(f"catalog validation could not load input: {exc}", file=sys.stderr)
        return 2

    errors = validate_documents(catalog, contract, markdown)
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print(f"Catalog validation passed: {len(catalog['functions'])} unique function IDs; no implementation status inferred.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
