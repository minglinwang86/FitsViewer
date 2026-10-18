"""WCS cache ownership/invalidation checks; no builds, services, or Gaia network.

标准库测试缓存生命周期；可选科学测试调用外部 Astropy，不查询 Gaia。
"""
from pathlib import Path
from types import ModuleType, SimpleNamespace
from unittest.mock import patch
import importlib.util
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "web/python"))
import fitsviewer_science as science
from fitsviewer_wcs import model_key


def owner(worker, name="view-a", generation=1, role="view"):
    worker.scope.owner = name
    worker.scope.document_generation = generation
    worker.scope.document_explicit = True
    worker.scope.science_role = role
    worker.scope.role_epoch = 0
    worker.scope.cancel = None


class ModelCache(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="fitsviewer-wcs-", dir=ROOT / "qa")
        self.root = Path(self.directory.name)
        self.assertTrue(self.root.resolve().is_relative_to((ROOT / "qa").resolve()))
        self.path = self.root / "field.fits"
        self.path.write_bytes(b"bounded fake file; scientific arrays are not needed")
        self.worker = science.Worker(self.root / "cache")
        owner(self.worker)
        self.image_calls = []

        def image(params):
            self.image_calls.append(dict(params))
            return self.path, params.get("hdu", 0), {"source": len(self.image_calls)}, None

        self.worker.image = image
        fake = ModuleType("astropy.wcs")
        fake.WCS = lambda header, naxis: SimpleNamespace(celestial=SimpleNamespace(has_celestial=True, header=header))
        self.mock_astropy = patch.dict(sys.modules, {"astropy.wcs": fake})
        self.mock_astropy.start()

    def tearDown(self):
        self.mock_astropy.stop()
        self.worker.close()
        self.directory.cleanup()

    def params(self, **extra):
        return dict(path=str(self.path), hdu=0, **extra)

    def test_moving_cursor_reuses_model_without_reopening_fits(self):
        first = self.worker.celestial(self.params(points=[[0, 0]]))
        second = self.worker.celestial(self.params(points=[[700, 500]]))
        self.assertIs(first, second)
        self.assertEqual(len(self.image_calls), 1)

    def test_hdu_override_and_file_change_never_reuse_old_model(self):
        base = self.worker.celestial(self.params())
        different_hdu = self.worker.celestial(dict(self.params(), hdu=1))
        self.assertIsNot(base, different_hdu)
        solved = self.worker.celestial(self.params(wcs_override="complete model A"))
        self.assertIsNot(solved, self.worker.celestial(self.params(wcs_override="complete model B")))
        self.path.write_bytes(b"changed file size and signature")
        self.assertIsNot(base, self.worker.celestial(self.params()))
        self.assertEqual(len(self.image_calls), 5)

    def test_change_during_header_load_is_rejected_before_caching(self):
        original = self.worker.image

        def changed(params):
            result = original(params)
            self.path.write_bytes(b"file replaced during header read")
            return result

        self.worker.image = changed
        with self.assertRaisesRegex(ValueError, "changed"):
            self.worker.celestial(self.params())
        self.assertFalse(self.worker.headers)

    def test_oversize_or_empty_override_is_rejected_before_file_read(self):
        for text in ["", " ", "x" * 65537, {"malformed": True}]:
            with self.subTest(text_type=type(text).__name__), self.assertRaisesRegex(ValueError, "64 KiB"):
                self.worker.celestial(self.params(wcs_override=text))
        self.assertFalse(self.image_calls)

    def test_model_lru_is_bounded_and_owner_metadata_is_evicted_together(self):
        for hdu in range(13):
            self.worker.celestial(dict(self.params(), hdu=hdu))
        self.assertEqual(len(self.worker.headers), 12)
        self.assertEqual(set(self.worker.headers), set(self.worker.header_owners))
        self.assertNotIn(model_key(self.params(), self.path), self.worker.headers)

    def test_releasing_one_view_keeps_shared_model_until_last_document_releases(self):
        model = self.worker.celestial(self.params())
        owner(self.worker, "view-b", 8)
        self.assertIs(model, self.worker.celestial(self.params()))
        self.worker.release_cached_owner("view-a", headers=True, generation=1)
        self.assertEqual(len(self.worker.headers), 1)
        self.worker.release_cached_owner("view-b", headers=True, generation=7)
        self.assertEqual(len(self.worker.headers), 1)
        self.worker.release_cached_owner("view-b", headers=True, generation=8)
        self.assertFalse(self.worker.headers)
        self.assertFalse(self.worker.header_owners)

    def test_pro_release_preserves_view_owned_j2000_model(self):
        owner(self.worker, role="pro")
        self.worker.celestial(self.params())
        self.worker.release_cached_owner("view-a", headers=False, role="pro", role_epoch=0)
        self.assertEqual(len(self.worker.headers), 1)
        self.worker.release_cached_owner("view-a", headers=True, generation=1)
        self.assertFalse(self.worker.headers)


HAS_SCIENCE = all(importlib.util.find_spec(package) is not None for package in ("numpy", "astropy"))


@unittest.skipUnless(HAS_SCIENCE, "External NumPy/Astropy are required for scientific WCS validation")
class ScientificWcs(unittest.TestCase):
    def test_tan_sip_ra_wrap_and_hdu_override_use_the_full_astropy_model(self):
        import numpy as np
        from astropy.coordinates import FK5, SkyCoord
        from astropy.io import fits
        from astropy.time import Time
        from astropy.wcs import Sip, WCS
        from astropy.wcs.utils import pixel_to_skycoord
        with tempfile.TemporaryDirectory(prefix="fitsviewer-wcs-science-", dir=ROOT / "qa") as directory:
            root = Path(directory)
            self.assertTrue(root.resolve().is_relative_to((ROOT / "qa").resolve()))
            model = WCS(naxis=2)
            model.wcs.ctype = ["RA---TAN-SIP", "DEC--TAN-SIP"]
            model.wcs.crval = [359.999, 60.]
            model.wcs.crpix = [64.5, 48.5]
            model.wcs.cd = np.array([[-.2, .1], [.1, .2]]) / 3600
            model.wcs.radesys = "ICRS"
            a, b = np.zeros((3, 3)), np.zeros((3, 3))
            a[2, 0], b[0, 2] = 2.e-4, -1.e-4
            model.sip = Sip(a, b, None, None, model.wcs.crpix)
            header = model.to_header(relax=True)
            shifted = header.copy()
            shifted["CRVAL1"] = 30.
            path = root / "sip.fits"
            fits.HDUList([fits.PrimaryHDU(np.zeros((96, 128), dtype="float32"), header),
                          fits.ImageHDU(np.zeros((96, 128), dtype="float32"), shifted)]).writeto(path)
            worker = science.Worker(root / "cache")
            owner(worker)
            try:
                points = np.array([[0., 0.], [127., 95.], [63.5, 47.5]])
                params = dict(path=str(path), hdu=0, points=points.tolist())
                actual = worker.wcs(params)
                expected = pixel_to_skycoord(points[:, 0], points[:, 1], model, origin=0, mode="all").transform_to(FK5(equinox=Time("J2000")))
                output = SkyCoord([point["ra_deg"] for point in actual["points"]],
                                  [point["dec_deg"] for point in actual["points"]], unit="deg", frame=FK5(equinox=Time("J2000")))
                self.assertLess(float(output.separation(expected).arcsec.max()), 1.e-6)
                self.assertTrue(actual["distortion"])
                linear_only = pixel_to_skycoord(points[:, 0], points[:, 1], model, origin=0, mode="wcs")
                self.assertGreater(float(linear_only.separation(expected).arcsec.max()), .01)
                with patch.object(worker, "image", wraps=worker.image) as read:
                    worker.wcs(dict(params, points=[[10., 12.]]))
                    worker.wcs(dict(params, points=[[11., 13.]]))
                    read.assert_not_called()
                different_hdu = worker.wcs(dict(params, hdu=1))
                self.assertGreater(abs(different_hdu["points"][0]["ra_deg"] - actual["points"][0]["ra_deg"]), 10.)
                override = model.to_header(relax=True).tostring(sep="\n")
                restored = worker.wcs(dict(params, hdu=1, wcs_override=override))
                self.assertAlmostEqual(restored["points"][0]["ra_deg"], actual["points"][0]["ra_deg"], places=10)
                self.assertEqual(actual["coordinate_origin"], 0)
                self.assertEqual((actual["frame"], actual["equinox"]), ("FK5", "J2000"))
            finally:
                worker.close()


if __name__ == "__main__":
    unittest.main()
