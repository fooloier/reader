import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import time
from support import ROOT, ARTIFACTS, epub, pdf, realistic_epub


def run(*args, **kwargs):
    return subprocess.run(args, cwd=ROOT, check=True, timeout=30, text=True, capture_output=True, **kwargs)


def main():
    for executable in ("hyprctl", "grim"):
        if not shutil.which(executable):
            raise RuntimeError(f"原生检查需要 {executable}")
    ARTIFACTS.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="reader-native-") as directory:
        root = pathlib.Path(directory)
        epub(root / "sample.epub")
        pdf(root / "sample.pdf")
        realistic_epub(root / "realistic.epub")
        env = os.environ.copy()
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
            ready = root / f"ready-{mode}"
            env["READER_READY_FILE"] = str(ready)
            with (ARTIFACTS / f"native-{mode}.log").open("w") as log:
                process = subprocess.Popen([str(ROOT / "target/debug/reader")], cwd=ROOT, env=env, stdout=log, stderr=log)
                try:
                    deadline = time.monotonic() + 30
                    client = None
                    while time.monotonic() < deadline:
                        if process.poll() is not None:
                            raise RuntimeError(f"原生进程提前退出 {process.returncode}")
                        clients = json.loads(run("hyprctl", "clients", "-j").stdout)
                        client = next((c for c in clients if c["pid"] == process.pid and c["mapped"]), None)
                        if client and ready.exists():
                            break
                    else:
                        raise RuntimeError("原生窗口未在期限内就绪")
                    run("hyprctl", "dispatch", f'hl.dsp.focus({{window = "pid:{process.pid}"}})')
                    x, y = client["at"]
                    width, height = client["size"]
                    screenshot = ARTIFACTS / f"native-{mode}.png"
                    previous = None
                    stable = 0
                    while time.monotonic() < deadline:
                        run("grim", "-g", f"{x},{y} {width}x{height}", str(screenshot))
                        digest = hashlib.sha256(screenshot.read_bytes()).hexdigest()
                        stable = stable + 1 if previous == digest else 0
                        previous = digest
                        if stable >= 2:
                            break
                    if stable < 2:
                        raise RuntimeError("窗口图像未稳定")
                    run("hyprctl", "dispatch", f'hl.dsp.window.close({{window = "pid:{process.pid}"}})')
                    process.wait(timeout=10)
                    if process.returncode:
                        raise RuntimeError(f"原生进程退出码 {process.returncode}")
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
                    evidence.append({"view": mode, "size": [width, height], "screenshot": str(screenshot), "exit_code": process.returncode, "book_order": actual_order})
                finally:
                    if process.poll() is None:
                        process.terminate()
                        process.wait(timeout=10)
        (ARTIFACTS / "native.json").write_text(json.dumps(evidence, ensure_ascii=False, indent=2))
        print(json.dumps(evidence, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
