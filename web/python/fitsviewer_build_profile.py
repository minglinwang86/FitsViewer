"""Build selection only; never imports scientific packages. / 仅描述构建模块。"""
from functools import lru_cache
import json
from pathlib import Path

BASIC_OPERATIONS = frozenset({
    "capabilities", "wcs", "gaia_solve", "job_start", "job_status", "job_cancel",
    "release_owner", "release_pro_owner", "release_gaia_owner", "release_pro", "artifact_path",
})


@lru_cache(maxsize=1)
def profile():
    """Read the staged profile; an unstaged development checkout contains all modules."""
    path = Path(__file__).resolve().parent.parent / "build-profile.json"
    if not path.exists():
        return {"schema": 1, "profile": "development", "modules": ["base", "pro", "desktop", "jupyter"], "pro": True, "desktop": True, "jupyter": True}
    value = json.loads(path.read_text(encoding="utf-8"))
    if value.get("schema") != 1 or not isinstance(value.get("modules"), list):
        raise ValueError("Invalid FitsViewer build profile")
    if set(value["modules"]) - {"base", "pro", "desktop", "jupyter"} or "base" not in value["modules"]:
        raise ValueError("Unknown FitsViewer build module")
    for key in ["pro", "desktop", "jupyter"]:
        if not isinstance(value.get(key), bool) or value[key] != (key in value["modules"]):
            raise ValueError("Inconsistent FitsViewer build profile: " + key)
    return value


def require_operation(operation, params):
    """Keep absent PRO adapters unreachable, including through a background job."""
    if profile()["pro"]:
        return
    if operation not in BASIC_OPERATIONS:
        raise ValueError("PRO is not included in this build")
    if operation == "job_start" and params.get("operation") != "gaia_solve":
        raise ValueError("Only Gaia background jobs are included in this build")


def available_operations(operations):
    return list(operations) if profile()["pro"] else [name for name in operations if name in BASIC_OPERATIONS]
