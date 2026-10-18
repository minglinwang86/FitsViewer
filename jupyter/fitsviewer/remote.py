"""Standalone SSH viewing and an optional paired local helper (stdlib only).

远端 fver 仅登记显式文件；本机助手仅启动固定查看器或系统浏览器，不接收任意命令。
"""
import argparse
import getpass
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
import webbrowser

from .runtime import resources, runtime_dir


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ValueError("A paired viewer endpoint must not redirect")


def endpoint(value, public=False):
    parsed = urllib.parse.urlsplit(value)
    if parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise ValueError("Viewer URLs must not contain credentials, queries, or fragments")
    loopback = parsed.hostname in {"127.0.0.1", "localhost", "::1"}
    if not (parsed.scheme == "http" and loopback or public and parsed.scheme == "https" and parsed.hostname):
        raise ValueError("Use an explicit loopback HTTP URL (or a controlled HTTPS gateway for --public-url)")
    if parsed.port == 0 or not parsed.netloc or not public and parsed.path not in {"", "/"}:
        raise ValueError("The local helper needs a direct loopback port, without a proxy path")
    return value.rstrip("/") + "/"


def api(base, route, token="", data=None, timeout=5):
    url = urllib.parse.urljoin(base, "api/fver/" + route)
    headers = {"Accept": "application/json"}
    if token:
        headers["X-Fitsviewer-Token"] = token
    body = None if data is None else json.dumps(data, ensure_ascii=False).encode()
    if body is not None:
        headers["Content-Type"] = "application/json"
    request = urllib.request.Request(url, body, headers)
    # Do not inherit HTTP proxies for a private loopback connection.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), _NoRedirect())
    try:
        with opener.open(request, timeout=timeout) as response:
            payload = response.read(1_048_577)
        if len(payload) > 1_048_576:
            raise ValueError("Viewer reply exceeds 1 MiB")
        return json.loads(payload)
    except urllib.error.HTTPError as error:
        raise RuntimeError(error.read(2048).decode("utf-8", "replace")) from error


def _health(base):
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), _NoRedirect())
    with opener.open(urllib.parse.urljoin(base, "api/health"), timeout=1) as response:
        return json.loads(response.read(8192))


def _private_directory(path):
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    if path.is_symlink():
        raise ValueError("The fver runtime directory must not be a symlink")
    if os.name != "nt":
        if path.stat().st_uid != os.getuid():
            raise ValueError("The fver runtime directory belongs to another user")
        path.chmod(0o700)
    return path


def _read_runtime(path):
    try:
        if path.is_symlink() or os.name != "nt" and (path.stat().st_uid != os.getuid() or path.stat().st_mode & 0o077):
            return None
        value = json.loads(path.read_text(encoding="utf-8"))
        if not value.get("fver") or not value.get("token"):
            return None
        health = _health("http://127.0.0.1:%d/" % value["port"])
        if health.get("viewer") != "fitsviewer" or not health.get("fver") or health.get("epoch") != value.get("epoch"):
            return None
        return value
    except (OSError, ValueError, KeyError, urllib.error.URLError):
        return None


def _server(args):
    directory = _private_directory(runtime_dir() / "fver")
    record = directory / "server.json"
    active = _read_runtime(record)
    if active:
        if args.public_url and endpoint(args.public_url, True) != endpoint(active["public_url"], True):
            raise ValueError("This fver broker already uses %s; use that mapping or an independent FITSVIEWER_RUNTIME_DIR" % active["public_url"])
        return active
    lock = directory / "start.lock"
    identity = uuid.uuid4().hex
    try:
        descriptor = os.open(str(lock), os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    except FileExistsError:
        for _ in range(50):
            time.sleep(.1)
            active = _read_runtime(record)
            if active:
                return active
        raise RuntimeError("Another fver startup is pending; retry shortly. A stale private start.lock may be removed after verifying no startup is running.")
    child = None
    try:
        with os.fdopen(descriptor, "w", encoding="ascii") as handle:
            handle.write(identity)
        root, binary, web = resources()
        public = endpoint(args.public_url, True) if args.public_url else "http://127.0.0.1:%d/" % args.port
        environment = {**os.environ, "FITSVIEWER_HOME": str(root), "FITSVIEWER_RUNTIME_DIR": str(directory),
                       "FITSVIEWER_PYTHON": os.environ.get("FITSVIEWER_PYTHON") or sys.executable}
        command = [str(binary), "--serve", "--fver-broker", "--port", str(args.port), "--root", Path.cwd().anchor,
                   "--web-dir", str(web), "--public-url", public, "--python", environment["FITSVIEWER_PYTHON"]]
        log = directory / "standalone.log"
        descriptor = os.open(str(log), os.O_CREAT | os.O_WRONLY | os.O_APPEND, 0o600)
        with os.fdopen(descriptor, "ab") as output:
            child = subprocess.Popen(command, env=environment, stdin=subprocess.DEVNULL, stdout=output, stderr=output,
                                     **({"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {"start_new_session": True}))
        for _ in range(100):
            if child.poll() is not None:
                raise RuntimeError("Standalone fver server exited. See %s" % log)
            active = _read_runtime(record)
            if active and active["pid"] == child.pid:
                return active
            time.sleep(.1)
        raise RuntimeError("Standalone fver server startup timed out; see %s" % log)
    except BaseException:
        if child is not None and child.poll() is None:
            child.terminate()  # Only the child created by this invocation.
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait(timeout=5)
        raise
    finally:
        if lock.exists() and lock.read_text(encoding="ascii") == identity:
            lock.unlink()


def _parser():
    parser = argparse.ArgumentParser(prog="fver", description="Explicit-file standalone SSH FITS viewer. Requires a real SSH HTTP forward; no Jupyter is used.")
    parser.add_argument("file", nargs="?", help="File to authorize, or pair/status/cancel/helper")
    parser.add_argument("request_id", nargs="?", help="Request ID for status/cancel")
    parser.add_argument("--target", default="default", help="Explicit paired target (default: default)")
    parser.add_argument("--route", choices=["auto", "browser", "native"], default="auto")
    parser.add_argument("--hdu", type=int)
    parser.add_argument("--port", type=int, default=8877, help="Remote loopback server port")
    parser.add_argument("--public-url", help="Browser address of the actual existing HTTP forward or controlled gateway")
    parser.add_argument("--url", help="Helper connection: local forwarded loopback URL")
    parser.add_argument("--ssh", help="Optional trusted system-SSH host alias; helper owns this tunnel")
    parser.add_argument("--remote-port", type=int, default=8877)
    parser.add_argument("--local-port", type=int, default=0)
    parser.add_argument("--mode", choices=["native", "browser"], default="native")
    return parser


def command(arguments=None):
    parser = _parser()
    args = parser.parse_args(arguments)
    if args.file is None:
        parser.print_help()
        return 0
    if not re.fullmatch(r"[A-Za-z0-9_-]{1,64}", args.target):
        parser.error("--target needs 1 to 64 letters, digits, underscores or hyphens")
    if not 1 <= args.port <= 65535 or not 1 <= args.remote_port <= 65535 or not 0 <= args.local_port <= 65535:
        parser.error("Invalid port")
    if args.hdu is not None and args.hdu < 0:
        parser.error("--hdu must be nonnegative")
    if args.file == "helper":
        return helper(args)
    if args.request_id and args.file not in {"status", "cancel"}:
        parser.error("Only status/cancel accept a request ID")
    selected = None
    if args.file not in {"pair", "status", "cancel"}:
        selected = Path(args.file).expanduser().resolve(strict=True)
        if not selected.is_file():
            raise ValueError("Select a regular file")
    active = _server(args)
    base = "http://127.0.0.1:%d/" % active["port"]
    token = active["token"]
    if args.file == "pair":
        result = api(base, "pair", token, {"target": args.target})
        print("Target: %s; server instance: %s" % (args.target, result["instance"]))
        print("Pairing code (one use, 120 seconds): " + result["pair_code"], flush=True)
        print("On your local computer: fver helper --url %s --target %s" % (active["public_url"], args.target))
        print("Or: fver helper --ssh YOUR_TRUSTED_SSH_ALIAS --remote-port %s --target %s" % (active["port"], args.target))
        return 0
    if args.file in {"status", "cancel"}:
        if not args.request_id:
            parser.error("%s requires the request ID printed by fver" % args.file)
        result = api(base, "cancel", token, {"target": args.target, "request_id": args.request_id}) if args.file == "cancel" else api(base, "status?" + urllib.parse.urlencode({"target": args.target, "request_id": args.request_id}), token)
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0
    result = api(base, "register", token, {"target": args.target, "route": args.route, "path": str(selected), "hdu": args.hdu})
    print(result["url"], flush=True)  # Always precedes helper probing and file transfer.
    intent = result["request"]
    print("Request %s; target %s; route %s. Link is one use and expires after 60 seconds." % (intent["request_id"], args.target, intent["route"]), file=sys.stderr, flush=True)
    public = urllib.parse.urlsplit(active["public_url"])
    if public.scheme == "http":
        print("SSH forwarding must already map local %s to remote 127.0.0.1:%s. Plain SSH alone cannot open a local window." % (public.port or 80, active["port"]), file=sys.stderr)
        print("Local setup example: ssh -N -T -L 127.0.0.1:%s:127.0.0.1:%s YOUR_TRUSTED_SSH_ALIAS" % (public.port or 80, active["port"]), file=sys.stderr)
    if intent["route"] == "helper":
        deadline = time.monotonic() + 3.5
        while time.monotonic() < deadline:
            time.sleep(.25)
            state = api(base, "status?" + urllib.parse.urlencode({"target": args.target, "request_id": intent["request_id"]}), token)["request"]
            if state["state"] != "queued" or state["route"] == "browser":
                print("Delivery: %s / %s. First frame is a separate receipt; check fver status %s." % (state["route"], state["state"], intent["request_id"]), file=sys.stderr)
                break
    return 0


def _tunnel(args):
    if not re.fullmatch(r"[A-Za-z0-9_.@:-]{1,253}", args.ssh or "") or args.ssh.startswith("-"):
        raise ValueError("--ssh must be an explicit trusted host alias, without shell syntax or options")
    if args.url:
        raise ValueError("Choose --url or an assistant-owned --ssh tunnel")
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", args.local_port))
        local = probe.getsockname()[1]
    process = subprocess.Popen(["ssh", "-N", "-T", "-o", "ExitOnForwardFailure=yes", "-o", "ForwardAgent=no",
                                "-o", "ServerAliveInterval=15", "-o", "ServerAliveCountMax=3", "-L",
                                "127.0.0.1:%s:127.0.0.1:%s" % (local, args.remote_port), args.ssh], stdin=None,
                               **({"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {}))
    base = "http://127.0.0.1:%s/" % local
    try:
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError("System SSH failed. Check its visible error, known_hosts, login, and forwarding policy")
            try:
                if _health(base).get("fver"):
                    return base, process
            except (OSError, ValueError, urllib.error.URLError):
                pass
            time.sleep(.2)
        raise RuntimeError("SSH tunnel did not reach a standalone fver broker in 30 seconds")
    except BaseException:
        _stop_owned(process)
        raise


def _stop_owned(process):
    if process.poll() is None:
        process.terminate()
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=3)


def helper(args):
    tunnel = None
    token = None
    children = {}
    try:
        if args.ssh:
            base, tunnel = _tunnel(args)
        elif args.url:
            base = endpoint(args.url)
        else:
            raise ValueError("Run this on the local computer with --url LOCAL_FORWARDED_URL or --ssh TRUSTED_ALIAS")
        health = _health(base)
        if not health.get("fver") or health.get("viewer") != "fitsviewer":
            raise ValueError("The local port is not a standalone fver broker")
        code = getpass.getpass("Paste the one-time fver pairing code: ").strip()
        paired = api(base, "bootstrap", data={"ticket": code, "mode": args.mode})
        if paired.get("target") != args.target or paired.get("instance") != health.get("instance"):
            raise ValueError("Pairing target/server does not match the explicit connection")
        token, instance = paired["helper_token"], paired["instance"]
        binary = None
        if args.mode == "native":
            _, binary, _ = resources()
            from .build_info import require_module
            require_module("desktop")
        print("Paired target %s, instance %s, mode %s. Keep this helper running; Ctrl+C closes its own native children/tunnel. Browser tabs manage their own lifetime." % (args.target, instance, args.mode), flush=True)
        failures = 0
        while True:
            if tunnel is not None and tunnel.poll() is not None:
                raise RuntimeError("The assistant-owned SSH tunnel exited")
            try:
                event = api(base, "events", token)
                if event.get("instance") != instance or event.get("target") != args.target:
                    raise ValueError("Server identity changed; pair again")
                failures = 0
            except (OSError, RuntimeError, urllib.error.URLError):
                failures += 1
                if failures >= 6:
                    raise RuntimeError("Paired connection unavailable after six attempts; pair again. HTML remains available from a new fver request.")
                time.sleep(min(2 ** failures, 8))
                continue
            requests = {request["request_id"]: request for request in event["requests"]}
            for identifier, (child, attempt) in list(children.items()):
                request = requests.get(identifier)
                if request is None or request["attempt_epoch"] != attempt or request["state"] in {"cancelled", "expired", "superseded", "closed"}:
                    _stop_owned(child)
                    del children[identifier]
                elif child.poll() is not None:
                    action = "close" if request["state"] == "first_frame" else "failed"
                    print("Native window %s exited (%s); delivery %s" % (identifier, child.returncode, action), file=sys.stderr)
                    try:
                        api(base, "ack", token, {"request_id": identifier, "attempt_epoch": attempt, "action": action})
                    except (OSError, RuntimeError, urllib.error.URLError):
                        pass
                    del children[identifier]
            for request in event["requests"]:
                if request["route"] != "helper" or request["state"] != "queued" or request["request_id"] in children:
                    continue
                if len(children) >= 8:
                    continue
                try:
                    claimed = api(base, "ack", token, {"request_id": request["request_id"], "attempt_epoch": request["attempt_epoch"], "action": "claim"})
                except RuntimeError as error:
                    # A stale claim/limit is local to this intent; keep other windows alive.
                    print("Request was not claimed: " + str(error), file=sys.stderr)
                    continue
                if claimed.get("instance") != instance:
                    raise ValueError("Launch acknowledgement came from a different server")
                intent = claimed["request"]
                if args.mode == "browser":
                    link = api(base, "browser-link", token, {"request_id": intent["request_id"], "attempt_epoch": intent["attempt_epoch"]})
                    opened = webbrowser.open(base + "#open=" + link["ticket"])
                    if not opened:
                        api(base, "ack", token, {**intent, "action": "failed"})
                    continue
                payload = {"api": base.rstrip("/"), "token": claimed["token"], "path": intent["path"], "hdu": intent["hdu"],
                           "request_id": intent["request_id"], "attempt_epoch": intent["attempt_epoch"], "instance": instance}
                child = None
                try:
                    child = subprocess.Popen([str(binary), "--native", "--connect-stdin"], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                             **({"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {}))
                    child.stdin.write(json.dumps(payload, ensure_ascii=False).encode())
                    child.stdin.close()
                    children[intent["request_id"]] = child, intent["attempt_epoch"]
                except (OSError, ValueError):
                    if child is not None:
                        _stop_owned(child)
                    api(base, "ack", token, {**intent, "action": "failed"})
            time.sleep(1)
    finally:
        for identifier, (child, attempt) in children.items():
            if token:
                try:
                    api(base, "ack", token, {"request_id": identifier, "attempt_epoch": attempt, "action": "close"}, timeout=1)
                except (OSError, RuntimeError, urllib.error.URLError):
                    pass
            _stop_owned(child)
        if tunnel is not None:
            _stop_owned(tunnel)


def main():
    try:
        raise SystemExit(command())
    except KeyboardInterrupt:
        print("fver: stopped", file=sys.stderr)
        raise SystemExit(130)
    except (OSError, ValueError, RuntimeError, KeyError, subprocess.SubprocessError, urllib.error.URLError) as error:
        print("fver: " + str(error), file=sys.stderr)
        raise SystemExit(1)


if __name__ == "__main__":
    main()
