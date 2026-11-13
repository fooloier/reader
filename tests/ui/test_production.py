import os
import pathlib
import subprocess
import tempfile
import unittest
from support import ROOT, ARTIFACTS, epub, pdf, realistic_epub, bookmarked_epub, rust_test_count


class ProductionUITests(unittest.TestCase):
    def test_production_interactions(self):
        with tempfile.TemporaryDirectory(prefix="reader-ui-") as directory:
            root = pathlib.Path(directory)
            epub(root / "sample.epub")
            shelf = root / "responsive-shelf"
            shelf.mkdir()
            for i in range(7):
                epub(shelf / f"book-{i}.epub", title=f"{i} 很长的中文书名与连续英文标题 " + "LongTitle" * 20)
            pdf(root / "sample.pdf")
            realistic_epub(root / "realistic.epub")
            bookmarked_epub(root / "bookmarks.epub")
            bookmarked_epub(root / "empty-bookmarks.epub", invalid_only=True)
            pdf(root / "bookmarks.pdf", bookmarks=[("Unresolved", None)] + [(f"Section {i}", 0) for i in range(40)] + [("Page two", 1), ("Page three", 2)])
            pdf(root / "empty-bookmarks.pdf", bookmarks=[("Unresolved", None)])
            env = os.environ.copy()
            env["READER_UI_ROOT"] = directory
            result = subprocess.run(["cargo", "test", "--locked", "--features", "test-support", "--lib", "--", "--test-threads=1", "--nocapture"], cwd=ROOT, env=env, text=True, capture_output=True, timeout=1200)
            ARTIFACTS.mkdir(parents=True, exist_ok=True)
            (ARTIFACTS / "ui.log").write_text(result.stdout + result.stderr)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(rust_test_count(result.stdout), 17)
