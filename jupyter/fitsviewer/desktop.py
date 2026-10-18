"""Per-user Linux desktop association. / 当前用户的原生 FITS 文件关联。"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

IDENTIFIER = "fitsviewer.desktop"
MIMES = ["image/fits", "application/fits", "application/x-fits-gzip"]

def _windows(action):
    from .runtime import resources
    _, binary, _ = resources()
    result=subprocess.run([str(binary),'--desktop-default',action],check=True,capture_output=True,
                          text=True,timeout=15,creationflags=subprocess.CREATE_NO_WINDOW)
    return json.loads(result.stdout)


def _paths():
    data = Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share")))
    return {"desktop": data / "applications" / IDENTIFIER,
            "state": data / "applications/fitsviewer-desktop.json",
            "mime": data / "mime/packages/fitsviewer.xml",
            "icon": data / "icons/hicolor/scalable/apps/fitsviewer.svg"}


def _default(mime):
    if not shutil.which("xdg-mime"): return None
    try:
        reply = subprocess.run(["xdg-mime", "query", "default", mime], capture_output=True, text=True, timeout=5)
    except subprocess.TimeoutExpired:
        return None
    return reply.stdout.strip() or None


def details():
    if os.name=='nt': return _windows('status')
    if sys.platform=='darwin':
        from . import desktop_macos
        return desktop_macos.details()
    paths = _paths()
    return {"registered": paths["desktop"].is_file(), "desktop_file": str(paths["desktop"]),
            "defaults": {mime: _default(mime) for mime in MIMES},
            "tools_available": all(shutil.which(n) for n in ["xdg-mime", "update-mime-database"])}


def _refresh(paths):
    if shutil.which("update-mime-database"):
        subprocess.run(["update-mime-database", str(paths["mime"].parents[1])], check=True, capture_output=True)
    if shutil.which("update-desktop-database"):
        subprocess.run(["update-desktop-database", str(paths["desktop"].parent)], check=True, capture_output=True)


def disable_default():
    """Restore recorded defaults; keep an installed Open With entry.

    恢复安装前默认值，保留已注册的“打开方式”入口。
    """
    if os.name=='nt': return _windows('no')
    if sys.platform=='darwin':
        from . import desktop_macos
        return desktop_macos.disable_default()
    paths = _paths()
    previous = json.loads(paths["state"].read_text()).get("previous", {}) if paths["state"].exists() else {}
    for mime, value in previous.items():
        if value and value != IDENTIFIER and _default(mime) == IDENTIFIER:
            subprocess.run(["xdg-mime", "default", value, mime], check=True)
    config = Path(os.environ.get("XDG_CONFIG_HOME", str(Path.home() / ".config")))
    for path in [config / "mimeapps.list", paths["desktop"].parent / "mimeapps.list"]:
        if not path.is_file(): continue
        lines = []
        changed = False
        for line in path.read_text().splitlines(keepends=True):
            key, separator, value = line.partition("=")
            if separator and key.strip() in MIMES and IDENTIFIER in value.split(";"):
                choices = [item for item in value.strip().split(";") if item and item != IDENTIFIER]
                line = key + "=" + ";".join(choices) + ";\n" if choices else ""
                changed = True
            lines.append(line)
        if changed: path.write_text("".join(lines))
    return details()


def install(default=True):
    from .build_info import require_module
    require_module("desktop")
    if os.name=='nt': return _windows('yes' if default else 'register')
    if sys.platform=='darwin':
        from . import desktop_macos
        return desktop_macos.install(default)
    from .runtime import resources
    _, _, web = resources()
    icon = (web / "icons/fitsviewer.svg").read_bytes()
    paths = _paths()
    missing = [name for name in ["xdg-mime", "update-mime-database"] if not shutil.which(name)]
    if missing: raise ValueError("Desktop integration needs xdg-utils and shared-mime-info (missing " + ", ".join(missing) + ")")
    cli = Path(sys.executable).parent / "fitsviewer"
    if not cli.is_file():
        source = Path(__file__).resolve().parents[2] / "bin/fitsviewer"
        if not source.is_file(): raise FileNotFoundError("Install the complete wheel before desktop registration")
        cli = source
    if paths["desktop"].exists() and "X-FitsViewer-Managed=true" not in paths["desktop"].read_text():
        raise ValueError("An unmanaged fitsviewer.desktop already exists")
    for path in paths.values(): path.parent.mkdir(parents=True, exist_ok=True)
    if not paths["state"].exists():
        paths["state"].write_text(json.dumps({"previous": {mime: _default(mime) for mime in MIMES}}, indent=2))
    state = json.loads(paths["state"].read_text())
    state["prefix"] = sys.prefix
    paths["state"].write_text(json.dumps(state, indent=2))
    # Desktop Entry quoting differs from shell quoting; %% is a literal percent.
    # desktop Exec 使用自己的引号规则，不能用 shell 的单引号转义。
    executable = str(cli.absolute()).replace("%", "%%")
    for character in ["\\", '"', "`", "$"]: executable = executable.replace(character, "\\" + character)
    executable = executable.replace("\\", "\\\\")
    paths["desktop"].write_text("[Desktop Entry]\nType=Application\nName=FitsViewer\nName[zh_CN]=FitsViewer 图像查看器\n"
        "Comment=View FITS images with the native Rust interface\nTerminal=false\nIcon=fitsviewer\nStartupWMClass=fitsviewer\n"
        'Exec="' + executable + '" --native %f\nCategories=Science;Astronomy;\nMimeType=' + ";".join(MIMES) + ";\nX-FitsViewer-Managed=true\n")
    globs = "".join('<glob pattern="' + pattern + '"/>' for pattern in ["*.fits", "*.fit", "*.fts", "*.fz", "*.fits.fz", "*.fit.fz", "*.fts.fz"])
    gzip = "".join('<glob pattern="' + pattern + '"/>' for pattern in ["*.fits.gz", "*.fit.gz", "*.fts.gz"])
    paths["mime"].write_text('<?xml version="1.0" encoding="UTF-8"?>\n'
        '<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">'
        '<mime-type type="image/fits"><comment>FITS image</comment>' + globs + '</mime-type>'
        '<mime-type type="application/x-fits-gzip"><comment>Gzip-compressed FITS image</comment>' + gzip + '</mime-type></mime-info>\n')
    paths["icon"].write_bytes(icon)
    _refresh(paths)
    if default:
        for mime in MIMES: subprocess.run(["xdg-mime", "default", IDENTIFIER, mime], check=True)
    else: disable_default()
    return details()


def remove():
    if os.name=='nt': return _windows('remove')
    if sys.platform=='darwin':
        from . import desktop_macos
        return desktop_macos.remove()
    paths = _paths()
    if paths["desktop"].exists() and "X-FitsViewer-Managed=true" not in paths["desktop"].read_text():
        raise ValueError("Refusing to remove an unmanaged desktop entry")
    disable_default()
    for path in paths.values(): path.unlink(missing_ok=True)
    _refresh(paths)
    return details()


def command(arguments):
    parser = argparse.ArgumentParser(prog="fitsviewer desktop")
    sub = parser.add_subparsers(dest="action", required=True)
    sub.add_parser("status")
    sub.add_parser("remove")
    installer = sub.add_parser("install")
    installer.add_argument("--no-default", action="store_true")
    args = parser.parse_args(arguments)
    data = install(not args.no_default) if args.action == "install" else remove() if args.action == "remove" else details()
    print(json.dumps(data, indent=2))
