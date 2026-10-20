import copy
import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from tools.validate_os_function_catalog import validate_documents


class OsFunctionCatalogValidatorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.catalog = json.loads((ROOT / "docs/OS_FUNCTION_CATALOG.json").read_text(encoding="utf-8"))
        cls.contract = json.loads((ROOT / "docs/OS_FUNCTION_CONTRACT_SCHEMA.json").read_text(encoding="utf-8"))
        cls.markdown = (ROOT / "docs/OS_FUNCTION_CATALOG.md").read_text(encoding="utf-8")

    def test_repository_baseline_is_consistent(self):
        self.assertEqual(validate_documents(self.catalog, self.contract, self.markdown), [])

    def test_duplicate_function_id_is_rejected(self):
        catalog = copy.deepcopy(self.catalog)
        catalog["functions"][1]["id"] = catalog["functions"][0]["id"]
        self.assertTrue(any("duplicate function ID" in e for e in validate_documents(catalog, self.contract, self.markdown)))

    def test_undeclared_status_is_rejected(self):
        catalog = copy.deepcopy(self.catalog)
        catalog["functions"][0]["status"] = "VERIFIED"
        self.assertTrue(any("status is not declared" in e for e in validate_documents(catalog, self.contract, self.markdown)))

    def test_markdown_json_id_drift_is_rejected(self):
        markdown = self.markdown.replace("ATC-FUNC-OS-BOOT-001", "ATC-FUNC-OS-BOOT-999", 1)
        self.assertTrue(any("IDs missing" in e for e in validate_documents(self.catalog, self.contract, markdown)))

    def test_invalid_priority_is_rejected(self):
        catalog = copy.deepcopy(self.catalog)
        catalog["functions"][0]["priority"] = "P9"
        self.assertTrue(any("priority must be" in e for e in validate_documents(catalog, self.contract, self.markdown)))


if __name__ == "__main__":
    unittest.main()
