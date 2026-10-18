"""Collect original legal texts for the actual native Full dependency graph.

仅补充 macOS 包内许可证；不改共享 inventory、源码或 Python wheel 元数据。
"""
from pathlib import Path
import hashlib
import json
import re
import shutil
import subprocess
import urllib.error
import urllib.parse
import urllib.request

TARGETS = {"arm64": "aarch64-apple-darwin", "x86_64": "x86_64-apple-darwin"}
LEGAL_NAMES = ["LICENSE", "LICENSE.txt", "LICENSE.md", "LICENSE-MIT", "LICENSE-APACHE",
               "LICENSE.MIT", "LICENSE.APACHE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE"]


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def native_packages(metadata):
    """Walk normal/build edges from the Full host; skip dev-only and workspace crates."""
    packages = {entry["id"]: entry for entry in metadata["packages"]}
    nodes = {entry["id"]: entry for entry in metadata["resolve"]["nodes"]}
    hosts = [entry["id"] for entry in metadata["packages"]
             if entry["name"] == "fitsviewer" and entry["id"] in metadata["workspace_members"]]
    if len(hosts) != 1:
        raise ValueError("Cannot identify the native Full host dependency graph")
    pending, visited = hosts[:], set()
    while pending:
        identifier = pending.pop()
        if identifier in visited:
            continue
        visited.add(identifier)
        for dependency in nodes[identifier]["deps"]:
            if any(kind["kind"] in {None, "build"} for kind in dependency["dep_kinds"]):
                pending.append(dependency["pkg"])
    workspace = set(metadata["workspace_members"])
    return sorted((packages[key] for key in visited - workspace), key=lambda item: (item["name"], item["version"]))


def legal_files(source):
    files = []
    for path in source.rglob("*"):
        name = path.name.lower()
        legal = any(word in name for word in ["license", "licence", "copying", "copyright", "notice"])
        legal = legal or name.startswith(("ofl", "ufl")) or ("fonts" in path.parts and path.suffix == ".txt")
        if legal and path.is_file() and not path.is_symlink() and 0 < path.stat().st_size < 1_000_000:
            files.append(path)
    return sorted(files)


def upstream_texts(package, source):
    """Only exact crate-recorded upstream commits may supplement omitted root licenses."""
    vcs = source / ".cargo_vcs_info.json"
    repository = (package.get("repository") or "").removesuffix(".git").rstrip("/")
    if not vcs.is_file() or not repository.startswith("https://github.com/"):
        return {}
    commit = json.loads(vcs.read_text())["git"]["sha1"]
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Invalid crate upstream commit")
    owner_repo = "/".join(urllib.parse.urlsplit(repository).path.strip("/").split("/")[:2])
    result = {}
    for name in LEGAL_NAMES:
        url = "https://raw.githubusercontent.com/" + owner_repo + "/" + commit + "/" + name
        try:
            with urllib.request.urlopen(url, timeout=10) as response:
                content = response.read(1_000_001)
            if 0 < len(content) < 1_000_000:
                result["upstream-" + name] = (content, url)
        except (urllib.error.URLError, TimeoutError):
            continue
    return result


def collect_from_metadata(metadata, root, resources, target, *, fetch_missing=True):
    packages = native_packages(metadata)
    records = []
    for package in packages:
        source = Path(package["manifest_path"]).resolve().parent
        key = package["name"] + "-" + package["version"]
        if not re.fullmatch(r"[A-Za-z0-9_.+-]+", key):
            raise ValueError("Invalid crate legal output name")
        folder = resources / "licenses/rust" / key
        folder.mkdir(parents=True, exist_ok=True)
        files, primary = [], []
        candidates = legal_files(source)
        declared = source / package["license_file"] if package.get("license_file") else None
        if declared and declared.resolve().is_relative_to(source) and declared.is_file() and declared not in candidates:
            candidates.append(declared)
        for path in candidates:
            relative = path.relative_to(source)
            output = folder / relative
            output.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, output)
            record = {"path": output.relative_to(resources).as_posix(), "bytes": output.stat().st_size,
                      "sha256": sha256(output), "origin": "crate-source:" + relative.as_posix()}
            files.append(record)
            if path.parent == source or path == declared or relative.parts[0].lower() in {"licenses", "licences"}:
                primary.append(record["path"])
        if not primary and fetch_missing:
            for name, (content, url) in upstream_texts(package, source).items():
                output = folder / name
                output.write_bytes(content)
                files.append({"path": output.relative_to(resources).as_posix(), "bytes": len(content),
                              "sha256": sha256(output), "origin": url})
                primary.append(output.relative_to(resources).as_posix())
        if not primary and package.get("license") == "CC0-1.0":
            canonical = root / "licenses/common/CC0-1.0.txt"
            if canonical.is_file():
                output = folder / "LICENSE-CC0-1.0.txt"
                shutil.copy2(canonical, output)
                files.append({"path": output.relative_to(resources).as_posix(), "bytes": output.stat().st_size,
                              "sha256": sha256(output), "origin": "declared CC0-1.0; licenses/common/CC0-1.0.txt"})
                primary.append(output.relative_to(resources).as_posix())
        if not primary or not (package.get("license") or package.get("license_file")):
            raise ValueError("Missing original native dependency license: " + key)
        records.append({"name": package["name"], "version": package["version"], "license": package.get("license"),
                        "license_file": package.get("license_file"), "authors": package.get("authors", []),
                        "repository": package.get("repository"), "source": package.get("source") or "vendored path dependency",
                        "primary_files": primary, "files": files})
    if not records:
        raise ValueError("Native dependency license inventory is empty")
    inventory = {"schema": 1, "target": target, "features": ["desktop", "pro"],
                 "scope": "Normal/build dependencies reachable from the native Full host; excludes dev-only dependencies",
                 "cargo_lock_sha256": sha256(root / "Cargo.lock"), "packages": records}
    (resources / "licenses/macos-native-inventory.json").write_text(json.dumps(inventory, indent=2) + "\n")
    notices = ["# macOS native dependency notices", "",
               "Supplement to THIRD_PARTY_NOTICES.md for the native Full host target `" + target + "`.",
               "Original dependency license texts, authors and upstream sources are preserved below.", ""]
    for package in records:
        notices += ["## " + package["name"] + " " + package["version"], "",
                    "License: `" + str(package["license"] or package["license_file"]) + "`.", ""]
        if package["authors"]:
            notices += ["Upstream authors: " + "; ".join(package["authors"]), ""]
        if package["repository"]:
            notices += ["Source: " + package["repository"], ""]
        notices += ["- [" + item["path"] + "](" + item["path"] + ")" for item in package["files"]]
        notices += [""]
    (resources / "MACOS_THIRD_PARTY_NOTICES.md").write_text("\n".join(notices), encoding="utf-8")
    with (resources / "THIRD_PARTY_NOTICES.md").open("a", encoding="utf-8") as stream:
        stream.write("\n\n## macOS native dependency supplement\n\nSee [original macOS native dependency notices](MACOS_THIRD_PARTY_NOTICES.md) and [the target-specific inventory](licenses/macos-native-inventory.json).\n")
    return inventory


def collect_native_licenses(root, resources, architecture, environment):
    target = TARGETS[architecture]
    arguments = ["cargo", "metadata", "--locked", "--offline", "--format-version", "1",
                 "--filter-platform", target, "--no-default-features", "--features", "fitsviewer/desktop,fitsviewer/pro"]
    metadata = json.loads(subprocess.check_output(arguments, cwd=root, env=environment, text=True))
    result = collect_from_metadata(metadata, root, resources, target)
    if not any(package["name"].startswith("objc2") for package in result["packages"]):
        raise ValueError("The native macOS dependency graph is missing Objective-C bindings")
    return result


def verify_inventory(resources, target, lock_sha256):
    inventory = json.loads((resources / "licenses/macos-native-inventory.json").read_text())
    if (inventory.get("schema") != 1 or inventory.get("target") != target
            or inventory.get("features") != ["desktop", "pro"] or inventory.get("cargo_lock_sha256") != lock_sha256):
        raise ValueError("Native legal inventory differs from this build")
    identities = set()
    for package in inventory["packages"]:
        identity = (package["name"], package["version"])
        if identity in identities or not package["primary_files"]:
            raise ValueError("Duplicate or incomplete native dependency legal record")
        identities.add(identity)
        paths = set()
        for item in package["files"]:
            path = resources / item["path"]
            if not path.resolve().is_relative_to(resources.resolve()) or path.is_symlink():
                raise ValueError("Native legal file escapes the app resources")
            if not path.is_file() or path.stat().st_size != item["bytes"] or sha256(path) != item["sha256"]:
                raise ValueError("Native legal file is absent or changed: " + item["path"])
            paths.add(item["path"])
        if not set(package["primary_files"]).issubset(paths):
            raise ValueError("Native primary license text is not in the audit")
    if not identities:
        raise ValueError("Native legal inventory is empty")
    return len(identities)
