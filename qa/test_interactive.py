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
                                   SelectionCancelled, SelectionTimeout)


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


if __name__ == "__main__":
    unittest.main(verbosity=2)
