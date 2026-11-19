#!/usr/bin/env python3
"""Fail-closed validation and retrieval for GitHub API evidence.

Transport success is not evidence success. Callers must validate the response
contract and bind evidence to the expected repository and full commit SHA.
"""
from __future__ import annotations

import json
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from datetime import datetime, timezone
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
    next_url: str | None = None
    requested_url: str | None = None
    retrieved_at: str = field(default_factory=lambda: datetime.now(timezone.utc).isoformat())
    result_count: int | None = None
    page_urls: tuple[str, ...] = ()

    @property
    def verified(self) -> bool:
        return self.state is State.SUCCESS

    def as_dict(self) -> dict[str, Any]:
        """Return a stable, machine-readable provenance record."""
        parsed = urllib.parse.urlparse(self.requested_url or "")
        return {
            "state": self.state.value,
            "reason": self.reason,
            "verified": self.verified,
            "requested_url": self.requested_url,
            "query": urllib.parse.parse_qs(parsed.query, keep_blank_values=True),
            "retrieved_at": self.retrieved_at,
            "http_status": self.http_status,
            "pages": self.pages,
            "result_count": self.result_count,
            "page_urls": list(self.page_urls),
        }


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
    """Validate decoded JSON against an explicit caller-owned contract."""
    if pending:
        return EvidenceResult(State.PENDING, "checks are queued or in progress", payload)
    if not isinstance(payload, (dict, list)):
        return EvidenceResult(State.ERROR, "JSON root must be an object or array", payload)

    if isinstance(payload, dict):
        missing = [name for name in required_fields if name not in payload]
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
            if not isinstance(runs, list) or any(not isinstance(run, dict) for run in runs):
                return EvidenceResult(State.ERROR, "check_runs must be an array of objects", payload)
            if any(run.get("status") != "completed" for run in runs):
                return EvidenceResult(State.PENDING, "one or more checks are not completed", payload)
        if "total_count" in payload and "items" in payload:
            items = payload["items"]
            if not isinstance(items, list) or payload["total_count"] != len(items):
                return EvidenceResult(State.EMPTY, "declared count does not match returned items", payload)
        is_empty = not payload or ("check_runs" in payload and not payload["check_runs"])
        if is_empty and not allow_empty:
            return EvidenceResult(State.EMPTY, "unexpectedly empty evidence payload", payload)
    elif not payload and not allow_empty:
        return EvidenceResult(State.EMPTY, "unexpectedly empty result array", payload)

    count = len(payload) if isinstance(payload, list) else None
    return EvidenceResult(State.SUCCESS, "payload satisfies declared contract", payload, result_count=count)


def _parse_link_next(link_header: str | None) -> str | None:
    """Parse a GitHub Link header without splitting commas inside URI references."""
    if not link_header:
        return None
    # GitHub's pagination URLs do not contain unescaped commas. Reject malformed
    # segments rather than guessing a next page.
    for part in link_header.split(","):
        segments = [segment.strip() for segment in part.split(";")]
        if len(segments) > 1 and any('rel="next"' in segment for segment in segments[1:]):
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
    """Fetch one GitHub API JSON endpoint with bounded transient retries."""
    if retries < 0 or retries > 5:
        return EvidenceResult(State.ERROR, "retries must be between 0 and 5", requested_url=url)
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme != "https" or parsed.hostname != "api.github.com" or parsed.username or parsed.password:
        return EvidenceResult(State.ERROR, "only https://api.github.com endpoints are permitted", requested_url=url)

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
                response_headers = getattr(response, "headers", {})
                link_header = response_headers.get("Link") if response_headers else None
            if status < 200 or status >= 300:
                last_error = f"HTTP {status}: unexpected non-success response"
                if status not in TRANSIENT_STATUSES or attempt == retries:
                    return EvidenceResult(State.ERROR, last_error, http_status=status, requested_url=url)
            else:
                try:
                    decoded = json.loads(raw.decode("utf-8"))
                except (UnicodeDecodeError, json.JSONDecodeError) as exc:
                    return EvidenceResult(State.ERROR, f"malformed JSON response: {exc}", http_status=status, requested_url=url)
                return EvidenceResult(
                    State.SUCCESS, "HTTP response decoded; caller must validate its contract",
                    decoded, http_status=status, next_url=_parse_link_next(link_header),
                    requested_url=url,
                )
        except urllib.error.HTTPError as exc:
            try:
                body = exc.read().decode("utf-8", errors="replace")[:2000]
            except Exception:
                body = ""
            detail = f"HTTP {exc.code}: {exc.reason}"
            if body:
                detail += f"; API body: {body}"
            last_error = detail
            if exc.code not in TRANSIENT_STATUSES or attempt == retries:
                return EvidenceResult(State.ERROR, last_error, http_status=exc.code, requested_url=url)
        except (urllib.error.URLError, TimeoutError, OSError) as exc:
            last_error = f"transport error: {exc}"
            if attempt == retries:
                return EvidenceResult(State.ERROR, last_error, requested_url=url)
        sleep(min(2 ** attempt, 8))
    return EvidenceResult(State.ERROR, last_error, requested_url=url)


def fetch_all_pages(
    first_url: str,
    *,
    token: str | None = None,
    max_pages: int = 100,
    allow_empty: bool = False,
    fetch: Callable[..., EvidenceResult] = fetch_json,
) -> EvidenceResult:
    """Retrieve every Link-paginated array; any missing/failed page fails closed."""
    parsed_first = urllib.parse.urlparse(first_url)
    if parsed_first.scheme != "https" or parsed_first.hostname != "api.github.com":
        return EvidenceResult(State.ERROR, "only https://api.github.com endpoints are permitted", requested_url=first_url)
    if max_pages < 1 or max_pages > 1000:
        return EvidenceResult(State.ERROR, "max_pages must be between 1 and 1000", requested_url=first_url)

    current: str | None = first_url
    combined: list[Any] = []
    visited: set[str] = set()
    page_urls: list[str] = []
    last_status: int | None = None
    while current:
        if current in visited:
            return EvidenceResult(State.ERROR, "pagination loop detected", combined, len(page_urls), last_status, requested_url=first_url, result_count=len(combined), page_urls=tuple(page_urls))
        if len(page_urls) >= max_pages:
            return EvidenceResult(State.EMPTY, f"pagination exceeded configured limit ({max_pages})", combined, len(page_urls), last_status, requested_url=first_url, result_count=len(combined), page_urls=tuple(page_urls))
        parsed = urllib.parse.urlparse(current)
        if parsed.scheme != "https" or parsed.hostname != "api.github.com" or parsed.username or parsed.password:
            return EvidenceResult(State.ERROR, "pagination link escaped the GitHub API host", combined, len(page_urls), last_status, requested_url=first_url, result_count=len(combined), page_urls=tuple(page_urls))

        visited.add(current)
        page_urls.append(current)
        result = fetch(current, token=token)
        last_status = result.http_status
        if result.state is not State.SUCCESS:
            return EvidenceResult(result.state, f"page {len(page_urls)} failed: {result.reason}", combined, len(page_urls), last_status, requested_url=first_url, result_count=len(combined), page_urls=tuple(page_urls))
        if not isinstance(result.data, list):
            return EvidenceResult(State.ERROR, f"page {len(page_urls)} root must be an array", result.data, len(page_urls), last_status, requested_url=first_url, result_count=len(combined), page_urls=tuple(page_urls))
        combined.extend(result.data)
        current = result.next_url

    if not combined and not allow_empty:
        return EvidenceResult(State.EMPTY, "all pages retrieved but result is empty", combined, len(page_urls), last_status, requested_url=first_url, result_count=0, page_urls=tuple(page_urls))
    return EvidenceResult(State.SUCCESS, "all pages retrieved", combined, len(page_urls), last_status, requested_url=first_url, result_count=len(combined), page_urls=tuple(page_urls))


def validate_check_runs(payload: Any, *, expected_sha: str, expected_repo: str) -> EvidenceResult:
    """Validate check-run evidence binding; callers still enforce green conclusions."""
    if not isinstance(payload, dict) or not isinstance(payload.get("check_runs"), list):
        return EvidenceResult(State.ERROR, "check-runs response must contain a check_runs array", payload)
    runs = payload["check_runs"]
    if not runs:
        return EvidenceResult(State.EMPTY, "no check runs returned for the requested SHA", payload)
    for run in runs:
        if not isinstance(run, dict):
            return EvidenceResult(State.ERROR, "check run entry is not an object", payload)
        if run.get("head_sha") != expected_sha:
            return EvidenceResult(State.ERROR, f"check run SHA mismatch: expected {expected_sha}, got {run.get('head_sha')!r}", payload)
        repository = run.get("repository")
        if isinstance(repository, dict):
            repository = repository.get("full_name")
        if repository is not None and repository != expected_repo:
            return EvidenceResult(State.ERROR, f"check run repository mismatch: expected {expected_repo}, got {repository!r}", payload)
        if run.get("status") != "completed":
            return EvidenceResult(State.PENDING, f"check run {run.get('name', '<unnamed>')} is not completed", payload)
        if run.get("conclusion") not in {"success", "failure", "cancelled", "timed_out", "action_required", "neutral", "skipped", "stale"}:
            return EvidenceResult(State.ERROR, f"check run has unknown conclusion: {run.get('conclusion')!r}", payload)
    return EvidenceResult(State.SUCCESS, f"{len(runs)} completed check runs bound to exact SHA", payload, pages=1, result_count=len(runs))


def main() -> int:
    """CLI: validate JSON stdin against caller-supplied repository and SHA."""
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, help="expected owner/repository")
    parser.add_argument("--sha", required=True, help="expected full commit SHA")
    parser.add_argument("--allow-empty", action="store_true", help="allow zero results only when contract explicitly permits it")
    args = parser.parse_args()
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, UnicodeDecodeError) as exc:
        print(json.dumps({"state": State.ERROR.value, "reason": f"invalid stdin JSON: {exc}", "verified": False}))
        return 2
    result = validate_payload(payload, expected_repo=args.repo, expected_sha=args.sha, allow_empty=args.allow_empty)
    print(json.dumps({"state": result.state.value, "reason": result.reason, "verified": result.verified}))
    return 0 if result.verified else 1


if __name__ == "__main__":
    raise SystemExit(main())
