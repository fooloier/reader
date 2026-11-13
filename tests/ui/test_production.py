import os
import pathlib
import subprocess
import tempfile
import unittest
from support import ROOT, ARTIFACTS, epub, pdf, realistic_epub, rust_test_count


class ProductionUITests(unittest.TestCase):
    def test_production_interactions(self):
        with tempfile.TemporaryDirectory(prefix="reader-ui-") as directory:
            root = pathlib.Path(directory)
            epub(root / "sample.epub")
            pdf(root / "sample.pdf")
            realistic_epub(root / "realistic.epub")
            env = os.environ.copy()
            env["READER_UI_ROOT"] = directory
            result = subprocess.run(["cargo", "test", "--locked", "--features", "test-support", "--lib", "ui_tests::", "--", "--test-threads=1", "--nocapture"], cwd=ROOT, env=env, text=True, capture_output=True, timeout=1200)
            ARTIFACTS.mkdir(parents=True, exist_ok=True)
            (ARTIFACTS / "ui.log").write_text(result.stdout + result.stderr)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(rust_test_count(result.stdout), 10)
