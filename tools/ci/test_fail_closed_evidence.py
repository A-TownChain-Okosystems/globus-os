import io
import json
import unittest
from urllib.error import HTTPError, URLError
from unittest.mock import patch

from fail_closed_evidence import (
    EvidenceResult, State, fetch_all_pages, fetch_json, validate_check_runs, validate_payload,
)


class Response:
    def __init__(self, body, status=200, headers=None):
        self.body = body
        self.status = status
        self.headers = headers or {}
    def __enter__(self):
        return self
    def __exit__(self, *args):
        return False
    def read(self):
        return self.body


class FailClosedEvidenceTests(unittest.TestCase):
    def test_valid_repo_and_exact_sha(self):
        result = validate_payload({"repository": "owner/repo", "sha": "a" * 40, "items": [1]},
                                  expected_repo="owner/repo", expected_sha="a" * 40)
        self.assertEqual(result.state, State.SUCCESS)
        self.assertTrue(result.verified)

    def test_unexpected_empty_array_fails_and_explicit_empty_passes(self):
        self.assertEqual(validate_payload([]).state, State.EMPTY)
        self.assertEqual(validate_payload([], allow_empty=True).state, State.SUCCESS)

    def test_empty_object_and_wrong_root_fail(self):
        self.assertEqual(validate_payload({}).state, State.EMPTY)
        self.assertEqual(validate_payload("not an object").state, State.ERROR)

    def test_missing_fields_repo_sha_and_count_mismatch_fail(self):
        self.assertEqual(validate_payload({"sha": "x"}, required_fields=["check_runs"]).state, State.EMPTY)
        self.assertEqual(validate_payload({"repository": "other/repo"}, expected_repo="owner/repo").state, State.ERROR)
        self.assertEqual(validate_payload({"sha": "x"}, expected_sha="y").state, State.ERROR)
        self.assertEqual(validate_payload({"total_count": 2, "items": [1]}).state, State.EMPTY)

    def test_pending_is_not_success(self):
        self.assertEqual(validate_payload({"sha": "x"}, pending=True).state, State.PENDING)

    def test_check_runs_bind_exact_sha_and_pending_status(self):
        good = {"check_runs": [{"head_sha": "a" * 40, "status": "completed", "conclusion": "success"}]}
        self.assertEqual(validate_check_runs(good, expected_sha="a" * 40, expected_repo="owner/repo").state, State.SUCCESS)
        wrong = {"check_runs": [{"head_sha": "b" * 40, "status": "completed", "conclusion": "success"}]}
        self.assertEqual(validate_check_runs(wrong, expected_sha="a" * 40, expected_repo="owner/repo").state, State.ERROR)
        running = {"check_runs": [{"head_sha": "a" * 40, "status": "in_progress"}]}
        self.assertEqual(validate_check_runs(running, expected_sha="a" * 40, expected_repo="owner/repo").state, State.PENDING)

    def test_malformed_json_fails(self):
        result = fetch_json("https://api.github.com/test", retries=0, opener=lambda *a, **k: Response(b"{"))
        self.assertEqual(result.state, State.ERROR)

    def test_401_403_404_fail_without_retry_and_capture_api_body(self):
        for status in (401, 403, 404):
            calls = []
            def opener(*args, _status=status, **kwargs):
                calls.append(1)
                raise HTTPError("url", _status, "denied", {}, io.BytesIO(b'{"message":"detail"}'))
            result = fetch_json("https://api.github.com/test", retries=3, opener=opener, sleep=lambda _: None)
            self.assertEqual(result.state, State.ERROR)
            self.assertEqual(result.http_status, status)
            self.assertEqual(len(calls), 1)
            self.assertIn("detail", result.reason)

    def test_429_and_5xx_retry_then_recover(self):
        for status in (429, 500, 502, 503, 504):
            calls = []
            def opener(*args, _status=status, **kwargs):
                calls.append(1)
                if len(calls) == 1:
                    raise HTTPError("url", _status, "temporary", {}, io.BytesIO(b'{"message":"retry"}'))
                return Response(b'[{"id":1}]')
            result = fetch_json("https://api.github.com/test?per_page=100", retries=2, opener=opener, sleep=lambda _: None)
            self.assertEqual(result.state, State.SUCCESS, status)
            self.assertEqual(len(calls), 2, status)
            self.assertEqual(result.data, [{"id": 1}])
            self.assertEqual(result.as_dict()["query"]["per_page"], ["100"])

    def test_transient_transport_error_retries_with_bound(self):
        calls = []
        def opener(*args, **kwargs):
            calls.append(1)
            raise URLError("temporary")
        result = fetch_json("https://api.github.com/test", retries=2, opener=opener, sleep=lambda _: None)
        self.assertEqual(result.state, State.ERROR)
        self.assertEqual(len(calls), 3)

    def test_retry_count_is_bounded(self):
        self.assertEqual(fetch_json("https://api.github.com/test", retries=6).state, State.ERROR)

    def test_pagination_retrieves_multiple_pages_and_records_provenance(self):
        pages = {
            "https://api.github.com/items?per_page=2": EvidenceResult(
                State.SUCCESS, "ok", [{"id": 1}, {"id": 2}], http_status=200,
                next_url="https://api.github.com/items?page=2&per_page=2"),
            "https://api.github.com/items?page=2&per_page=2": EvidenceResult(
                State.SUCCESS, "ok", [{"id": 3}], http_status=200),
        }
        calls = []
        def fetch(url, token=None):
            calls.append(url)
            return pages[url]
        result = fetch_all_pages("https://api.github.com/items?per_page=2", fetch=fetch)
        self.assertEqual(result.state, State.SUCCESS)
        self.assertEqual(result.data, [{"id": 1}, {"id": 2}, {"id": 3}])
        self.assertEqual(result.pages, 2)
        self.assertEqual(result.result_count, 3)
        self.assertEqual(len(result.page_urls), 2)
        self.assertEqual(result.as_dict()["query"]["per_page"], ["2"])
        self.assertTrue(result.as_dict()["retrieved_at"])

    def test_failed_second_page_fails_closed_without_claiming_success(self):
        calls = []
        def fetch(url, token=None):
            calls.append(url)
            if len(calls) == 1:
                return EvidenceResult(State.SUCCESS, "ok", [1], next_url="https://api.github.com/items?page=2")
            return EvidenceResult(State.ERROR, "HTTP 503", http_status=503)
        result = fetch_all_pages("https://api.github.com/items", fetch=fetch)
        self.assertEqual(result.state, State.ERROR)
        self.assertEqual(result.pages, 2)
        self.assertEqual(result.data, [1])

    def test_non_array_page_and_empty_result_fail_closed(self):
        result = fetch_all_pages("https://api.github.com/items",
                                 fetch=lambda url, token=None: EvidenceResult(State.SUCCESS, "ok", {"items": []}))
        self.assertEqual(result.state, State.ERROR)
        empty = fetch_all_pages("https://api.github.com/items",
                                fetch=lambda url, token=None: EvidenceResult(State.SUCCESS, "ok", []))
        self.assertEqual(empty.state, State.EMPTY)
        allowed = fetch_all_pages("https://api.github.com/items", allow_empty=True,
                                  fetch=lambda url, token=None: EvidenceResult(State.SUCCESS, "ok", []))
        self.assertEqual(allowed.state, State.SUCCESS)

    def test_pagination_host_escape_loop_and_page_limit_fail(self):
        escape = fetch_all_pages("https://api.github.com/items",
                                 fetch=lambda url, token=None: EvidenceResult(State.SUCCESS, "ok", [1], next_url="https://example.com/steal"))
        self.assertEqual(escape.state, State.ERROR)
        loop = fetch_all_pages("https://api.github.com/items",
                               fetch=lambda url, token=None: EvidenceResult(State.SUCCESS, "ok", [1], next_url="https://api.github.com/items"))
        self.assertEqual(loop.state, State.ERROR)
        limited = fetch_all_pages("https://api.github.com/items", max_pages=1,
                                  fetch=lambda url, token=None: EvidenceResult(State.SUCCESS, "ok", [1], next_url="https://api.github.com/items?page=2"))
        self.assertEqual(limited.state, State.EMPTY)

    def test_link_parser_and_non_github_endpoint(self):
        from fail_closed_evidence import _parse_link_next
        self.assertEqual(_parse_link_next('<https://api.github.com/page=2>; rel="next", <https://api.github.com/page=1>; rel="last'),
                         "https://api.github.com/page=2")
        self.assertIsNone(_parse_link_next(None))
        self.assertEqual(fetch_json("https://example.com", retries=0).state, State.ERROR)


if __name__ == "__main__":
    unittest.main()
