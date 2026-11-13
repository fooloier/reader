import shutil
import sqlite3
from support import ROOT, RuntimeCase, epub, pdf, realistic_epub
import json
import subprocess
import time


class LibraryTests(RuntimeCase):
    def test_addition_and_explicit_open_recency_survives_restart(self):
        ids = []
        sources = []
        for title in ("第一本", "第二本"):
            source = self.sources / f"{title}.epub"
            epub(source, title=title)
            sources.append(source)
            ids.append(self.command(op="import", paths=[str(source)])[0]["id"])
        first, second = ids

        def order():
            return [book["id"] for book in self.command(op="books")]

        self.assertEqual(order(), [second, first])
        self.command(op="open", id=first)
        self.assertEqual(order(), [first, second])
        responses = self.commands(*({"op": "open", "id": book_id} for book_id in [first, second, first, second]))
        self.assertTrue(all(response["ok"] for response in responses), responses)
        self.assertEqual(order(), [second, first])
        self.command(op="open", id=first)
        third_source = self.sources / "第三本.epub"
        epub(third_source, title="第三本")
        third = self.command(op="import", paths=[str(third_source)])[0]["id"]
        self.assertEqual(order(), [third, first, second])
        self.command(op="position", id=second, location={"format": "epub", "href": "/OPS/two.xhtml", "anchor": "end"})
        self.command(op="rename", id=second, title="新书名")
        self.command(op="import", paths=[str(sources[1])])
        self.command(op="workspace", value={"tabs": [first, second], "active": second})
        self.command(op="workspace")
        self.assertEqual(order(), [third, first, second])
        self.assertFalse(self.commands({"op": "open", "id": 999})[0]["ok"])
        self.assertEqual(order(), [third, first, second])
        self.command(op="delete", id=third)
        self.assertEqual(order(), [first, second])

    def test_legacy_library_migration_preserves_newest_first_and_book_data(self):
        first_source = self.sources / "first.epub"
        second_source = self.sources / "second.epub"
        epub(first_source, title="第一本")
        epub(second_source, title="第二本")
        first = self.command(op="import", paths=[str(first_source)])[0]["id"]
        second = self.command(op="import", paths=[str(second_source)])[0]["id"]
        location = {"format": "epub", "href": "/OPS/one.xhtml", "anchor": "p30"}
        self.command(op="position", id=first, location=location)
        self.command(op="rename", id=first, title="保留书名")
        expected = self.command(op="books")
        with sqlite3.connect(self.library / "data/library.sqlite3") as db:
            db.executescript("DROP INDEX books_recency; ALTER TABLE books DROP COLUMN recency; PRAGMA user_version=0;")
        self.assertEqual(self.command(op="books"), expected)
        self.assertEqual(self.command(op="books"), expected)
        self.command(op="open", id=first)
        self.assertEqual([book["id"] for book in self.command(op="books")], [first, second])
        self.assertTrue(all((self.library / "data/books" / book["filename"]).is_file() for book in expected))

    def test_recursive_import_metadata_dedup_and_failures(self):
        folder = self.sources / "nested"
        folder.mkdir()
        epub(folder / "book.epub")
        pdf(self.sources / "book.PDF")
        shutil.copyfile(folder / "book.epub", self.sources / "duplicate.epub")
        (folder / "broken.epub").write_bytes(b"bad zip")
        (folder / "broken.pdf").write_bytes(b"bad pdf")
        (folder / "notes.txt").write_text("unsupported")
        (folder / "cycle").symlink_to(self.sources, target_is_directory=True)
        result = self.command(op="import", paths=[str(self.sources)])
        self.assertEqual(sorted(r["status"] for r in result), ["duplicate", "failed", "failed", "imported", "imported", "skipped", "skipped"])
        books = self.command(op="books")
        self.assertEqual(len(books), 2)
        ebook = next(b for b in books if b["format"] == "epub")
        self.assertEqual(ebook["metadata"], {"title": "测试书籍", "authors": ["测试作者"], "year": 2021})
        pdf_book = next(b for b in books if b["format"] == "pdf")
        self.assertIsNone(pdf_book["metadata"]["year"])
        self.assertIsNotNone(pdf_book["cover"])
        self.assertIsNotNone(ebook["cover"])
        self.assertTrue((self.library / "cache" / pdf_book["cover"]).read_bytes().startswith(b"\x89PNG"))

    def test_rename_position_duplicate_restart_delete_source_untouched(self):
        source, book_id = self.import_epub()
        original = source.read_bytes()
        location = {"format": "epub", "href": "/OPS/one.xhtml", "anchor": "p30"}
        self.command(op="position", id=book_id, location=location)
        book = self.command(op="rename", id=book_id, title="我的书名")
        self.assertIn("我的书名", book["filename"])
        self.assertEqual(book["metadata"]["title"], "测试书籍")
        self.command(op="import", paths=[str(source)])
        book = self.command(op="books")[0]
        self.assertEqual(book["display_title"], "我的书名")
        self.assertEqual(book["location"], location)
        self.command(op="workspace", value={"tabs": [book_id, book_id, 999], "active": book_id})
        self.assertEqual(self.command(op="workspace"), {"tabs": [book_id], "active": book_id})
        self.command(op="delete", id=book_id)
        self.assertEqual(self.command(op="books"), [])
        self.assertEqual(self.command(op="workspace"), {"tabs": [], "active": None})
        self.assertEqual(source.read_bytes(), original)
        self.assertEqual(list((self.library / "data/books").iterdir()), [])
        self.assertFalse(self.commands({"op": "position", "id": book_id, "location": location})[0]["ok"])

    def test_settings_format_validation_and_restart(self):
        _, book_id = self.import_epub()
        settings = self.command(op="settings")
        settings["appearance"] = "dark"
        settings["shelf"] = "list"
        settings["epub"]["size"] = 28
        settings["pdf"] = {"mode": "zoom", "value": 1.5}
        self.command(op="settings", value=settings)
        self.assertEqual(self.command(op="settings"), settings)
        invalid = dict(settings, pdf={"mode": "zoom", "value": -1})
        self.assertFalse(self.commands({"op": "settings", "value": invalid})[0]["ok"])
        self.assertFalse(self.commands({"op": "position", "id": book_id, "location": {"format": "pdf", "page": 2, "fraction": 0.4}})[0]["ok"])

    def test_recovery_cleans_only_staging_and_preserves_orphans(self):
        self.import_epub()
        (self.library / "data/staging/interrupted.tmp").write_bytes(b"partial")
        (self.library / "data/books/orphan.epub").write_bytes(b"uncommitted")
        self.assertEqual(len(self.command(op="books")), 1)
        self.assertEqual(list((self.library / "data/staging").iterdir()), [])
        self.assertEqual((self.library / "data/books/orphan.epub").read_bytes(), b"uncommitted")
        self.command(op="books")
        self.assertTrue((self.library / "data/books/orphan.epub").exists())

    def test_epub_blocks_toc_resources_and_fixed_layout_rejection(self):
        _, book_id = self.import_epub()
        doc = self.command(op="epub", id=book_id)
        self.assertEqual(len(doc["chapters"]), 2)
        self.assertTrue(any("#p30" in item["href"] for item in doc["toc"]))
        blocks = doc["chapters"][0]["blocks"]
        self.assertTrue(any(b["anchor"] == "p30" for b in blocks))
        text = "\n".join(b["markdown"] for b in blocks)
        self.assertIn("**bold**", text)
        self.assertIn("| --- | --- |", text)
        self.assertIn("/OPS/cover.png", text)
        epub(self.sources / "fixed.epub", fixed=True)
        result = self.command(op="import", paths=[str(self.sources / "fixed.epub")])
        self.assertEqual(result[0]["status"], "failed")

    def test_cover_rebuild_and_filename_metadata_fallback(self):
        source = self.sources / "没有元数据.pdf"
        pdf(source, title="")
        self.command(op="import", paths=[str(source)])
        book = self.command(op="books")[0]
        self.assertEqual(book["display_title"], "没有元数据")
        cover = self.library / "cache" / book["cover"]
        cover.unlink()
        self.command(op="books")
        self.assertTrue(cover.read_bytes().startswith(b"\x89PNG"))

    def test_missing_original_record_survives_and_restores(self):
        _, book_id = self.import_epub()
        book = self.command(op="books")[0]
        path = self.library / "data/books" / book["filename"]
        original = path.read_bytes()
        path.unlink()
        self.assertEqual(self.command(op="books")[0]["id"], book_id)
        self.assertFalse(self.commands({"op": "epub", "id": book_id})[0]["ok"])
        path.write_bytes(original)
        self.assertEqual(len(self.command(op="epub", id=book_id)["chapters"]), 2)

    def test_reset_database_keeps_managed_originals(self):
        self.import_epub()
        book = self.command(op="books")[0]
        path = self.library / "data/books" / book["filename"]
        original = path.read_bytes()
        (self.library / "data/library.sqlite3").unlink()
        self.assertEqual(self.command(op="books"), [])
        self.assertEqual(path.read_bytes(), original)
        self.command(op="books")
        self.assertEqual(path.read_bytes(), original)

    def test_corrupt_settings_and_workspace_backup_defaults(self):
        self.command(op="books")
        for relative, payload, op in [("config/settings.json", b"broken-json", "settings"), ("state/workspace.json", b'{"tabs":false}', "workspace"), ("config/settings.json", b'{"appearance":"light","shelf":"cards","pdf":{"mode":"zoom","value":-1},"epub":{"font":"x","size":20,"line_height":1.7,"paragraph_gap":1,"width":720}}', "settings")]:
            path = self.library / relative
            path.write_bytes(payload)
            value = self.command(op=op)
            self.assertIsInstance(value, dict)
            self.assertTrue(any(p.read_bytes() == payload for p in path.parent.glob(path.name + ".corrupt-*")))
            self.command(op=op, value=value)

    def test_root_symlink_and_folder_skips(self):
        epub(self.sources / "book.epub")
        (self.sources / "notes.txt").write_text("not a book")
        link = self.root / "selected"
        link.symlink_to(self.sources, target_is_directory=True)
        result = self.command(op="import", paths=[str(link)])
        self.assertEqual(sorted(r["status"] for r in result), ["imported", "skipped"])
        result = self.command(op="import", paths=[str(self.sources / "notes.txt")])
        self.assertEqual(result[0]["status"], "failed")
        file_link = self.root / "selected-file-without-extension"
        file_link.symlink_to(self.sources / "book.epub")
        result = self.command(op="import", paths=[str(file_link)])
        self.assertEqual(result[0]["status"], "duplicate", result)

    def test_managed_filename_uses_utf8_byte_budget(self):
        _, book_id = self.import_epub()
        book = self.command(op="rename", id=book_id, title="𠀀" * 200)
        self.assertLessEqual(len(book["filename"].encode()), 240)
        self.assertTrue((self.library / "data/books" / book["filename"]).is_file())

    def test_realistic_xhtml_urls_toc_images_and_restart(self):
        source = self.sources / "realistic.epub"
        realistic_epub(source)
        result = self.command(op="import", paths=[str(source)])
        self.assertEqual(result[0]["status"], "imported", result)
        book_id = result[0]["id"]
        doc = self.command(op="epub", id=book_id)
        self.assertEqual(len(doc["chapters"]), 4)
        self.assertEqual(doc["chapters"][1]["blocks"], [])
        self.assertIn("%25", doc["chapters"][0]["blocks"][0]["markdown"])
        self.assertTrue(any("Hello **bold** world" in b["markdown"] for b in doc["chapters"][2]["blocks"]))
        self.assertGreater(doc["toc"][1]["depth"], doc["toc"][0]["depth"])
        href = doc["toc"][1]["href"]
        self.assertEqual(self.command(op="destination", id=book_id, href=href), [3, 1])
        self.assertIsNone(self.command(op="destination", id=book_id, href=doc["toc"][2]["href"]))
        self.assertIn("/OPS/%E5%B0%81%E9%9D%A2%20%2520%23%29.png", doc["image_hrefs"])
        location = {"format": "epub", "href": doc["chapters"][3]["href"], "anchor": "节 %20#"}
        self.command(op="position", id=book_id, location=location)
        self.assertEqual(self.command(op="books")[0]["location"], location)
        epub2 = self.sources / "realistic-v2.epub"
        realistic_epub(epub2, version=2)
        result = self.command(op="import", paths=[str(epub2)])
        self.assertEqual(result[0]["status"], "imported", result)
        book2 = result[0]["id"]
        doc2 = self.command(op="epub", id=book2)
        self.assertEqual(len(doc2["chapters"]), 4)
        self.assertEqual(self.command(op="destination", id=book2, href=doc2["toc"][1]["href"]), [3, 1])

    def test_real_import_process_interruption_cleans_partial_staging(self):
        source = self.sources / "large.pdf"
        with source.open("wb") as stream:
            stream.write(b"%PDF-1.4\n")
            stream.truncate(256 * 1024 * 1024)
        process = subprocess.Popen([str(ROOT / "target/debug/reader-driver"), str(self.library)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=self.env)
        try:
            process.stdin.write(json.dumps({"op": "import", "paths": [str(source)]}) + "\n")
            process.stdin.flush()
            staging = self.library / "data/staging/import.tmp"
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if staging.exists() and 0 < staging.stat().st_size < source.stat().st_size:
                    process.kill()
                    process.wait(timeout=10)
                    break
                self.assertIsNone(process.poll(), "导入进程不应在中断前结束")
            else:
                self.fail("未观察到真实导入的部分写入")
            self.assertLess(process.returncode, 0)
            self.assertTrue(staging.exists())
            self.assertEqual(self.command(op="books"), [])
            self.assertEqual(list(staging.parent.iterdir()), [])
            self.assertEqual(source.stat().st_size, 256 * 1024 * 1024)
            with source.open("rb") as stream:
                self.assertEqual(stream.read(9), b"%PDF-1.4\n")
        finally:
            if process.poll() is None:
                process.kill()
            process.communicate(timeout=10)
