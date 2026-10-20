"""Validate checksums and explicit maintainer review of corresponding sources.

This checks the supplied inventory; it cannot infer whether a source bundle is
complete. A maintainer must review enabled dependencies and the build recipe.
"""
from pathlib import Path
import hashlib
import json
import sys

root = Path(sys.argv[1]).resolve()
manifest = json.loads((root / "verified.json").read_text(encoding="utf-8-sig"))
required = {"ffmpeg", "libraw", "libvips"}
components = manifest.get("components", [])
if manifest.get("format") != 1 or not required <= {c.get("name") for c in components}:
    raise SystemExit("Source inventory must cover ffmpeg, libraw and libvips with enabled dependencies")
for component in components:
    if not component.get("reviewed_by") or not component.get("reviewed_at") or not component.get("revision"):
        raise SystemExit("Every source group requires an explicit review and upstream revision")
    if component.get("complete_with_dependencies") is not True or not component.get("files"):
        raise SystemExit("Corresponding-source completeness has not been reviewed")
    for entry in component["files"]:
        relative = Path(entry["path"])
        if relative.is_absolute() or ".." in relative.parts:
            raise SystemExit("Unsafe source archive path")
        path = root / relative
        if path.is_symlink() or not path.resolve().is_relative_to(root) or not path.is_file():
            raise SystemExit("Missing or linked source archive")
        digest = hashlib.file_digest(path.open("rb"), "sha256").hexdigest()
        if digest != entry["sha256"]:
            raise SystemExit("Corresponding-source checksum mismatch")
print("Corresponding-source inventory checksums and review fields verified")
