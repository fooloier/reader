import json
import os
import pathlib
import struct
import subprocess
import tempfile
from support import ROOT, ARTIFACTS, epub, pdf, realistic_epub


def run(*args, **kwargs):
    return subprocess.run(args, cwd=ROOT, check=True, timeout=30, text=True, capture_output=True, **kwargs)


def main():
    ARTIFACTS.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="reader-native-") as directory:
        root = pathlib.Path(directory)
        epub(root / "sample.epub")
        pdf(root / "sample.pdf")
        realistic_epub(root / "realistic.epub")
        env = os.environ.copy()
        for name in ("DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET"):
            env.pop(name, None)
        env["XDG_RUNTIME_DIR"] = directory
        for name in ("XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME"):
            env[name] = str(root / name.lower())
        requests = [{"op": "import", "paths": [str(root / "sample.epub"), str(root / "sample.pdf"), str(root / "realistic.epub")]}, {"op": "books"}]
        result = run(str(ROOT / "target/debug/reader-driver"), "--native", env=env, input="".join(json.dumps(r) + "\n" for r in requests))
        books = json.loads(result.stdout.splitlines()[-1])["value"]
        oldest = books[-1]["id"]
        result = run(str(ROOT / "target/debug/reader-driver"), "--native", env=env, input=json.dumps({"op": "open", "id": oldest}) + "\n" + json.dumps({"op": "books"}) + "\n")
        books = json.loads(result.stdout.splitlines()[-1])["value"]
        if books[0]["id"] != oldest:
            raise AssertionError("最近打开的书籍未排在最前")
        expected_order = [book["id"] for book in books]
        evidence = []
        initial = run(str(ROOT / "target/debug/reader-driver"), "--native", env=env, input=json.dumps({"op": "settings"}) + "\n")
        base_settings = json.loads(initial.stdout)["value"]
        for mode in ("shelf", "shelf-dark-list", "epub-start", "epub", "pdf", "epub-realistic-cover", "epub-realistic"):
            settings = json.loads(json.dumps(base_settings))
            if mode == "shelf-dark-list":
                settings["appearance"] = "dark"
                settings["shelf"] = "list"
            run(str(ROOT / "target/debug/reader-driver"), "--native", env=env, input=json.dumps({"op": "settings", "value": settings}) + "\n")
            expected_format = "epub" if mode.startswith("epub") else mode
            title = "真实 XHTML" if mode.startswith("epub-realistic") else "测试书籍"
            selected = next((b["id"] for b in books if b["format"] == expected_format and (expected_format != "epub" or b["metadata"]["title"] == title)), None)
            workspace = {"tabs": [b["id"] for b in books] if selected else [], "active": selected}
            run(str(ROOT / "target/debug/reader-driver"), "--native", env=env, input=json.dumps({"op": "workspace", "value": workspace}) + "\n")
            position = None
            if mode == "epub-start":
                position = {"format": "epub", "href": "/OPS/one.xhtml", "anchor": "start"}
            elif mode == "epub":
                position = {"format": "epub", "href": "/OPS/one.xhtml", "anchor": "p30"}
            elif mode == "epub-realistic-cover":
                position = {"format": "epub", "href": "/OPS/wrapper.xhtml", "anchor": "block-0"}
            elif mode == "epub-realistic":
                position = {"format": "epub", "href": "/OPS/%E4%B8%AD%E6%96%87%20%2520%23.xhtml", "anchor": "节 %20#"}
            elif mode == "pdf":
                position = {"format": "pdf", "page": 1, "fraction": 0.0}
            if position:
                run(str(ROOT / "target/debug/reader-driver"), "--native", env=env, input=json.dumps({"op": "position", "id": selected, "location": position}) + "\n")
            background_pdf = next(b["id"] for b in books if b["format"] == "pdf")
            if mode == "epub":
                run(str(ROOT / "target/debug/reader-driver"), "--native", env=env, input=json.dumps({"op": "position", "id": background_pdf, "location": {"format": "pdf", "page": 1, "fraction": 0.25}}) + "\n")
            screenshot = ARTIFACTS / f"native-{mode}.png"
            screenshot.unlink(missing_ok=True)
            env["READER_SCREENSHOT"] = str(screenshot)
            with (ARTIFACTS / f"native-{mode}.log").open("w") as log:
                subprocess.run(
                    ["cargo", "test", "--locked", "--features", "test-support", "--lib",
                     "offscreen_tests::capture_reader", "--", "--ignored", "--exact", "--nocapture"],
                    cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=1200,
                )
            with screenshot.open("rb") as image:
                image.seek(16)
                width, height = struct.unpack(">II", image.read(8))
            saved = run(str(ROOT / "target/debug/reader-driver"), "--native", env=env, input=json.dumps({"op": "books"}) + "\n")
            saved_books = json.loads(saved.stdout)["value"]
            actual_order = [book["id"] for book in saved_books]
            if actual_order != expected_order:
                raise AssertionError(f"自动恢复或退出保存改变了书架顺序 {actual_order}")
            if position:
                actual = next(b["location"] for b in saved_books if b["id"] == selected)
                if mode == "epub":
                    inactive = next(b["location"] for b in saved_books if b["id"] == background_pdf)
                    if inactive != {"format": "pdf", "page": 1, "fraction": 0.25}:
                        raise AssertionError(f"后台 PDF 标签位置被覆盖 {inactive}")
                if mode.startswith("epub") and actual != position:
                    raise AssertionError(f"EPUB 恢复位置不符 {actual}")
                if mode == "pdf" and (actual["page"] != 1 or actual["fraction"] > 0.01):
                    raise AssertionError(f"PDF 恢复位置不符 {actual}")
            evidence.append({"view": mode, "size": [width, height], "screenshot": str(screenshot), "exit_code": 0, "book_order": actual_order})
        (ARTIFACTS / "native.json").write_text(json.dumps(evidence, ensure_ascii=False, indent=2))
        print(json.dumps(evidence, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
