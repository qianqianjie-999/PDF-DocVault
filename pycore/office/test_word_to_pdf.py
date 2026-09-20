"""word_to_pdf 纯逻辑单元测试(stdlib unittest,无额外依赖)。

运行:pycore/.venv/bin/python -m unittest office.test_word_to_pdf -v
"""

import tempfile
import unittest
from pathlib import Path

from office.word_to_pdf import _LO_FILTER, _unique_pdf_path, detect_engines


class TestWordToPdf(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def test_unique_pdf_path_no_conflict(self):
        # 无冲突:直接使用源文件名
        self.assertEqual(_unique_pdf_path(self.dir, "报告"), self.dir / "报告.pdf")

    def test_unique_pdf_path_adds_suffix(self):
        (self.dir / "a.pdf").write_bytes(b"%PDF")
        self.assertEqual(_unique_pdf_path(self.dir, "a"), self.dir / "a~1.pdf")

        (self.dir / "a~1.pdf").write_bytes(b"%PDF")
        self.assertEqual(_unique_pdf_path(self.dir, "a"), self.dir / "a~2.pdf")

    def test_quality_filters_complete(self):
        # 三个等级的过滤器 JSON 均存在且格式合法
        for level in ("high", "medium", "low"):
            self.assertTrue(_LO_FILTER[level].startswith("{"))
            self.assertIn("Quality", _LO_FILTER[level])

    def test_detect_engines_keys(self):
        # 自检报告覆盖全部引擎位(COM/CLI)
        engines = detect_engines()
        for key in ("libreoffice", "word_com", "wps_com", "wps_cli"):
            self.assertIn(key, engines)


class TestPdfQuality(unittest.TestCase):
    """WPS CLI 质量后处理(apply_quality)。"""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        # 手工构造"A4 页(612×792pt) + 3000×3000px 图片"的场景:
        # 有效 DPI = 3000/612×72 ≈ 353,远超各级上限,与 Word 排版一致
        import io

        import pikepdf
        from PIL import Image

        buf = io.BytesIO()
        Image.new("RGB", (3000, 3000), color=(120, 40, 200)).save(
            buf, format="JPEG", quality=95
        )
        pdf = pikepdf.new()
        pdf.add_blank_page(page_size=(612, 792))
        stream = pdf.make_stream(buf.getvalue())
        image_xobj = pdf.make_indirect(stream)
        image_xobj.Type = pikepdf.Name("/XObject")
        image_xobj.Subtype = pikepdf.Name("/Image")
        image_xobj.Width = 3000
        image_xobj.Height = 3000
        image_xobj.ColorSpace = pikepdf.Name("/DeviceRGB")
        image_xobj.BitsPerComponent = 8
        image_xobj.Filter = pikepdf.Name("/DCTDecode")
        page = pdf.pages[0]
        page.Resources = pikepdf.Dictionary(
            XObject=pikepdf.Dictionary(Im0=image_xobj)
        )
        page.Contents = pdf.make_stream(b"q 612 0 0 792 0 0 cm /Im0 Do Q")
        self.pdf = self.dir / "img.pdf"
        pdf.save(self.pdf)

    def tearDown(self):
        self.tmp.cleanup()

    def _image_dims(self, path: Path) -> tuple[int, int]:
        import pikepdf

        with pikepdf.open(path) as pdf:
            for page in pdf.pages:
                for _, xobj in page.get("/Resources", {}).get("/XObject", {}).items():
                    if str(xobj.get("/Subtype", "")) == "/Image":
                        return int(xobj["/Width"]), int(xobj["/Height"])
        return (0, 0)

    def test_high_is_noop(self):
        import shutil

        other = self.dir / "high.pdf"
        shutil.copy(self.pdf, other)
        from office.pdf_quality import apply_quality

        apply_quality(other, "high")
        self.assertEqual(self._image_dims(other), (3000, 3000))

    def test_low_downsamples(self):
        import shutil

        other = self.dir / "low.pdf"
        shutil.copy(self.pdf, other)
        from office.pdf_quality import apply_quality

        apply_quality(other, "low")
        w, h = self._image_dims(other)
        # A4 页宽 612pt → 96 DPI 目标 ≈ 816px;明显小于原图 3000px
        self.assertLess(w, 1500)
        self.assertLess(h, 1500)
        # 体积也应下降(JPEG 60 + 降采样)
        self.assertLess(other.stat().st_size, self.pdf.stat().st_size)


if __name__ == "__main__":
    unittest.main()
