"""Archive standalone runtime notices outside the portable distribution."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sys

ROOT = Path(__file__).resolve().parent.parent
NOTICE_NAME = re.compile(
    r"(?:^|[-_.])(?:licen[cs]es?|copying|copyright|notices?|unlicense)(?:$|[-_.])"
    r"|^third[-_]?party[-_]?(?:notices?|licen[cs]es?)(?:$|[.])"
    r"|^licensecheck\.json$", re.IGNORECASE,
)


# 根据路径各段识别独立许可说明文件，排除具有运行用途的代码和二进制文件。
def notice_path(relative):
    for part in Path(relative).parts:
        if part.lower() == "licensecheck.json":
            return True
        if Path(part).suffix.lower() in {".js", ".mjs", ".cjs", ".ts", ".json", ".map", ".node", ".dll", ".exe"}:
            continue
        if NOTICE_NAME.search(part):
            return True
    return False


# 遍历生成的运行目录，拒绝链接和越界路径，并排除重复的解压目录。
def runtime_paths(runtime):
    for directory, folders, files in os.walk(runtime, followlinks=False):
        base = Path(directory)
        if base == runtime:
            folders[:] = [name for name in folders if name != "node-unpacked"]
        for name in folders + files:
            path = base / name
            if path.is_symlink() or path.is_junction():
                raise RuntimeError(f"Unexpected linked runtime entry: {path}")
            if not path.resolve().is_relative_to(runtime):
                raise RuntimeError(f"Runtime entry escapes generated directory: {path}")
        yield base, folders, files


# 先复制并校验许可说明到包外档案，再删除运行目录中的对应副本并记录清单。
def prune(label):
    runtime = ROOT / {"ai": "target/ai-runtime", "codecs": "target/codecs/runtime"}[label]
    archive = ROOT / "target/package-reports/runtime-notices" / label
    for folder in (runtime, archive):
        if folder.resolve() != folder or not folder.resolve().is_relative_to(ROOT / "target"):
            raise RuntimeError(f"Invalid generated runtime or archive path: {folder}")
    entries = []
    directories = []
    for base, folders, files in runtime_paths(runtime):
        directories.extend(base / name for name in folders)
        for name in files:
            source = base / name
            relative = source.relative_to(runtime)
            if not notice_path(relative):
                continue
            destination = archive / relative
            if destination.resolve() != destination:
                raise RuntimeError(f"Linked notice archive path: {destination}")
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
            checksum = hashlib.sha256(source.read_bytes()).hexdigest()
            if hashlib.sha256(destination.read_bytes()).hexdigest() != checksum:
                raise RuntimeError(f"Notice archive verification failed: {relative}")
            source.unlink()
            entries.append({"path": relative.as_posix(), "sha256": checksum})
    for directory in sorted(directories, key=lambda path: len(path.parts), reverse=True):
        if notice_path(directory.relative_to(runtime)) and not any(directory.iterdir()):
            directory.rmdir()
    archive.mkdir(parents=True, exist_ok=True)
    (archive / "archived-files.json").write_text(json.dumps(entries, indent=2) + "\n", encoding="utf8")
    print(json.dumps({"runtime": label, "notices_archived_outside_package": len(entries)}))


# 检查最终包目录是否还留有应已归档的独立许可说明。
def check(runtime):
    if not runtime.is_dir():
        raise RuntimeError(f"Missing package directory: {runtime}")
    remaining = [str((base / name).relative_to(runtime))
                 for base, folders, files in runtime_paths(runtime)
                 for name in folders + files if notice_path((base / name).relative_to(runtime))]
    if remaining:
        raise RuntimeError(f"Standalone notices remain in package: {remaining}")


if __name__ == "__main__":
    if len(sys.argv) == 2 and sys.argv[1] in ("ai", "codecs"):
        prune(sys.argv[1])
    elif len(sys.argv) == 3 and sys.argv[1] == "check":
        check(Path(sys.argv[2]).resolve())
    else:
        raise SystemExit("Expected ai, codecs, or check PACKAGE_DIRECTORY")
