"""Regenerate a shared common-Chinese font; fontTools is a build-only tool.
原生和网页共用常用中文字库，安装/运行无需 fontTools。
"""
from pathlib import Path
import argparse
import hashlib
import io
import json
import urllib.request
import fontTools
from fontTools import subset
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parents[1]
BASE = "https://raw.githubusercontent.com/aosp-mirror/platform_frameworks_base/android-4.4.4_r2/data/fonts/"
SOURCE_SHA256 = "05d71b179ef97b82cf1bb91cef290c600a510f77f39b4964359e3ef88378c79d"
UI_TEXT = "中文数据观测星场路径空格（）测试图像光谱星点孔径测光背景坐标压缩文件目录"


def fetch(url):
    with urllib.request.urlopen(url, timeout=120) as response:
        return response.read()


def common_characters():
    # GB2312 level one: B0A1-D7F9, using Python's standard codec.
    # 用 Python 标准编码表生成一级常用字，无额外字表数据库。
    han = {ord(bytes([row, column]).decode("gb2312"))
           for row in range(0xB0, 0xD8) for column in range(0xA1, 0xFF)
           if row < 0xD7 or column <= 0xF9}
    assert len(han) == 3755
    support = set(range(0x20, 0x100))
    # Chinese punctuation, Greek labels, basic mathematics and units.
    # 中文标点、希腊字母、常见数学符号和单位。
    for first, last in [(0x3000, 0x3040), (0xFF00, 0xFF66), (0x370, 0x400),
                        (0x2000, 0x2070), (0x2100, 0x2150), (0x2190, 0x2200), (0x2200, 0x2300)]:
        support.update(range(first, last))
    support.update(map(ord, UI_TEXT))
    return han, support


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, help="Verified upstream TTF; avoids downloading")
    parser.add_argument("--charset", choices=["common", "full"], default="common")
    parser.add_argument("--text-file", type=Path, action="append", default=[], help="Extra UTF-8 characters to retain")
    parser.add_argument("--output", type=Path, default=ROOT / "web/fonts/cjk.ttf")
    args = parser.parse_args()
    source = args.source.read_bytes() if args.source else fetch(BASE + "DroidSansFallback.ttf")
    if hashlib.sha256(source).hexdigest() != SOURCE_SHA256:
        raise ValueError("Upstream font checksum does not match the reviewed source")
    font = TTFont(io.BytesIO(source), recalcTimestamp=False)
    available = set(font.getBestCmap())
    han, support = common_characters()
    requested = han | support
    for path in args.text_file:
        if path.stat().st_size > 256 * 1024:
            raise ValueError("Extra character text must be at most 256 KiB")
        requested.update(map(ord, path.read_text(encoding="utf-8")))
    if args.charset == "common":
        if han - available:
            raise ValueError("Upstream font is missing common Chinese characters")
        options = subset.Options()
        options.hinting = True
        options.notdef_outline = True
        options.recommended_glyphs = True
        options.name_IDs = [0, 1, 2, 3, 4, 5, 6, 13, 14]
        options.name_legacy = True
        options.name_languages = ["*"]
        subsetter = subset.Subsetter(options=options)
        subsetter.populate(unicodes=sorted(requested & available))
        subsetter.subset(font)
        for record in font["name"].names:
            if record.nameID in [1, 4, 6]:
                name = "FitsViewerCJKCommon" if record.nameID == 6 else "FitsViewer CJK Common"
                record.string = name.encode(record.getEncoding())
        buffer = io.BytesIO()
        font.save(buffer)
        data = buffer.getvalue()
    else:
        data = source
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(data)
    cmap = TTFont(io.BytesIO(data)).getBestCmap()
    assert han <= set(cmap) and all(ord(c) in cmap for c in UI_TEXT)
    notice = ROOT / "licenses/fonts/droid"
    notice.mkdir(parents=True, exist_ok=True)
    for name, url in [("NOTICE.txt", BASE + "NOTICE"),
                      ("LICENSE-APACHE.txt", "https://www.apache.org/licenses/LICENSE-2.0.txt")]:
        if not (notice / name).is_file():
            (notice / name).write_bytes(fetch(url))
    manifest = {"upstream": "Droid Sans Fallback", "license": "Apache-2.0",
                "source_url": BASE + "DroidSansFallback.ttf", "source_tag": "android-4.4.4_r2",
                "source_sha256": SOURCE_SHA256, "source_bytes": len(source),
                "asset_sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data),
                "mapped_codepoints": len(cmap), "common_han_characters": len(han), "charset": args.charset,
                "fonttools_version": fontTools.__version__,
                "modifications": "Common-character subset; renamed FitsViewer CJK Common; hinting retained" if args.charset == "common" else "None",
                "coverage_note": "GB2312 level-one Chinese, punctuation and scientific symbols. Rare/traditional characters are not guaranteed; --charset full or --text-file extends coverage."}
    args.output.with_name("font-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(manifest))


if __name__ == "__main__":
    main()
