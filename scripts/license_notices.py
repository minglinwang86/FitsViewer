"""Collect locked dependency/font license texts. / 收集依赖及字体许可。"""
from pathlib import Path
import argparse
import json
import shutil
import subprocess
import urllib.error
import urllib.parse
import urllib.request
import re
from packaging.licenses import canonicalize_license_expression

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--offline", action="store_true")
args = parser.parse_args()
packages = {}
for target in ["x86_64-unknown-linux-gnu", "wasm32-unknown-unknown"]:
    command = ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", target]
    if args.offline: command.append("--offline")
    metadata = json.loads(subprocess.check_output(command, cwd=root, text=True))
    active = {node["id"] for node in metadata["resolve"]["nodes"]}
    packages.update({p["id"]: p for p in metadata["packages"] if p["source"] and p["id"] in active})
inventory, missing, cache = [], [], {}
names = ["LICENSE", "LICENSE.txt", "LICENSE.md", "LICENSE-MIT", "LICENSE-APACHE", "LICENSE.MIT", "LICENSE.APACHE", "COPYING", "COPYING.CC0", "COPYRIGHT", "NOTICE"]
for package in sorted(packages.values(), key=lambda p: (p["name"], p["version"])):
    source = Path(package["manifest_path"]).parent
    folder = root / "licenses/rust" / (package["name"] + "-" + package["version"])
    folder.mkdir(parents=True, exist_ok=True)
    files = []
    for path in sorted(source.rglob("*")):
        name = path.name.lower()
        legal = any(w in name for w in ["license", "licence", "copying", "copyright", "notice"])
        legal = legal or name.startswith(("ofl", "ufl")) or ("fonts" in path.parts and path.suffix == ".txt")
        if path.is_file() and legal and path.stat().st_size < 1_000_000:
            target = folder / path.relative_to(source)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, target)
            files.append(target.relative_to(root).as_posix())
    if package.get("license_file"):
        path = source / package["license_file"]
        if path.is_file():
            target = folder / path.name
            shutil.copy2(path, target)
            files.append(target.relative_to(root).as_posix())
    for existing in folder.rglob("*"):
        if existing.is_file(): files.append(existing.relative_to(root).as_posix())
    # Workspace crate archives may omit root licenses. Fetch from the exact commit.
    # 工作区子 crate 可能未带根许可；只从其记录的精确提交补充。
    top_level = any(Path(f).parent == folder.relative_to(root) for f in files)
    repository = (package.get("repository") or "").removesuffix(".git").rstrip("/")
    vcs = source / ".cargo_vcs_info.json"
    if (not top_level or not files) and not args.offline and repository.startswith("https://github.com/") and vcs.exists():
        commit = json.loads(vcs.read_text()).get("git", {}).get("sha1")
        owner_repo = "/".join(urllib.parse.urlsplit(repository).path.strip("/").split("/")[:2])
        key = (owner_repo, commit)
        if commit and key not in cache:
            found = {}
            for name in names:
                url = "https://raw.githubusercontent.com/" + owner_repo + "/" + commit + "/" + name
                try:
                    with urllib.request.urlopen(url, timeout=10) as response: found[name] = response.read(1_000_000)
                except (urllib.error.URLError, TimeoutError): continue
            cache[key] = found
        for name, content in cache.get(key, {}).items():
            target = folder / ("upstream-" + name)
            target.write_bytes(content)
            files.append(target.relative_to(root).as_posix())
    for existing in folder.rglob("*"):
        if existing.is_file(): files.append(existing.relative_to(root).as_posix())
    if not files and package.get("license") == "CC0-1.0":
        canonical = root / "licenses/common/CC0-1.0.txt"
        if not canonical.exists() and not args.offline:
            canonical.parent.mkdir(parents=True, exist_ok=True)
            with urllib.request.urlopen("https://raw.githubusercontent.com/spdx/license-list-data/main/text/CC0-1.0.txt", timeout=15) as response:
                text = response.read()
            assert b"CC0" in text and b"Waiver" in text, "Unexpected CC0 legal text"
            canonical.write_bytes(text)
        if canonical.exists(): files.append(canonical.relative_to(root).as_posix())
    if not files: missing.append(package["name"] + " " + package["version"])
    inventory.append({"name": package["name"], "version": package["version"], "license": package.get("license"),
                      "authors": package.get("authors", []), "repository": repository, "files": sorted(set(files))})
(root / "licenses/inventory.json").write_text(json.dumps(inventory, indent=2) + "\n")
lines = ["# Third-party notices / 第三方许可", "",
         "Project source is MIT. This inventory covers locked Cargo dependencies reachable in native/WASM build graphs, including build dependencies for completeness. It does not assert every crate is linked into every binary.", "",
         "Dependencies and bundled fonts retain their original terms. Full texts are under `licenses/`. Jupyter/Lumino frontend packages are shared by the host; generated frontend notices accompany the prebuilt extension.", ""]
for p in inventory:
    lines.extend(["## " + p["name"] + " " + p["version"], "", "License: `" + str(p["license"]) + "`.", ""])
    if p["authors"]: lines.extend(["Upstream authors: " + "; ".join(p["authors"]), ""])
    if p["repository"]: lines.extend(["Source: " + p["repository"], ""])
    lines.extend(["- [" + f + "](" + f + ")" for f in p["files"]]); lines.append("")
(root / "THIRD_PARTY_NOTICES.md").write_text("\n".join(lines))
expressions = {"MIT"}
for p in inventory:
    expression = (p["license"] or "").replace("Apache-2.0/MIT", "Apache-2.0 OR MIT").replace("MIT/Apache-2.0", "MIT OR Apache-2.0")
    expressions.add(canonicalize_license_expression(expression))
combined = " AND ".join("(" + expression + ")" for expression in sorted(expressions))
project = root / "jupyter/pyproject.toml"
project.write_text(re.sub(r'^license = ".*"$', "license = " + json.dumps(combined), project.read_text(), flags=re.MULTILINE))
print(json.dumps({"packages": len(inventory), "missing_license_files": missing}))
if missing: raise SystemExit("Resolve missing license texts before publishing")
