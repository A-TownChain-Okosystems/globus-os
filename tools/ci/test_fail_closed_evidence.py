import io
import json
import unittest
from unittest.mock import patch
from urllib.error import HTTPError, URLError

from fail_closed_evidence import (
    EvidenceResult, State, fetch_json, validate_check_runs, validate_payload,
)


class Response:
    status = 200
    def __init__(self, body):
        self.body = body
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

    def test_unexpected_empty_array_fails(self):
        self.assertEqual(validate_payload([]).state, State.EMPTY)

    def test_explicitly_allowed_zero_results(self):
        self.assertEqual(validate_payload([], allow_empty=True).state, State.SUCCESS)

    def test_empty_object_fails(self):
        self.assertEqual(validate_payload({}).state, State.EMPTY)

    def test_wrong_json_root_fails(self):
        self.assertEqual(validate_payload("not an object").state, State.ERROR)

    def test_missing_required_field_fails(self):
        self.assertEqual(validate_payload({"sha": "x"}, required_fields=["check_runs"]).state, State.EMPTY)

    def test_wrong_repo_and_sha_fail(self):
        self.assertEqual(validate_payload({"repository": "other/repo", "sha": "x"},
                                           expected_repo="owner/repo").state, State.ERROR)
        self.assertEqual(validate_payload({"sha": "x"}, expected_sha="y").state, State.ERROR)

    def test_count_mismatch_fails(self):
        self.assertEqual(validate_payload({"total_count": 2, "items": [1]}).state, State.EMPTY)

    def test_pending_is_not_success(self):
        self.assertEqual(validate_payload({"sha": "x"}, pending=True).state, State.PENDING)

    def test_check_runs_require_exact_sha_and_completion(self):
        good = {"check_runs": [{"head_sha": "a" * 40, "status": "completed", "conclusion": "success"}]}
        self.assertEqual(validate_check_runs(good, expected_sha="a" * 40, expected_repo="owner/repo").state, State.SUCCESS)
        wrong = {"check_runs": [{"head_sha": "b" * 40, "status": "completed", "conclusion": "success"}]}
        self.assertEqual(validate_check_runs(wrong, expected_sha="a" * 40, expected_repo="owner/repo").state, State.ERROR)
        running = {"check_runs": [{"head_sha": "a" * 40, "status": "in_progress"}]}
        self.assertEqual(validate_check_runs(running, expected_sha="a" * 40, expected_repo="owner/repo").state, State.PENDING)

    def test_malformed_json_fails(self):
        result = fetch_json("https://api.github.com/test", retries=0,
                            opener=lambda *a, **k: Response(b"{"))
        self.assertEqual(result.state, State.ERROR)

    def test_http_401_fails_without_retry(self):
        calls = []
        def opener(*args, **kwargs):
            calls.append(1)
            raise HTTPError("url", 401, "unauthorized", {}, io.BytesIO(b"{}"))
        result = fetch_json("https://api.github.com/test", retries=3, opener=opener, sleep=lambda _: None)
        self.assertEqual(result.state, State.ERROR)
        self.assertEqual(result.http_status, 401)
        self.assertEqual(len(calls), 1)

    def test_transient_transport_error_retries_with_bound(self):
        calls = []
        def opener(*args, **kwargs):
            calls.append(1)
            raise URLError("temporary")
        result = fetch_json("https://api.github.com/test", retries=2, opener=opener, sleep=lambda _: None)
        self.assertEqual(result.state, State.ERROR)
        self.assertEqual(len(calls), 3)

    def test_non_github_endpoint_is_rejected(self):
        self.assertEqual(fetch_json("https://example.com", retries=0).state, State.ERROR)


if __name__ == "__main__":
    unittest.main()
