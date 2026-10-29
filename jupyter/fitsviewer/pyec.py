"""Explicit opt-in adapter for ECsnoopy's xparun selection subset.

This is a Python adapter, not an XPA server or a complete pyds9 implementation.
It never imports/modifies ECsnoopy. Its original DS9 backend stays the default.
"""
import math
import os
import shlex
import time

from .interactive import SelectionChanged, SelectionError, SelectionTimeout, ViewerClient


class UnsupportedDS9Command(NotImplementedError):
    pass


class PyECViewer:
    """The display/xpaget/xpaset helpers used by PyEC's manual-star loops.

    DS9 image coordinates use one-based pixel centres. Python API results can
    instead use origin=0. No centroid fitting, clipping or photometry occurs
    here. ``on_url`` receives the private authorization link only when explicitly
    requested; it must not write that link to shared or persistent logs.
    """

    def __init__(self, client, *, timeout=600, open_browser=True, on_url=None):
        self.client = client
        self.timeout = timeout
        self.open_browser = open_browser
        self.on_url = on_url
        self.size, self.color, self.lcolor = 15, "green", "black"
        self.fontsize, self.csystem, self.shape, self.label = 24, "image", "circle", True
        self.status = True
        self.selection = None
        self._view = {"cmap": "grey", "invert": True, "limits": "zscale", "fit": True}
        self._markers, self._emitted = [], []
        self._opened = False

    @property
    def open_ds9(self):
        """Check the chosen FitsViewer service; never start DS9 or XPA."""
        self.status = self.client._request("health").get("viewer") == "fitsviewer"
        return self.status

    @property
    def url(self):
        return self.selection.url if self.selection is not None else None

    def _require(self):
        if self.selection is None:
            raise SelectionError("Call display(path) before requesting interactive coordinates")
        return self.selection

    def _image_only(self):
        if self.csystem.lower() != "image":
            raise UnsupportedDS9Command("This adapter supports image coordinates only; physical/FK5/WCS need an explicit transformation")

    def display(self, img, **kwargs):
        """Open a server-side file. pix/pix2 are explicitly DS9 image origin=1."""
        unknown = set(kwargs) - {"pix", "pix2", "label", "hdu"}
        if unknown:
            raise UnsupportedDS9Command("Unsupported display options: " + ", ".join(sorted(unknown)))
        self._image_only()
        if self.selection is not None and self.selection._snapshot["status"] not in {"completed", "cancelled", "expired", "failed"}:
            self.selection.cancel()
        self._markers, self._emitted = [], []
        self.selection = self.client.start_selection(os.fspath(img), hdu=kwargs.get("hdu"), origin=1,
                                                     timeout=self.timeout, view=self._view)
        if "pix" in kwargs:
            self.xpa_region(kwargs["pix"], kwargs.get("label", self.label), color="red")
        if "pix2" in kwargs:
            self.xpa_region(kwargs["pix2"], False, shape="box", color="green", size=2 * self.size)
        if not self._opened:
            if self.on_url is not None:
                self.on_url(self.selection.url)
            opened = self.selection.open_browser() if self.open_browser else False
            if not opened and self.on_url is None:
                print("FitsViewer: open viewer.url in your local browser; keep its authorization fragment private.")
            self._opened = bool(opened or self.on_url is not None)
        return self.selection

    def _configure(self, **kwargs):
        if self.selection is not None:
            # Q can arrive while PyEC is processing a previously queued click.
            # Its coordinates remain valid; optional display feedback must not
            # fail that computation after the user has finished the viewer.
            if self.selection._snapshot["status"] == "completed":
                return
            try:
                self.selection.configure(**kwargs)
            except SelectionError:
                # Completion can also win between the local check and POST.
                if self.selection.poll()["status"] != "completed":
                    raise

    def xpa_label(self, x, y, text):
        """Show the instruction in the selection banner; x/y are not measured."""
        self._image_only()
        if not isinstance(text, str) or len(text) > 256:
            raise ValueError("Selection instruction must be at most 256 characters")
        self._require(); self._configure(prompt=text)

    def xpa_region(self, xy, text, **kwargs):
        """Add non-measuring circle/box markers at one-based image coordinates."""
        self._image_only()
        unknown = set(kwargs) - {"shape", "size", "color"}
        if unknown:
            raise UnsupportedDS9Command("Unsupported marker options: " + ", ".join(sorted(unknown)))
        shape = kwargs.get("shape", self.shape)
        if shape not in {"circle", "box"}:
            raise UnsupportedDS9Command("Only circle and box selection markers are supported")
        size, color = kwargs.get("size", self.size), kwargs.get("color", self.color)
        if not isinstance(size, (int, float)) or not math.isfinite(size) or not 0 < size <= 4096:
            raise ValueError("Marker size must be positive and at most 4096 pixels")
        if color not in {"white", "red", "green", "blue", "cyan", "magenta", "yellow"}:
            raise UnsupportedDS9Command("Unsupported marker color")
        markers = list(self._markers)
        for index, point in enumerate(xy):
            if len(point) != 2:
                raise ValueError("Each marker needs x and y")
            x, y = float(point[0]), float(point[1])
            if not math.isfinite(x) or not math.isfinite(y):
                raise ValueError("Marker coordinates must be finite")
            markers.append({"x": x - 1, "y": y - 1, "radius": size if shape == "circle" else size / 2,
                            "shape": shape, "color": color, "label": str(index + 1) if text else ""})
        if len(markers) > 1024:
            raise ValueError("At most 1024 selection markers")
        self._require(); self._configure(markers=markers)
        self._markers = markers

    def set(self, command):
        """Support only the documented display subset; return pyds9-style 1."""
        tokens = shlex.split(command)
        if tokens[:1] in (["fits"], ["file"]):
            if len(tokens) != 2:
                raise ValueError("Use set('fits " + '"path with spaces"' + "') or display(path)")
            self.display(tokens[1])
            return 1
        change = None
        if tokens in (["cmap", "grey"], ["cmap", "gray"]):
            change = {"cmap": "grey"}
        elif tokens[:2] == ["cmap", "invert"] and len(tokens) in {2, 3}:
            if len(tokens) == 3 and tokens[2] not in {"yes", "no"}:
                raise UnsupportedDS9Command("cmap invert accepts only yes or no")
            change = {"invert": not self._view.get("invert", False) if len(tokens) == 2 else tokens[2] == "yes"}
        elif tokens == ["scale", "zscale"]:
            change = {"limits": "zscale"}
        elif tokens == ["scale", "linear"]:
            change = {"stretch": "Linear"}
        elif tokens[:2] == ["scale", "limits"] and len(tokens) == 4:
            low, high = map(float, tokens[2:])
            if not math.isfinite(low) or not math.isfinite(high) or low >= high:
                raise ValueError("Scale limits require finite low < high")
            change = {"low": low, "high": high}
        elif tokens == ["zoom", "to", "fit"]:
            change = {"fit": True}
        elif tokens in (["regions", "delete"], ["region", "delete"], ["regions", "delete", "all"]):
            self._require(); self._configure(markers=[])
            self._markers = []
            return 1
        if change is None:
            raise UnsupportedDS9Command("FitsViewer supports a documented DS9-style subset, not this command: " + command)
        if "limits" in change:
            self._view.pop("low", None)
            self._view.pop("high", None)
        if "low" in change:
            self._view.pop("limits", None)
        self._view.update(change)
        self._configure(view=change)
        return 1

    def get(self, command):
        """Return str; iexam returns each click once as 'x X Y', then 'q 0 0'.

        q coordinates are placeholders and must not be used scientifically.
        Cancellation raises, so it cannot be mistaken for accepted empty data.
        If Undo changes a point already returned, SelectionChanged tells the
        caller to restart its scientific selection loop. Use Selection.wait()
        when editable final multi-point results are preferable.
        """
        tokens = shlex.split(command)
        if tokens == ["xpa", "info"]:
            return "FitsViewer Python selection adapter (not XPA)" if self.open_ds9 else ""
        if tokens == ["scale", "limits"]:
            selection = self._require()
            while True:
                snapshot = selection.poll()
                view = snapshot.get("view", {})
                if snapshot.get("view_revision", -1) >= snapshot.get("config_revision", 1) and all(isinstance(view.get(key), (float, int)) and math.isfinite(view[key]) for key in ("low", "high")) and view["low"] < view["high"]:
                    return f"{view['low']:.17g} {view['high']:.17g}"
                selection._result()
                self._pause(selection)
        if tokens != ["iexam", "key", "coordinate", "image"]:
            raise UnsupportedDS9Command("Only 'iexam key coordinate image', 'scale limits' and 'xpa info' are supported")
        self._image_only()
        selection = self._require()
        try:
            while True:
                snapshot = selection.poll()
                points = [tuple(point) for point in snapshot["points"]]
                if points[:len(self._emitted)] != self._emitted:
                    raise SelectionChanged("A point already sent to Python was removed or changed; restart the scientific selection loop")
                if snapshot["status"] in {"cancelled", "expired", "failed"}:
                    selection._result()
                if len(points) > len(self._emitted):
                    point = points[len(self._emitted)]
                    self._emitted.append(point)
                    return f"x {point[0] + 1:.17g} {point[1] + 1:.17g}"
                if snapshot["status"] == "completed":
                    return "q 0 0"
                self._pause(selection)
        except KeyboardInterrupt:
            selection.cancel()
            raise

    @staticmethod
    def _pause(selection):
        if time.monotonic() >= selection._deadline:
            selection.cancel()
            raise SelectionTimeout("Interactive coordinate wait timed out")
        time.sleep(.1)

    def xpaget(self, command):
        """Return bytes, matching snoopylib.xparun.xpaget and its .decode() calls."""
        return (self.get(command) + "\n").encode("utf-8")

    def xpaset(self, command):
        return self.set(command)

    def array(self, value):
        raise UnsupportedDS9Command("array() is not supported: save a FITS cutout in an authorized server root and display(path); keep its origin offset explicit")

    def cancel(self):
        return self._require().cancel()


def make_pyec_xparun(client=None, **options):
    """Return an opt-in replacement factory for snoopylib.xparun(check).

    Example::

        from fitsviewer.pyec import make_pyec_xparun
        from ecsnoopy import snoopylib
        original = snoopylib.xparun
        snoopylib.xparun = make_pyec_xparun(client)
        try:
            run_the_existing_pyec_operation()
        finally:
            snoopylib.xparun = original

    The scientific package on disk and its default DS9 backend are unchanged.
    """
    client = ViewerClient.from_runtime() if client is None else client

    def xparun(check=False):
        viewer = PyECViewer(client, **options)
        viewer.check = check
        viewer.open_ds9
        return viewer

    return xparun


__all__ = ["PyECViewer", "make_pyec_xparun", "UnsupportedDS9Command"]
