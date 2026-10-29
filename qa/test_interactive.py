"""Focused stdlib selection/client tests; does not start a viewer or touch user FITS."""
import copy
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "jupyter"))
from fitsviewer.interactive import (ViewerClient, Selection, SelectionError,
                                   SelectionCancelled, SelectionTimeout, SelectionChanged)
from fitsviewer.pyec import PyECViewer, UnsupportedDS9Command, make_pyec_xparun


def snapshot(status="selecting", points=None, revision=1, **changes):
    value = {"schema": 1, "id": "fixture-selection", "status": status,
             "coordinate_system": "image", "origin": 0, "revision": revision,
             "config_revision": 1, "view_revision": 1, "points": [] if points is None else points,
             "source": {"path": "synthetic image.fits", "hdu": 1, "width": 100, "height": 80, "identity": "fixture-version"},
             "url": "http://127.0.0.1:12345/?selection=fixture#fv_token=private-fixture",
             "view": {"low": -2.5, "high": 100}}
    value.update(changes)
    return value


def selection(value=None, origin=0):
    client = Mock()
    value = snapshot() if value is None else value
    obj = Selection(client, copy.deepcopy(value), origin=origin, timeout=600, requested_hdu=1)
    return client, obj


class SelectionTests(unittest.TestCase):
    def test_completed_result_has_explicit_origin_hdu_and_provenance(self):
        for origin in (0, 1):
            _, obj = selection(snapshot("completed", [[0, 0], [2.25, 4.5]]), origin)
            result = obj.wait()
            self.assertEqual(result.points, ((origin, origin), (2.25 + origin, 4.5 + origin)))
            self.assertEqual(result.source.hdu, 1)
            self.assertEqual(result.source.identity, "fixture-version")
            self.assertEqual(result.coordinate_system, "image")
            self.assertNotIn("private-fixture", repr(obj))

    def test_cancel_and_expiry_do_not_return_partial_points(self):
        for status, error in (("cancelled", SelectionCancelled), ("expired", SelectionTimeout), ("failed", SelectionError)):
            _, obj = selection(snapshot(status, [[5, 8]]))
            with self.subTest(status=status), self.assertRaises(error):
                obj.wait()

    def test_wait_timeout_cancels_and_completion_race_is_preserved(self):
        for final in ("cancelled", "completed"):
            client, obj = selection()
            obj._deadline = 0
            client._request.return_value = snapshot(final, [[1, 2]], revision=2)
            if final == "cancelled":
                with self.assertRaises(SelectionTimeout):
                    obj.wait()
            else:
                self.assertEqual(obj.wait().points, ((1, 2),))
            self.assertEqual(client._request.call_args.args[1], {"action": "cancel", "id": obj.id})

    def test_keyboard_interrupt_requests_cancel(self):
        client, obj = selection()
        client._request.side_effect = [KeyboardInterrupt(), snapshot("cancelled", revision=2)]
        with self.assertRaises(KeyboardInterrupt):
            obj.wait()
        self.assertEqual(client._request.call_args.args[1]["action"], "cancel")

    def test_wrong_hdu_origin_source_nonfinite_and_outside_points_rejected(self):
        bad = [snapshot(origin=1), snapshot(coordinate_system="physical"),
               snapshot(points=[[float("nan"), 2]]), snapshot(points=[[100, 3]]),
               snapshot(points=[[-.51, 0]])]
        value = snapshot(); value["source"]["hdu"] = 2; bad.append(value)
        value = snapshot(); value["source"]["identity"] = ""; bad.append(value)
        for value in bad:
            with self.subTest(value=value), self.assertRaises((ValueError, SelectionError)):
                selection(value)
        _, obj = selection()
        changed = snapshot(revision=2); changed["source"]["identity"] = "replacement-file"
        with self.assertRaises(SelectionError):
            obj._accept(changed)

    def test_fractional_last_pixel_matches_host_and_viewer_bounds(self):
        _, obj = selection(snapshot("completed", [[99.75, 79.75]]))
        self.assertEqual(obj.wait().points, ((99.75, 79.75),))

    def test_revisions_and_terminal_state_cannot_regress(self):
        _, obj = selection(snapshot(revision=4))
        with self.assertRaises(SelectionError):
            obj._accept(snapshot(revision=3))
        obj._accept(snapshot("completed", revision=5))
        with self.assertRaises(SelectionError):
            obj._accept(snapshot("cancelled", revision=6))


class ClientTests(unittest.TestCase):
    def test_header_auth_and_proxy_prefix_never_query_credential(self):
        for name in ("token", "jupyter_token"):
            client = ViewerClient("https://viewer.example/user/test/fitsviewer/", **{name: "private-fixture"})
            response = Mock(); response.__enter__ = Mock(return_value=response); response.__exit__ = Mock(return_value=None)
            response.read.return_value = b'{"ok":true}'
            client._opener = Mock(); client._opener.open.return_value = response
            client._request("selection", {"action": "cancel", "id": "fixture"})
            request = client._opener.open.call_args.args[0]
            self.assertEqual(request.full_url, "https://viewer.example/user/test/fitsviewer/api/selection")
            self.assertNotIn("private-fixture", request.full_url)
            expected = "private-fixture" if name == "token" else "token private-fixture"
            self.assertEqual(request.get_header("X-fitsviewer-token" if name == "token" else "Authorization"), expected)

    def test_invalid_auth_endpoints_and_unbounded_inputs(self):
        for url in ("http://viewer.example/", "https://secret@viewer.example/", "https://viewer.example/?token=secret"):
            with self.assertRaises(ValueError):
                ViewerClient(url, token="fixture")
        for auth in ({}, {"token": "a", "jupyter_token": "b"}, {"token": "bad\r\nheader"}):
            with self.assertRaises(ValueError):
                ViewerClient("http://127.0.0.1:12345", **auth)
        client = ViewerClient("http://127.0.0.1:12345", token="fixture")
        for parameters in ({"origin": True}, {"hdu": -1}, {"max_points": 0}, {"timeout": float("inf")}, {"max_points": 4097}):
            with self.assertRaises(ValueError):
                client.start_selection("fixture.fits", **parameters)

    def test_runtime_rejects_fver_jupyter_wrong_owner_mode_and_stale_epoch(self):
        with tempfile.TemporaryDirectory(prefix="selection-contract-") as directory:
            path = Path(directory) / "server.json"
            base = {"port": 12345, "token": "fixture", "epoch": "old", "fver": False, "jupyter": False}
            for changes in ({"fver": True}, {"jupyter": True}, {}):
                path.write_text(json.dumps(dict(base, **changes))); path.chmod(0o600)
                with patch.object(ViewerClient, "_request", return_value={"viewer": "fitsviewer", "epoch": "new"}), self.assertRaises((ValueError, SelectionError)):
                    ViewerClient.from_runtime(path)
            if sys.platform != "win32":
                path.chmod(0o644)
                with self.assertRaises(ValueError):
                    ViewerClient.from_runtime(path)


class PyECAdapterTests(unittest.TestCase):
    def viewer(self, replies):
        client, active = selection()
        client.start_selection.return_value = active
        client._request.side_effect = replies
        viewer = PyECViewer(client, open_browser=False, on_url=lambda _: None)
        viewer.display("synthetic image.fits", hdu=1)
        return client, viewer

    def test_actual_pyec_decode_x_loop_and_q_use_one_based_image(self):
        client, viewer = self.viewer([snapshot(points=[[0.25, 1.5]], revision=2),
                                      snapshot("completed", [[0.25, 1.5], [5, 6]], revision=3),
                                      snapshot("completed", [[0.25, 1.5], [5, 6]], revision=3)])
        returned = []
        while True:
            output = viewer.xpaget("iexam key coordinate image").decode()
            if output[0] == "q":
                break
            self.assertEqual(output[0], "x")
            returned.append([float(x) for x in output.split()[1:]])
        self.assertEqual(returned, [[1.25, 2.5], [6, 7]])

    def test_undo_of_already_consumed_point_is_explicit(self):
        _, viewer = self.viewer([snapshot(points=[[1, 2]], revision=2), snapshot(points=[], revision=3)])
        self.assertTrue(viewer.get("iexam key coordinate image").startswith("x "))
        with self.assertRaises(SelectionChanged):
            viewer.get("iexam key coordinate image")

    def test_cancel_does_not_fabricate_q(self):
        _, viewer = self.viewer([snapshot("cancelled", revision=2)])
        with self.assertRaises(SelectionCancelled):
            viewer.xpaget("iexam key coordinate image")

    def test_q_during_pyec_processing_keeps_the_last_point_valid(self):
        client, viewer = self.viewer([snapshot("completed", [[2, 3]], revision=2)])
        self.assertEqual(viewer.get("iexam key coordinate image"), "x 3 4")
        before = client._request.call_count
        viewer.xpa_region([[3, 4]], False)
        self.assertEqual(client._request.call_count, before)

    def test_completion_winning_feedback_request_does_not_abort_pyec(self):
        client, viewer = self.viewer([snapshot(points=[[2, 3]], revision=2),
                                      SelectionError("Completed during feedback"),
                                      snapshot("completed", [[2, 3]], revision=3)])
        self.assertEqual(viewer.get("iexam key coordinate image"), "x 3 4")
        viewer.xpa_region([[3, 4]], False)
        self.assertEqual(viewer.selection._snapshot["status"], "completed")

    def test_scale_limits_wait_for_real_view_ack(self):
        first = snapshot(revision=2, config_revision=2, view_revision=1)
        second = snapshot(revision=3, config_revision=2, view_revision=2, view={"low": 10, "high": 60})
        client, viewer = self.viewer([first, second])
        with patch("fitsviewer.pyec.time.sleep"):
            self.assertEqual(viewer.get("scale limits"), "10 60")
        self.assertEqual(client._request.call_count, 2)

    def test_marker_conversion_and_view_subset_are_explicit(self):
        client, viewer = self.viewer([snapshot(revision=2), snapshot(revision=3), snapshot(revision=4)])
        viewer.xpa_region([[1, 1], [3.5, 9]], True)
        markers = client._request.call_args.args[1]["markers"]
        self.assertEqual([(m["x"], m["y"]) for m in markers], [(0, 0), (2.5, 8)])
        viewer.xpaset("scale limits -10 200")
        self.assertEqual(client._request.call_args.args[1]["view"], {"low": -10, "high": 200})
        viewer.xpaset("regions delete")
        self.assertEqual(client._request.call_args.args[1]["markers"], [])
        for command in ("iexam key coordinate physical", "iexam coordinate wcs fk5"):
            with self.assertRaises(UnsupportedDS9Command):
                viewer.get(command)
        for command in ("frame 2", "catalog sdss", "scale log", "regions load /tmp/not-read.reg"):
            with self.assertRaises(UnsupportedDS9Command):
                viewer.set(command)
        viewer.csystem = "fk5"
        with self.assertRaises(UnsupportedDS9Command):
            viewer.xpa_region([[1, 1]], False)

    def test_factory_is_explicit_and_does_not_import_or_modify_pyec(self):
        client = Mock(); client._request.return_value = {"viewer": "fitsviewer"}
        before = set(sys.modules)
        factory = make_pyec_xparun(client, open_browser=False)
        viewer = factory(False)
        self.assertTrue(viewer.status)
        self.assertFalse(any(name.startswith("ecsnoopy") for name in set(sys.modules) - before))


if __name__ == "__main__":
    unittest.main(verbosity=2)
