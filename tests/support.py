import re
import struct
import zlib
import json
import os
import pathlib
import subprocess
import tempfile
import unittest
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
ARTIFACTS = ROOT / "target/test-results"


def rust_test_count(output):
    results = re.findall(r"^test result: ok\. (\d+) passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;", output, re.MULTILINE)
    if not results:
        raise AssertionError("没有完整的 Rust 成功测试摘要")
    return sum(map(int, results))


def pdf(path, title="Fixture PDF", pages=3, bookmarks=None):
    objects = [b"<< /Type /Catalog /Pages 2 0 R /Outlines 5 0 R >>", b"", b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>", f"<< /Title ({title}) /Author (Test Author) /CreationDate (D:20200101000000) >>".encode(), b"<< /Type /Outlines /First 6 0 R /Last 6 0 R /Count 1 >>", b"<< /Title (Chapter one) /Parent 5 0 R /Dest [7 0 R /Fit] >>"]
    page_ids = []
    for i in range(pages):
        page_id = len(objects) + 1
        page_ids.append(page_id)
        content = f"BT /F1 24 Tf 40 730 Td (Reader page {i + 1} selectable text) Tj ET".encode()
        objects.extend([f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /Font << /F1 3 0 R >> >> /Contents {page_id + 1} 0 R >>".encode(), b"<< /Length " + str(len(content)).encode() + b" >>\nstream\n" + content + b"\nendstream"])
    objects[1] = f"<< /Type /Pages /Kids [{' '.join(f'{i} 0 R' for i in page_ids)}] /Count {pages} >>".encode()
    if bookmarks is not None:
        first = len(objects) + 1
        objects[4] = f"<< /Type /Outlines /First {first} 0 R /Last {first + len(bookmarks) - 1} 0 R /Count {len(bookmarks)} >>".encode()
        for i, (label, page) in enumerate(bookmarks):
            destination = f"/Dest [{page_ids[page]} 0 R /Fit]" if page is not None else ""
            sibling = f"/Next {first + i + 1} 0 R" if i + 1 < len(bookmarks) else ""
            objects.append(f"<< /Title ({label}) /Parent 5 0 R {destination} {sibling} >>".encode())
    data = bytearray(b"%PDF-1.4\n")
    offsets = [0]
    for i, obj in enumerate(objects, 1):
        offsets.append(len(data))
        data.extend(f"{i} 0 obj\n".encode() + obj + b"\nendobj\n")
    xref = len(data)
    data.extend(f"xref\n0 {len(offsets)}\n0000000000 65535 f \n".encode())
    for offset in offsets[1:]:
        data.extend(f"{offset:010} 00000 n \n".encode())
    data.extend(f"trailer\n<< /Size {len(offsets)} /Root 1 0 R /Info 4 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode())
    path.write_bytes(data)


def png():
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    width, height = 120, 160
    rows = b"".join(b"\0" + bytes([60 + y // 2, 110, 145]) * width for y in range(height))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")


def epub(path, title="测试书籍", fixed=False):
    content = "".join(f'<p id="p{i}">Paragraph {i}. Selectable reading text. 中文阅读内容，测试字号与阅读位置。</p>' for i in range(60))
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("mimetype", "application/epub+zip")
        archive.writestr("META-INF/container.xml", '<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
        layout = '<meta property="rendition:layout">pre-paginated</meta>' if fixed else ""
        archive.writestr("OPS/book.opf", f'''<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="uid">fixture</dc:identifier><dc:title>{title}</dc:title><dc:creator>测试作者</dc:creator><dc:date>2021-08-01</dc:date><dc:language>zh</dc:language>{layout}</metadata><manifest><item id="one" href="one.xhtml" media-type="application/xhtml+xml"/><item id="two" href="two.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" properties="nav" media-type="application/xhtml+xml"/><item id="cover" href="cover.png" properties="cover-image" media-type="image/png"/></manifest><spine><itemref idref="one"/><itemref idref="two"/></spine></package>''')
        archive.writestr("OPS/one.xhtml", f'<html xmlns="http://www.w3.org/1999/xhtml"><body><h1 id="start">第一章</h1><p>Some <strong>bold</strong> and <em>italic</em> text.</p><ul><li>第一项</li><li>第二项</li></ul><table><tr><th>名称</th><th>值</th></tr><tr><td>项目</td><td>内容</td></tr></table><img src="cover.png" alt="封面"/>{content}<a href="two.xhtml#second">下一章</a></body></html>')
        archive.writestr("OPS/two.xhtml", '<html xmlns="http://www.w3.org/1999/xhtml"><body><h1 id="second">第二章</h1><p id="end">The end. 阅读结束。</p></body></html>')
        archive.writestr("OPS/nav.xhtml", '<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol><li><a href="one.xhtml#p30">章节中间</a></li><li><a href="two.xhtml#second">第二章</a></li></ol></nav></body></html>')
        archive.writestr("OPS/cover.png", png())


def realistic_epub(path, version=3):
    from urllib.parse import quote
    epub(path, title="真实 XHTML")
    with zipfile.ZipFile(path) as archive:
        entries = {name: archive.read(name) for name in archive.namelist()}
    chapter = "中文 %20#.xhtml"
    picture = "封面 %20#).png"
    chapter_url = quote(chapter)
    picture_url = quote(picture)
    anchor = quote("节 %20#")
    opf = entries["OPS/book.opf"].decode().replace("two.xhtml", chapter_url).replace("cover.png", picture_url)
    opf = opf.replace("</manifest>", '<item id="wrapper" href="wrapper.xhtml" media-type="application/xhtml+xml"/><item id="empty" href="empty.xhtml" media-type="application/xhtml+xml"/><item id="css" href="style.css" media-type="text/css"/></manifest>')
    opf = opf.replace('<spine>', '<spine><itemref idref="wrapper"/><itemref idref="empty"/>')
    entries["OPS/book.opf"] = opf.encode()
    prefix = '<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">'
    entries["OPS/wrapper.xhtml"] = (prefix + f'<html xmlns="http://www.w3.org/1999/xhtml" xmlns:xlink="http://www.w3.org/1999/xlink"><body><svg xmlns="http://www.w3.org/2000/svg"><image xlink:href="{picture_url}"/></svg></body></html>').encode()
    entries["OPS/empty.xhtml"] = (prefix + '<html xmlns="http://www.w3.org/1999/xhtml"><body/></html>').encode()
    entries["OPS/one.xhtml"] = (prefix + entries["OPS/one.xhtml"].decode().replace('two.xhtml#second', chapter_url + '#' + anchor).replace('cover.png', picture_url).replace('<p>Some <strong>bold</strong> and <em>italic</em> text.</p>', '<div id="mixed">Hello <b>bold</b> world&nbsp;&copy;. Text mentions vertical-rl.</div>')).encode()
    entries["OPS/one.xhtml"] = entries["OPS/one.xhtml"].decode().replace('</div>', f'</div><p id="internal-link"><a href="{chapter_url}#{anchor}">内部跳转</a></p>').encode()
    entries["OPS/" + chapter] = (prefix + '<html xmlns="http://www.w3.org/1999/xhtml"><body><h1 id="second">第二章</h1><div id="节 %20#">Target <b>bold</b> text.</div><p>Literal # heading - list 1. item `code` ~tilde~ | pipe</p></body></html>').encode()
    del entries["OPS/two.xhtml"]
    entries["OPS/" + picture] = entries.pop("OPS/cover.png")
    entries["OPS/style.css"] = b'.unused { writing-mode: vertical-rl; }'
    entries["OPS/nav.xhtml"] = f'<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol><li><a href="one.xhtml#p30">章节中间</a><ol><li><a href="{chapter_url}#{anchor}">特殊文件名</a></li><li><a href="{chapter_url}#missing">缺失目标</a></li></ol></li></ol></nav></body></html>'.encode()
    if version == 2:
        entries["OPS/book.opf"] = entries["OPS/book.opf"].decode().replace('version="3.0"', 'version="2.0"').replace(' properties="nav"', '').replace(' properties="cover-image"', '').replace('</metadata>', '<meta name="cover" content="cover"/></metadata>').replace('</manifest>', '<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/></manifest>').replace('<spine>', '<spine toc="ncx">').encode()
        entries["OPS/toc.ncx"] = f'<?xml version="1.0"?><!DOCTYPE ncx PUBLIC "-//NISO//DTD ncx 2005-1//EN" "http://www.daisy.org/z3986/2005/ncx-2005-1.dtd"><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head><meta name="dtb:uid" content="fixture"/></head><docTitle><text>真实 XHTML</text></docTitle><navMap><navPoint id="parent" playOrder="1"><navLabel><text>章节中间</text></navLabel><content src="one.xhtml#p30"/><navPoint id="target" playOrder="2"><navLabel><text>特殊文件名</text></navLabel><content src="{chapter_url}#{anchor}"/></navPoint></navPoint></navMap></ncx>'.encode()
    with zipfile.ZipFile(path, "w") as archive:
        for name, data in entries.items():
            archive.writestr(name, data)


def bookmarked_epub(path, invalid_only=False):
    epub(path)
    with zipfile.ZipFile(path) as archive:
        entries = {name: archive.read(name) for name in archive.namelist()}
    links = ''.join(f'<li><a href="one.xhtml#p{i}">段落 {i}</a></li>' for i in range(60))
    children = f'<ol>{links}</ol>' if not invalid_only else ''
    second = '<li><a href="two.xhtml#second">第二章</a></li>' if not invalid_only else ''
    entries["OPS/nav.xhtml"] = f'<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol><li><a href="one.xhtml#missing">无效书签</a>{children}</li>{second}</ol></nav></body></html>'.encode()
    with zipfile.ZipFile(path, "w") as archive:
        for name, data in entries.items():
            archive.writestr(name, data)


class RuntimeCase(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="reader-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name)
        self.library = self.root / "library"
        self.sources = self.root / "sources"
        self.sources.mkdir()
        self.env = os.environ.copy()
        for name in ("HOME", "XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME"):
            self.env[name] = str(self.root / name.lower())

    def commands(self, *commands):
        result = subprocess.run([str(ROOT / "target/debug/reader-driver"), str(self.library)], input="".join(json.dumps(c, ensure_ascii=False) + "\n" for c in commands), text=True, capture_output=True, env=self.env, timeout=60, cwd=ROOT)
        self.assertEqual(result.returncode, 0, result.stderr)
        responses = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual(len(responses), len(commands), result.stdout)
        return responses

    def command(self, **command):
        response = self.commands(command)[0]
        self.assertTrue(response["ok"], response)
        return response["value"]

    def import_epub(self):
        source = self.sources / "sample.epub"
        epub(source)
        result = self.command(op="import", paths=[str(source)])
        self.assertEqual(result[0]["status"], "imported", result)
        return source, result[0]["id"]
