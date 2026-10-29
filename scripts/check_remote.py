"""Test fver without starting services; --runtime explicitly probes an isolated broker.

默认仅 stdlib 单测和模拟子进程；协议检查只访问调用者明确指定的测试服务，并只创建合成 FITS。
"""
import argparse
import contextlib
import io
import json
from pathlib import Path
import shlex
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch, Mock
import urllib.error
import urllib.parse
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "jupyter"))
from fitsviewer import remote


class RemoteCliTests(unittest.TestCase):
    def test_endpoint_is_explicit_and_loopback(self):
        self.assertEqual(remote.endpoint("http://127.0.0.1:8877"), "http://127.0.0.1:8877/")
        self.assertEqual(remote.endpoint("https://viewer.example/proxy/", True), "https://viewer.example/proxy/")
        for value in ["http://example.org", "file:///tmp/file", "http://127.0.0.1:8877@evil.example/", "http://127.0.0.1:8877/?token=secret", "http://localhost:0", "http://localhost:8877/proxy", "http://localhost:8877/#ticket"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                remote.endpoint(value)

    def test_url_precedes_helper_probing_and_never_exposes_control_token(self):
        with tempfile.TemporaryDirectory(prefix="fver-cli-fixture-", dir=ROOT / "qa") as folder:
            path = Path(folder) / "synthetic file.fits"
            path.write_bytes(b"isolated command fixture")
            output, errors = io.StringIO(), io.StringIO()
            result = {"url": "http://127.0.0.1:8877/#open=one-time", "request": {"request_id": "fixture", "route": "helper"}}
            calls = []
            def api(base, route, token, data=None):
                calls.append(route)
                if route.startswith("status?"):
                    self.assertTrue(output.getvalue().startswith(result["url"] + "\n"))
                    return {"request": {"route": "helper", "state": "accepted"}}
                self.assertEqual(data["path"], str(path.resolve()))
                return result
            with patch.object(remote, "_server", return_value={"port": 8877, "public_url": "http://127.0.0.1:8877/", "token": "private-control-fixture"}), patch.object(remote, "api", side_effect=api), patch.object(remote.time, "sleep"), contextlib.redirect_stdout(output), contextlib.redirect_stderr(errors):
                self.assertEqual(remote.command([str(path)]), 0)
            self.assertEqual(calls[0], "register")
            self.assertNotIn("private-control-fixture", output.getvalue() + errors.getvalue())
            self.assertIn("Plain SSH alone", errors.getvalue())

    def test_helper_launch_is_fixed_and_capability_only_in_stdin(self):
        args = remote._parser().parse_args(["helper", "--url", "http://127.0.0.1:8877", "--target", "fixture"])
        identity = "a" * 32
        request = {"request_id": "b" * 32, "attempt_epoch": 1, "route": "helper", "state": "queued", "path": "/synthetic/remote file.fits", "hdu": None}
        child = Mock(); child.poll.return_value = None; child.stdin = Mock(); child.wait.return_value = 0
        def api(base, route, token="", data=None, **kwargs):
            if route == "bootstrap": return {"helper_token": "paired-helper-fixture", "instance": identity, "target": "fixture"}
            if route == "events": return {"instance": identity, "target": "fixture", "requests": [request]}
            if route == "ack": return {"instance": identity, "request": request, "token": "fv1_scoped-fixture"}
            self.fail("Unexpected helper action: " + route)
        with patch.object(remote, "_health", return_value={"viewer": "fitsviewer", "fver": True, "instance": identity}), patch.object(remote, "api", side_effect=api), patch.object(remote.getpass, "getpass", return_value="pair-fixture"), patch.object(remote, "resources", return_value=(ROOT, Path("fixed-viewer"), ROOT / "web")), patch("fitsviewer.build_info.require_module"), patch.object(remote.subprocess, "Popen", return_value=child) as spawn, patch.object(remote.time, "sleep", side_effect=KeyboardInterrupt), contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaises(KeyboardInterrupt): remote.helper(args)
        command = spawn.call_args.args[0]
        self.assertEqual(command, ["fixed-viewer", "--native", "--connect-stdin"])
        self.assertNotIn("fv1_scoped-fixture", " ".join(command))
        payload = json.loads(child.stdin.write.call_args.args[0])
        self.assertEqual(payload["token"], "fv1_scoped-fixture")
        self.assertEqual(payload["api"], "http://127.0.0.1:8877")
        self.assertNotIn("command", payload)
        child.terminate.assert_called_once()
        child.wait.assert_called()

    def test_ssh_target_cannot_be_a_command_or_option(self):
        for value in ["-oProxyCommand=evil", "host;command", "host name", "host$(command)", "host/../../file"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                remote._tunnel(remote._parser().parse_args(["helper", "--ssh=" + value]))

    def _pair_output(self, public_url):
        output = io.StringIO()
        active = {"port": 13007, "public_url": public_url, "token": "private-control-pair-fixture"}
        result = {"instance": "fixture-instance", "pair_code": "one-use-pair-fixture"}
        with patch.object(remote, "_server", return_value=active), patch.object(remote, "api", return_value=result) as api, contextlib.redirect_stdout(output):
            self.assertEqual(remote.command(["pair", "--target", "desk"]), 0)
        api.assert_called_once_with("http://127.0.0.1:13007/", "pair", active["token"], {"target": "desk"})
        text = output.getvalue()
        self.assertNotIn(active["token"], text)
        self.assertIn("one-use-pair-fixture", text)
        commands = [line[line.index("fver helper "):] for line in text.splitlines() if "fver helper " in line]
        parsed = [remote._parser().parse_args(shlex.split(line)[1:]) for line in commands]
        tunnel = [args for args in parsed if args.ssh]
        self.assertEqual(len(tunnel), 1)
        self.assertEqual(tunnel[0].target, "desk")
        self.assertEqual(tunnel[0].remote_port, 13007)
        return text, parsed

    def test_pair_direct_url_explains_required_forward_and_keeps_distinct_ports(self):
        text, commands = self._pair_output("http://127.0.0.1:18007/")
        direct = [args for args in commands if args.url]
        self.assertEqual(len(direct), 1)
        self.assertEqual(remote.endpoint(direct[0].url), "http://127.0.0.1:18007/")
        self.assertEqual(direct[0].target, "desk")
        self.assertIn("already forwarded to remote 127.0.0.1:13007", text)
        self.assertIn("helper-owned SSH forward", text)

    def test_pair_gateway_guidance_never_suggests_an_invalid_helper_url(self):
        for public_url in ["https://viewer.example/", "https://viewer.example/fitsviewer/", "http://127.0.0.1:18007/proxy/"]:
            with self.subTest(public_url=public_url):
                # Public browser gateways are supported, while direct helpers remain restricted.
                self.assertEqual(remote.endpoint(public_url, True), public_url)
                with self.assertRaises(ValueError):
                    remote.endpoint(public_url)
                text, commands = self._pair_output(public_url)
                self.assertFalse(any(args.url for args in commands))
                self.assertIn("Browser address: " + public_url, text)
                self.assertIn("fver YOUR_FILE.fits --target desk --route browser", text)
                self.assertIn("one-time URL printed by that command", text)
                self.assertIn("helper-owned SSH forward", text)


def _fits(path):
    cards = ["SIMPLE  =                    T", "BITPIX  =                   16", "NAXIS   =                    2", "NAXIS1  =                    2", "NAXIS2  =                    2", "END"]
    header = "".join(card.ljust(80) for card in cards).ljust(2880).encode("ascii")
    path.write_bytes(header + struct.pack(">hhhh", 1, 2, 3, 4) + bytes(2872))


def protocol(runtime):
    value = json.loads(Path(runtime).read_text(encoding="utf-8"))
    if not value.get("fver"): raise ValueError("Pass an isolated --fver-broker runtime, never a Jupyter runtime")
    base, control = "http://127.0.0.1:%s/" % value["port"], value["token"]
    health = remote._health(base)
    if health.get("epoch") != value["epoch"] or not health.get("fver"): raise ValueError("Broker identity mismatch")
    target = "qa_" + uuid.uuid4().hex[:12]
    checks = []
    def request(route, token, data=None, status=200, origin=None, host=None):
        headers = {"X-Fitsviewer-Token": token, "Content-Type": "application/json"}
        if origin: headers["Origin"] = origin
        if host: headers["Host"] = host
        body = None if data is None else json.dumps(data).encode()
        call = urllib.request.Request(base + route, data=body, headers=headers)
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), remote._NoRedirect())
        try:
            with opener.open(call, timeout=15) as reply: actual, content = reply.status, reply.read(2_000_000)
        except urllib.error.HTTPError as error: actual, content = error.code, error.read(4096)
        if actual != status: raise AssertionError("%s: expected %s, got %s: %s" % (route.split('?')[0], status, actual, content[:240]))
        checks.append({"endpoint": route.split('?')[0], "status": actual})
        return json.loads(content) if content.startswith(b"{") else content
    with tempfile.TemporaryDirectory(prefix="fver-contract-fixtures-", dir=ROOT / "qa") as folder:
        a, b = Path(folder) / "synthetic α image.fits", Path(folder) / "other file.fits"
        _fits(a); _fits(b)
        registered = remote.api(base, "register", control, {"path": str(a), "target": target, "route": "browser"})
        if control in registered["url"] or "?token=" in registered["url"]: raise AssertionError("Control credential leaked")
        ticket = urllib.parse.parse_qs(urllib.parse.urlsplit(registered["url"]).fragment)["open"][0]
        boot = request("api/fver/bootstrap", "", {"ticket": ticket})
        request("api/fver/bootstrap", "", {"ticket": ticket}, status=403)
        token, intent = boot["token"], boot["request"]
        # Capability URLs use the broker's canonical spelling (not Windows Path's
        # ordinary backslashes). Keep the adversarial other-file path distinct.
        granted_path = intent["path"]
        request("api/fver/events", token, origin="https://untrusted.example", status=403)
        for operation in ["metadata", "full", "tile", "pixel"]:
            suffix = urllib.parse.urlencode({"path": str(b), "version": "fixture"})
            request("api/image/" + operation + "?" + suffix, token, status=403)
        request("api/image?" + urllib.parse.urlencode({"path": str(a)}), token, status=403)
        if health.get("features", {}).get("pro"):
            request("api/file?" + urllib.parse.urlencode({"path": str(a)}), token, status=403)
        metadata = request("api/image/metadata?" + urllib.parse.urlencode({"path": granted_path}), token)
        packet_bytes = metadata.get("full_packet_bytes")
        if not isinstance(packet_bytes, int) or not 0 < packet_bytes <= 20_000_000:
            raise AssertionError("Synthetic image has an invalid or unexpectedly large full packet")
        request("api/science", token, {"operation": "export", "params": {"_viewer_session": "fake-owner", "panels": [{"path": str(b)}]}}, status=403)
        request("api/science", token, {"operation": "spectral_analysis", "params": {"_viewer_session": "fake-owner", "path": str(a), "error_path": str(b)}}, status=403)
        request("api/science", token, {"operation": "job_status", "params": {"_viewer_session": "forged", "job_id": "another-job"}}, status=403)
        request("api/science", token, {"operation": "wcs", "params": {"_viewer_session": "fixture", "path": str(a)}}, status=403)
        for operation in ["vo_download", "vo_datalink", "vo_query"]:
            request("api/science", token, {"operation": operation, "params": {"_viewer_session": "fixture", "_document_generation": 1, "url": "http://127.0.0.1/private"}}, status=403)
        request("api/artifact?id=" + "0" * 32, token, status=403)
        request("api/requests", token, status=403)
        request("api/open", token, {"path": str(a)}, status=403)
        request("api/fver/ack", token, {**intent, "action": "claim"})
        request("api/fver/ack", token, {**intent, "attempt_epoch": 0, "action": "first_frame"}, status=409)
        request("api/fver/ack", token, {**intent, "action": "visible"})
        request("api/fver/ack", token, {**intent, "action": "first_frame"})
        a.write_bytes(a.read_bytes() + b"source changed")
        request("api/image/metadata?" + urllib.parse.urlencode({"path": granted_path}), token, status=409)
        remote.api(base, "cancel", control, {"target": target, "request_id": intent["request_id"]})
        request("api/image/metadata?" + urllib.parse.urlencode({"path": granted_path}), token, status=403)
        # A helper credential can route one target, but cannot itself read its files.
        paired = remote.api(base, "pair", control, {"target": target})
        forwarded_host = "127.0.0.1:44217"
        request("api/fver/bootstrap", "", {"ticket": paired["pair_code"], "mode": "native"}, host=forwarded_host, origin="http://"+forwarded_host, status=403)
        helper = request("api/fver/bootstrap", "", {"ticket": paired["pair_code"], "mode": "native"}, host=forwarded_host)["helper_token"]
        request("api/fver/events", helper, host=forwarded_host)
        opened = remote.api(base, "register", control, {"path": str(b), "target": target, "route": "auto"})["request"]
        claimed = request("api/fver/ack", helper, {**opened, "action": "claim"})
        request("api/image/metadata?" + urllib.parse.urlencode({"path": str(b)}), helper, status=401)
        request("api/image/metadata?" + urllib.parse.urlencode({"path": str(a)}), claimed["token"], status=403)
        remote.api(base, "cancel", control, {"target": target, "request_id": opened["request_id"]})
        request("api/image/metadata?" + urllib.parse.urlencode({"path": str(b)}), claimed["token"], status=401)
    print(json.dumps({"ok": True, "synthetic_fixtures_only": True, "checks": checks}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", help="Explicit isolated broker server.json; no service is launched")
    args, remaining = parser.parse_known_args()
    if args.runtime: protocol(args.runtime)
    else: unittest.main(argv=[sys.argv[0], *remaining])
