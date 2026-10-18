"""Plan/select modules from one source tree. Compilation requires --execute.

同一源码按模块构建；默认只输出计划，不调用 Cargo/npm 或打包器。
"""
from dataclasses import dataclass
from pathlib import Path
import argparse
import hashlib
import json
import os
import platform
import shutil
import shlex
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "config/build-modules.json"


@dataclass(frozen=True)
class Selection:
    name: str
    modules: tuple

    @property
    def host_features(self):
        return [name for name in ["desktop", "pro"] if name in self.modules]

    @property
    def viewer_features(self):
        return ["pro"] if "pro" in self.modules else []

    def metadata(self):
        return {"schema": 1, "profile": self.name, "modules": list(self.modules),
                **{name: name in self.modules for name in ["pro", "desktop", "jupyter"]}}


def registry():
    value = json.loads(REGISTRY.read_text(encoding="utf-8"))
    if value.get("schema") != 1 or not isinstance(value.get("modules"), dict):
        raise ValueError("Invalid build module registry")
    return value


def select(profile=None, modules=None):
    value = registry()
    if profile is not None and modules is not None:
        raise ValueError("Choose a preset or explicit modules, not both")
    if modules is None:
        name = profile or "full"
        if name not in value["profiles"]:
            raise ValueError("Unknown build profile: " + name)
        wanted = set(value["profiles"][name])
    else:
        wanted = set(modules) | {"base"}
        name = "custom"
    unknown = wanted - value["modules"].keys()
    if unknown:
        raise ValueError("Unknown modules: " + ", ".join(sorted(unknown)))
    previous = None
    while previous != wanted:
        previous = wanted.copy()
        wanted.update(dependency for name in previous for dependency in value["modules"][name]["requires"])
    chosen = tuple(name for name in value["modules"] if name in wanted)
    return Selection("custom-" + "-".join(chosen) if name == "custom" else name, chosen)


def python_files(selection):
    value = registry()
    declared = [filename for item in value["modules"].values() for filename in item["python"]]
    actual = {path.name for path in (ROOT / "web/python").glob("*.py")}
    if len(declared) != len(set(declared)) or set(declared) != actual:
        raise ValueError("Every Python adapter must belong to exactly one build module")
    return {filename for name in selection.modules for filename in value["modules"][name]["python"]}


def cargo_arguments(package, features):
    args = ["cargo", "build", "--locked", "--release", "-p", package, "--no-default-features"]
    return args + (["--features", ",".join(features)] if features else [])


def plan(selection, portable=False, wheel=False, repair=False):
    allowed = python_files(selection)
    host = cargo_arguments("fitsviewer", selection.host_features)
    triple = None
    if portable:
        architecture = {"AMD64": "x86_64"}.get(platform.machine(), platform.machine())
        if architecture not in {"x86_64", "aarch64"}:
            raise ValueError("Portable builds support x86_64/aarch64")
        triple = architecture + "-unknown-linux-gnu"
        host[1] = "zigbuild"
        host += ["--target", triple + ".2.28"]
    wasm = cargo_arguments("fitsviewer-web", selection.viewer_features) + ["--target", "wasm32-unknown-unknown"]
    commands = [{"cwd": ".", "argv": host}, {"cwd": ".", "argv": wasm}]
    commands.append({"cwd": ".", "argv": ["wasm-bindgen", "--target", "web", "--no-typescript", "<compiled-wasm>", "--out-dir", "<profile-assets>/web/pkg"]})
    if "jupyter" in selection.modules:
        commands.extend([{"cwd": "jupyter", "argv": ["npm", "ci"]}, {"cwd": "jupyter", "argv": ["npm", "run", "build"]}])
    if wheel:
        commands.append({"cwd": ".", "argv": [sys.executable, "scripts/build_wheel.py", "--assets-dir", "<profile-assets>"] + (["--repair"] if repair else [])})
    version = next(item["version"] for item in tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8"))["package"] if item["name"] == "wasm-bindgen")
    return {**selection.metadata(), "will_execute": False, "host_features": selection.host_features,
            "viewer_features": selection.viewer_features, "portable_target": triple,
            "python_adapters": sorted(allowed), "commands": commands,
            "source_rewriting": False, "source_tree_copy": False,
            "build_tool_requirements": {"wasm-bindgen-cli": version, "install_if_missing_or_mismatched": ["cargo", "install", "wasm-bindgen-cli", "--version", version, "--locked"]}}


def run(command, cwd=ROOT, env=None):
    subprocess.run(command, cwd=cwd, env=env, check=True)


def stage(selection, target_directory, host_target=None):
    """Copy selected resources into an owned output, without changing source files."""
    parent = (ROOT / "build/profiles").resolve()
    if not parent.is_relative_to(ROOT.resolve()):
        raise ValueError("Profile output escaped the project")
    parent.mkdir(parents=True, exist_ok=True)
    output = parent / selection.name
    resolved = output.resolve()
    if resolved.parent != parent or output.is_symlink():
        raise ValueError("Unsafe profile output directory")
    if output.exists():
        old = output / "build-profile.json"
        if not old.is_file() or json.loads(old.read_text())["profile"] != selection.name:
            raise ValueError("Refusing to replace unrecognized build output")
        shutil.rmtree(output)
    (output / "bin").mkdir(parents=True)
    executable = "fitsviewer.exe" if os.name == "nt" else "fitsviewer"
    binary = target_directory / (host_target or "") / "release" / executable
    actual = json.loads(subprocess.check_output([str(binary), "--build-info"], text=True))
    if actual["features"] != {"base": True, "pro": "pro" in selection.modules, "desktop": "desktop" in selection.modules}:
        raise ValueError("Compiled host features do not match the selected profile")
    shutil.copy2(binary, output / "bin" / executable)
    if shutil.which("strip"):
        run(["strip", "--strip-unneeded", str(output / "bin" / executable)])
    included = python_files(selection)

    def ignore(directory, names):
        omitted = {name for name in names if name in {"pkg", "__pycache__", "node_modules"} or name.endswith((".pyc", ".pyo"))}
        if Path(directory).resolve() == (ROOT / "web/python").resolve():
            omitted.update(name for name in names if name.endswith(".py") and name not in included)
        return omitted

    shutil.copytree(ROOT / "web", output / "web", ignore=ignore)
    pkg = output / "web/pkg"
    run(["wasm-bindgen", "--target", "web", "--out-dir", str(pkg), "--no-typescript", str(target_directory / "wasm32-unknown-unknown/release/fitsviewer_web.wasm")])
    metadata = selection.metadata()
    for path in [output / "build-profile.json", output / "web/build-profile.json"]:
        path.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    files = {path.relative_to(output).as_posix(): {"bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
             for path in output.rglob("*") if path.is_file()}
    (output / "artifact-manifest.json").write_text(json.dumps({"schema": 1, "features": metadata, "files": files}, indent=2), encoding="utf-8")
    return output


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    options = parser.add_mutually_exclusive_group()
    options.add_argument("--profile", help="mini, desktop-mini, remote-pro, or full")
    options.add_argument("--modules", help="Comma-separated modules; base is included automatically")
    parser.add_argument("--plan", action="store_true", help="Print the plan only (the default)")
    parser.add_argument("--execute", action="store_true", help="Explicitly compile and stage the selected modules")
    parser.add_argument("--wheel", action="store_true", help="Also build a wheel when --execute is present")
    parser.add_argument("--portable", action="store_true", help="Use the existing GNU glibc 2.28 cross-build toolchain")
    parser.add_argument("--repair", action="store_true", help="Request auditwheel repair of an explicitly built wheel")
    args = parser.parse_args(argv)
    if args.modules is not None and any(not name.strip() for name in args.modules.split(",")):
        parser.error("--modules needs nonempty module names")
    if args.plan and args.execute:
        parser.error("--plan cannot be combined with --execute")
    try:
        selection = select(args.profile, [name.strip() for name in args.modules.split(",")] if args.modules is not None else None)
        description = plan(selection, args.portable, args.wheel, args.repair)
    except ValueError as error:
        parser.error(str(error))
    description["will_execute"] = args.execute
    print(json.dumps(description, indent=2))
    if not args.execute:
        return 0
    if platform.system() != "Linux":
        raise RuntimeError("This build driver executes on Linux; other platforms can inspect the same module plan")
    environment = os.environ.copy()
    cargo_home = Path(environment.get("CARGO_HOME", ROOT / ".tools/cargo")).resolve()
    flags = environment.get("CARGO_ENCODED_RUSTFLAGS", "").split("\x1f") if environment.get("CARGO_ENCODED_RUSTFLAGS") else shlex.split(environment.get("RUSTFLAGS", ""))
    flags += [f"--remap-path-prefix={ROOT}=fitsviewer-source", f"--remap-path-prefix={cargo_home}=cargo-source"]
    environment["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
    environment.pop("RUSTFLAGS", None)
    environment["CARGO_HOME"] = str(cargo_home)
    environment["PATH"] = str(cargo_home / "bin") + os.pathsep + environment.get("PATH", "")
    bindgen = shutil.which("wasm-bindgen", path=environment["PATH"])
    version = description["build_tool_requirements"]["wasm-bindgen-cli"]
    actual_version = subprocess.check_output([bindgen, "--version"], text=True).strip() if bindgen else ""
    if actual_version != "wasm-bindgen " + version:
        run(description["build_tool_requirements"]["install_if_missing_or_mismatched"], env=environment)
    for command in description["commands"][:2]:
        run(command["argv"], env=environment)
    if "jupyter" in selection.modules:
        run(["npm", "ci"], ROOT / "jupyter", environment)
        run(["npm", "run", "build"], ROOT / "jupyter", environment)
    target = Path(environment.get("CARGO_TARGET_DIR", ROOT / "target"))
    target = (target if target.is_absolute() else ROOT / target).resolve()
    output = stage(selection, target, description["portable_target"])
    if args.wheel:
        if args.portable:
            environment["FITSVIEWER_WHEEL_PLATFORM"] = "manylinux_2_28_" + platform.machine()
        run([sys.executable, str(ROOT / "scripts/build_wheel.py"), "--assets-dir", str(output)] + (["--repair"] if args.repair else []), env=environment)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
