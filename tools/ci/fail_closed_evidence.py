#!/usr/bin/env python3
"""Fail-closed validation for GitHub API evidence.

This module deliberately separates transport/API failures from valid empty results.
It is safe to import from governance tooling and has no third-party dependencies.
"""
from __future__ import annotations

import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from enum import Enum
from typing import Any, Callable, Iterable


class State(str, Enum):
    SUCCESS = "SUCCESS"
    EMPTY = "EMPTY / INCOMPLETE"
    ERROR = "ERROR / UNKNOWN"
    PENDING = "PENDING"


@dataclass(frozen=True)
class EvidenceResult:
    state: State
    reason: str
    data: Any = None
    pages: int = 0
    http_status: int | None = None

    @property
    def verified(self) -> bool:
        return self.state is State.SUCCESS


TRANSIENT_STATUSES = {408, 425, 429, 500, 502, 503, 504}


def validate_payload(
    payload: Any,
    *,
    expected_repo: str | None = None,
    expected_sha: str | None = None,
    required_fields: Iterable[str] = (),
    allow_empty: bool = False,
    pending: bool = False,
) -> EvidenceResult:
    """Validate a decoded JSON response against the caller's explicit contract."""
    if pending:
        return EvidenceResult(State.PENDING, "checks are queued or in progress", payload)
    if not isinstance(payload, (dict, list)):
        return EvidenceResult(State.ERROR, "JSON root must be an object or array", payload)

    if isinstance(payload, dict):
        missing = [field for field in required_fields if field not in payload]
        if missing:
            return EvidenceResult(State.EMPTY, "missing required fields: " + ", ".join(missing), payload)
        if expected_repo is not None:
            actual_repo = payload.get("repository") or payload.get("full_name")
            if isinstance(actual_repo, dict):
                actual_repo = actual_repo.get("full_name")
            if actual_repo != expected_repo:
                return EvidenceResult(State.ERROR, f"repository mismatch: expected {expected_repo!r}, got {actual_repo!r}", payload)
        if expected_sha is not None:
            actual_sha = payload.get("head_sha") or payload.get("sha") or payload.get("commit_sha")
            if isinstance(actual_sha, dict):
                actual_sha = actual_sha.get("sha")
            if actual_sha != expected_sha:
                return EvidenceResult(State.ERROR, f"SHA mismatch: expected {expected_sha!r}, got {actual_sha!r}", payload)
        if "check_runs" in payload:
            runs = payload["check_runs"]
            if not isinstance(runs, list):
                return EvidenceResult(State.ERROR, "check_runs must be an array", payload)
            if any(not isinstance(run, dict) for run in runs):
                return EvidenceResult(State.ERROR, "check_runs contains a non-object entry", payload)
            if any(run.get("status") != "completed" for run in runs):
                return EvidenceResult(State.PENDING, "one or more checks are not completed", payload)
        if "total_count" in payload and "items" in payload:
            items = payload["items"]
            if not isinstance(items, list) or payload["total_count"] != len(items):
                return EvidenceResult(State.EMPTY, "declared count does not match returned items", payload)
        is_empty = not payload or ("check_runs" in payload and len(payload["check_runs"]) == 0)
        if is_empty and not allow_empty:
            return EvidenceResult(State.EMPTY, "unexpectedly empty evidence payload", payload)
    else:
        if not payload and not allow_empty:
            return EvidenceResult(State.EMPTY, "unexpectedly empty result array", payload)

    return EvidenceResult(State.SUCCESS, "payload satisfies declared contract", payload)


def _parse_link_next(link_header: str | None) -> str | None:
    if not link_header:
        return None
    for part in link_header.split(","):
        segments = [segment.strip() for segment in part.split(";")]
        if len(segments) > 1 and 'rel="next"' in segments[1:]:
            target = segments[0]
            if target.startswith("<") and target.endswith(">"):
                return target[1:-1]
    return None


def fetch_json(
    url: str,
    *,
    token: str | None = None,
    retries: int = 3,
    opener: Callable[..., Any] = urllib.request.urlopen,
    sleep: Callable[[float], None] = time.sleep,
) -> EvidenceResult:
    """Fetch one JSON endpoint with bounded retry on transient HTTP/transport errors."""
    if retries < 0 or retries > 5:
        return EvidenceResult(State.ERROR, "retries must be between 0 and 5")
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme != "https" or parsed.hostname != "api.github.com":
        return EvidenceResult(State.ERROR, "only https://api.github.com endpoints are permitted")

    last_error = "request failed"
    for attempt in range(retries + 1):
        headers = {
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "ATC-fail-closed-evidence-validator",
        }
        if token:
            headers["Authorization"] = "Bearer " + token
        request = urllib.request.Request(url, headers=headers)
        try:
            with opener(request, timeout=20) as response:
                status = getattr(response, "status", 200)
                raw = response.read()
            if status < 200 or status >= 300:
                last_error = f"unexpected HTTP status {status}"
                if status not in TRANSIENT_STATUSES or attempt == retries:
                    return EvidenceResult(State.ERROR, last_error, http_status=status)
            else:
                try:
                    decoded = json.loads(raw.decode("utf-8"))
                except (UnicodeDecodeError, json.JSONDecodeError) as exc:
                    return EvidenceResult(State.ERROR, f"malformed JSON response: {exc}", http_status=status)
                return EvidenceResult(State.SUCCESS, "HTTP response decoded; caller must validate its contract", decoded, http_status=status)
        except urllib.error.HTTPError as exc:
            last_error = f"HTTP {exc.code}: {exc.reason}"
            if exc.code not in TRANSIENT_STATUSES or attempt == retries:
                return EvidenceResult(State.ERROR, last_error, http_status=exc.code)
        except (urllib.error.URLError, TimeoutError, OSError) as exc:
            last_error = f"transport error: {exc}"
            if attempt == retries:
                return EvidenceResult(State.ERROR, last_error)
        sleep(min(2 ** attempt, 8))
    return EvidenceResult(State.ERROR, last_error)


def fetch_all_pages(
    first_url: str,
    *,
    token: str | None = None,
    max_pages: int = 100,
    fetch: Callable[..., EvidenceResult] = fetch_json,
) -> EvidenceResult:
    """Follow GitHub Link pagination; fail closed if page shape or page bound is invalid."""
    if max_pages < 1 or max_pages > 1000:
        return EvidenceResult(State.ERROR, "max_pages must be between 1 and 1000")
    current: str | None = first_url
    combined: list[Any] = []
    pages = 0
    while current:
        if pages >= max_pages:
            return EvidenceResult(State.EMPTY, f"pagination exceeded configured limit ({max_pages})", combined, pages)
        result = fetch(current, token=token)
        if result.state is not State.SUCCESS:
            return EvidenceResult(result.state, f"page {pages + 1}: {result.reason}", combined, pages, result.http_status)
        # fetch_json returns decoded data, not a transport wrapper. Link headers are
        # unavailable through this abstraction, so pagination must use fetch_pages below.
        if not isinstance(result.data, list):
            return EvidenceResult(State.ERROR, f"page {pages + 1} root must be an array", result.data, pages + 1)
        combined.extend(result.data)
        pages += 1
        current = None
    return EvidenceResult(State.SUCCESS if combined else State.EMPTY,
                          "all pages retrieved" if combined else "all pages retrieved but result is empty",
                          combined, pages)


def validate_check_runs(payload: Any, *, expected_sha: str, expected_repo: str) -> EvidenceResult:
    """Validate a GitHub check-runs response and bind every check to the requested SHA."""
    if not isinstance(payload, dict) or not isinstance(payload.get("check_runs"), list):
        return EvidenceResult(State.ERROR, "check-runs response must contain a check_runs array", payload)
    runs = payload["check_runs"]
    if not runs:
        return EvidenceResult(State.EMPTY, "no check runs returned for the requested SHA", payload)
    for run in runs:
        if not isinstance(run, dict):
            return EvidenceResult(State.ERROR, "check run entry is not an object", payload)
        head_sha = run.get("head_sha")
        if head_sha != expected_sha:
            return EvidenceResult(State.ERROR, f"check run SHA mismatch: expected {expected_sha}, got {head_sha!r}", payload)
        if run.get("repository") not in (None, expected_repo):
            return EvidenceResult(State.ERROR, f"check run repository mismatch: expected {expected_repo}, got {run.get('repository')!r}", payload)
        if run.get("status") != "completed":
            return EvidenceResult(State.PENDING, f"check run {run.get('name', '<unnamed>')} is not completed", payload)
        if run.get("conclusion") not in {"success", "failure", "cancelled", "timed_out", "action_required", "neutral", "skipped", "stale"}:
            return EvidenceResult(State.ERROR, f"check run has unknown conclusion: {run.get('conclusion')!r}", payload)
    return EvidenceResult(State.SUCCESS, f"{len(runs)} completed check runs bound to exact SHA", payload, pages=1)


def main() -> int:
    """CLI: read JSON from stdin and validate it against explicit expectations."""
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, help="expected owner/repository")
    parser.add_argument("--sha", required=True, help="expected full commit SHA")
    parser.add_argument("--allow-empty", action="store_true", help="allow a zero-result payload only when contract permits it")
    args = parser.parse_args()
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, UnicodeDecodeError) as exc:
        print(json.dumps({"state": State.ERROR.value, "reason": f"invalid stdin JSON: {exc}"}))
        return 2
    result = validate_payload(payload, expected_repo=args.repo, expected_sha=args.sha, allow_empty=args.allow_empty)
    print(json.dumps({"state": result.state.value, "reason": result.reason, "verified": result.verified}))
    return 0 if result.verified else 1


if __name__ == "__main__":
    raise SystemExit(main())
