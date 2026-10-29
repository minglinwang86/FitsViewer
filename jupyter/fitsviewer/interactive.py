"""Authenticated manual image-point selection; no scientific packages are imported.

Image coordinates refer to original HDU pixel centres: origin=0 is NumPy/Astropy,
origin=1 is DS9 image convention. A click is not a centroid or a measurement.
"""
from dataclasses import dataclass
import json
import math
import os
from pathlib import Path
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import webbrowser

from .remote import _NoRedirect, endpoint

TERMINAL = frozenset({"completed", "cancelled", "expired", "failed"})


class SelectionError(RuntimeError):
    """An interactive selection failed without a valid completed result."""


class SelectionCancelled(SelectionError):
    pass


class SelectionTimeout(SelectionError, TimeoutError):
    pass


class SelectionChanged(SelectionError):
    """An already-consumed DS9-style point was removed or changed."""


def _integer(value, name, minimum, maximum):
    if isinstance(value, bool) or not isinstance(value, int) or not minimum <= value <= maximum:
        raise ValueError(f"{name} must be an integer from {minimum} to {maximum}")
    return value


def _number(value, name, minimum, maximum):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or not minimum <= value <= maximum:
        raise ValueError(f"{name} must be finite and between {minimum} and {maximum}")
    return value


@dataclass(frozen=True)
class Source:
    path: str
    hdu: int
    width: int
    height: int
    identity: str


@dataclass(frozen=True)
class SelectionResult:
    id: str
    points: tuple
    source: Source
    origin: int
    coordinate_system: str = "image"


class ViewerClient:
    """Connect to an existing authenticated ordinary FitsViewer service.

    ``base_url`` includes the /fitsviewer/ proxy prefix for Jupyter. ``token``
    uses X-Fitsviewer-Token; ``jupyter_token`` uses Jupyter's Authorization
    header. Supply exactly one. HTTP is limited to loopback; HTTPS requires an
    explicit trusted gateway URL. Redirects and environment proxies are disabled.
    The isolated fver capability broker is not this API's transport.
    """

    def __init__(self, base_url, *, token=None, jupyter_token=None, request_timeout=5):
        self.base_url = endpoint(base_url, public=True)
        if bool(token) == bool(jupyter_token):
            raise ValueError("Supply exactly one FitsViewer token or Jupyter token")
        secret = token or jupyter_token
        if not isinstance(secret, str) or not secret or len(secret) > 4096 or any(ord(c) < 33 or ord(c) > 126 for c in secret):
            raise ValueError("Invalid authentication credential")
        self._headers = {"Accept": "application/json", "Content-Type": "application/json"}
        self._headers["X-Fitsviewer-Token" if token else "Authorization"] = secret if token else "token " + secret
        self.request_timeout = _number(request_timeout, "request_timeout", .1, 60)
        self._opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), _NoRedirect())
        self._process = None
        self._temporary = None
        self._sessions = []

    def _request(self, route, data=None, *, timeout=None):
        request = urllib.request.Request(
            urllib.parse.urljoin(self.base_url, "api/" + route),
            None if data is None else json.dumps(data, ensure_ascii=False, allow_nan=False).encode("utf-8"),
            self._headers,
        )
        try:
            with self._opener.open(request, timeout=timeout or self.request_timeout) as response:
                body = response.read(1_048_577)
        except urllib.error.HTTPError as error:
            # Do not echo untrusted server HTML, URLs or authentication data.
            raise SelectionError(f"FitsViewer HTTP {error.code}; verify authorization, path and selection state") from None
        except (OSError, ValueError) as error:
            raise SelectionError("FitsViewer connection failed; check the service and explicit tunnel") from error
        if len(body) > 1_048_576:
            raise SelectionError("FitsViewer response exceeds 1 MiB")
        try:
            value = json.loads(body)
        except (ValueError, UnicodeError):
            raise SelectionError("FitsViewer returned invalid JSON") from None
        if not isinstance(value, dict):
            raise SelectionError("FitsViewer response must be a JSON object")
        return value

    @classmethod
    def from_runtime(cls, path=None):
        """Attach to this OS user's private ordinary-server runtime record."""
        if path is None:
            from .runtime import runtime_dir
            path = runtime_dir() / "server.json"
        path = Path(path)
        if path.is_symlink():
            raise ValueError("Runtime record must not be a symlink")
        info = path.stat()
        if os.name != "nt" and (info.st_uid != os.getuid() or info.st_mode & 0o077):
            raise ValueError("Runtime record must belong to this user with private permissions")
        if info.st_size > 16384:
            raise ValueError("Runtime record exceeds 16 KiB")
        value = json.loads(path.read_text(encoding="utf-8"))
        if value.get("fver"):
            raise ValueError("The fver broker does not support the ordinary selection API")
        if value.get("jupyter"):
            raise ValueError("Use the authenticated Jupyter proxy URL and jupyter_token")
        port = _integer(value.get("port"), "port", 1, 65535)
        client = cls(f"http://127.0.0.1:{port}/", token=value.get("token"))
        health = client._request("health")
        if health.get("viewer") != "fitsviewer" or health.get("fver") or health.get("epoch") != value.get("epoch"):
            raise SelectionError("Runtime record is stale or belongs to a different service")
        return client

    @classmethod
    def start(cls, root, *, port=0, public_url=None, startup_timeout=30):
        """Start one owned loopback service with a fresh private runtime directory.

        root is the caller's explicit readable dataset root. No SSH setting is
        changed and no pre-existing service is stopped. Use ``close`` (or with)
        to stop only this newly created child after the selection is finished.
        """
        from .runtime import resources
        port = _integer(port, "port", 0, 65535)
        startup_timeout = _number(startup_timeout, "startup_timeout", 1, 120)
        root = Path(root).expanduser().resolve(strict=True)
        if not root.is_dir():
            raise ValueError("root must be a directory")
        home, binary, web = resources()
        temporary = tempfile.TemporaryDirectory(prefix="fitsviewer-selection-")
        environment = dict(os.environ, FITSVIEWER_HOME=str(home), FITSVIEWER_RUNTIME_DIR=temporary.name)
        command = [str(binary), "--serve", "--no-browser", "--port", str(port), "--root", str(root), "--web-dir", str(web)]
        if public_url is not None:
            command.extend(["--public-url", endpoint(public_url, public=True)])
        process = None
        try:
            with open(Path(temporary.name) / "service.log", "ab") as output:
                options = {"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {}
                process = subprocess.Popen(command, env=environment, stdin=subprocess.DEVNULL, stdout=output, stderr=output, **options)
            deadline = time.monotonic() + startup_timeout
            record = Path(temporary.name) / "server.json"
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise SelectionError("Owned FitsViewer service exited during startup")
                if record.exists():
                    try:
                        client = cls.from_runtime(record)
                    except (OSError, ValueError, SelectionError):
                        pass  # A record can exist while its JSON is being written.
                    else:
                        client._process, client._temporary = process, temporary
                        return client
                time.sleep(.1)
            raise SelectionTimeout("Owned FitsViewer service did not become ready")
        except BaseException:
            if process is not None and process.poll() is None:
                process.terminate()
                process.wait(timeout=10)
            temporary.cleanup()
            raise

    def start_selection(self, path, *, hdu=None, origin=0, max_points=4096, timeout=600, view=None, prompt="", markers=None):
        """Open a file and return a cancellable handle; call its wait() for results.

        The file is resolved by the server inside its configured read roots, not
        by the Python client's filesystem. ``url`` can be opened on the local
        browser through an already-established SSH forward.
        """
        path = os.fspath(path)
        if not isinstance(path, str) or not path or len(path) > 16384 or "\x00" in path:
            raise ValueError("path must be a non-empty server-side file path")
        _integer(origin, "origin", 0, 1)
        if hdu is not None:
            _integer(hdu, "hdu", 0, 4095)
        _integer(max_points, "max_points", 1, 4096)
        _number(timeout, "timeout", 1, 1800)
        data = {"action": "start", "path": path, "hdu": hdu, "max_points": max_points, "timeout_seconds": math.ceil(timeout),
                "view": view or {}, "prompt": prompt, "markers": markers or []}
        value = self._request("selection", data)
        selection = Selection(self, value, origin=origin, timeout=timeout, requested_hdu=hdu)
        self._sessions = [active for active in self._sessions if active._snapshot["status"] not in TERMINAL]
        self._sessions.append(selection)
        return selection

    select_stars = start_selection

    def close(self):
        """Cancel this client's unfinished sessions; stop only its owned child."""
        for selection in self._sessions:
            if selection._snapshot["status"] not in TERMINAL:
                try:
                    selection.cancel()
                except SelectionError:
                    pass
        if self._process is not None:
            if self._process.poll() is None:
                self._process.terminate()
                self._process.wait(timeout=15)
            self._process = None
            self._temporary.cleanup()
            self._temporary = None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


class Selection:
    """A bounded session. Cancellation never replaces an already completed result."""

    def __init__(self, client, snapshot, *, origin, timeout, requested_hdu):
        self._client, self.origin = client, origin
        self._requested_hdu = requested_hdu
        self._deadline = time.monotonic() + timeout
        self.id = snapshot.get("id")
        if not isinstance(self.id, str) or not self.id or len(self.id) > 128:
            raise SelectionError("Missing selection identity")
        self.url = snapshot.get("url", "")
        self._snapshot = {}
        self._source = None
        self._accept(snapshot)

    def __repr__(self):
        return f"Selection(status={self._snapshot['status']!r}, origin={self.origin})"

    def _accept(self, value):
        if value.get("schema") != 1 or value.get("id") != self.id or value.get("coordinate_system") != "image" or value.get("origin") != 0:
            raise SelectionError("Selection identity or coordinate contract changed")
        status = value.get("status")
        if status not in TERMINAL | {"waiting", "selecting"}:
            raise SelectionError("Unknown selection status")
        revision = value.get("revision", 0)
        _integer(revision, "revision", 0, 2**53)
        if revision < self._snapshot.get("revision", 0):
            raise SelectionError("Stale selection revision")
        if self._snapshot.get("status") in TERMINAL and value != self._snapshot:
            raise SelectionError("Terminal selection changed")
        source = value.get("source")
        if source is not None:
            if not isinstance(source, dict) or not isinstance(source.get("path"), str) or not source["path"] or not isinstance(source.get("identity"), str) or not source["identity"]:
                raise SelectionError("Selection lacks source provenance")
            source = Source(source["path"], _integer(source.get("hdu"), "hdu", 0, 1_000_000),
                            _integer(source.get("width"), "width", 1, 2**53), _integer(source.get("height"), "height", 1, 2**53), source["identity"])
            if self._requested_hdu is not None and source.hdu != self._requested_hdu:
                raise SelectionError("Viewer selected a different HDU")
            if self._source is not None and source != self._source:
                raise SelectionError("Selection source changed")
            self._source = source
        points = value.get("points")
        if not isinstance(points, list) or len(points) > 4096 or (points and self._source is None):
            raise SelectionError("Invalid point list")
        for point in points:
            if not isinstance(point, list) or len(point) != 2:
                raise SelectionError("Expected image x,y coordinates")
            _number(point[0], "x", 0, self._source.width)
            _number(point[1], "y", 0, self._source.height)
            if point[0] >= self._source.width or point[1] >= self._source.height:
                raise SelectionError("Point is outside the source image")
        if status == "completed" and self._source is None:
            raise SelectionError("Completed selection lacks source provenance")
        self._snapshot = value
        return value

    def poll(self):
        return self._accept(self._client._request("selection?" + urllib.parse.urlencode({"id": self.id})))

    def configure(self, *, view=None, prompt=None, markers=None):
        """Update the limited selection presentation contract; never modify pixels."""
        data = {"action": "configure", "id": self.id}
        for key, value in (("view", view), ("prompt", prompt), ("markers", markers)):
            if value is not None:
                data[key] = value
        return self._accept(self._client._request("selection", data))

    def cancel(self):
        return self._accept(self._client._request("selection", {"action": "cancel", "id": self.id}))

    def open_browser(self):
        parsed = urllib.parse.urlsplit(self.url)
        if parsed.scheme not in {"http", "https"} or not parsed.hostname or parsed.username or parsed.password:
            raise SelectionError("Viewer did not return a valid browser URL")
        return webbrowser.open(self.url)

    def _result(self):
        status = self._snapshot["status"]
        if status == "completed":
            return SelectionResult(self.id, tuple((x + self.origin, y + self.origin) for x, y in self._snapshot["points"]), self._source, self.origin)
        if status == "cancelled":
            raise SelectionCancelled("User or client cancelled selection; partial points are not a completed result")
        if status == "expired":
            raise SelectionTimeout("Selection expired; start a new selection")
        if status == "failed":
            raise SelectionError("Viewer could not complete selection; inspect its displayed error")
        return None

    def wait(self, *, timeout=None, poll_interval=.15):
        """Block until completion; timeout/KeyboardInterrupt sends cancellation.

        If completion wins a timeout/cancel race, return that completed result.
        Server expiry still bounds lifetime if the connection is lost.
        """
        _number(poll_interval, "poll_interval", .02, 5)
        deadline = self._deadline if timeout is None else min(self._deadline, time.monotonic() + _number(timeout, "timeout", .001, 1800))
        try:
            while True:
                result = self._result()
                if result is not None:
                    return result
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    self.cancel()
                    if self._snapshot["status"] == "completed":
                        return self._result()
                    raise SelectionTimeout("Selection wait timed out and cancellation was requested")
                self.poll()
                if self._snapshot["status"] not in TERMINAL:
                    time.sleep(min(poll_interval, max(0, deadline - time.monotonic())))
        except KeyboardInterrupt:
            self.cancel()
            raise


__all__ = ["ViewerClient", "Selection", "SelectionResult", "Source", "SelectionError", "SelectionCancelled", "SelectionTimeout", "SelectionChanged"]
