"""Create source identity, a dependency inventory and retained license texts.

Uses only Python's standard library and Cargo's locked local metadata. Inventory
is intentionally a superset of linked packages, including build/platform crates.
"""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import datetime
import tomllib
from urllib.parse import quote

ROOT = Path(__file__).resolve().parent.parent


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf8")


def identity():
    names = set(git("ls-files", "-z", "--cached", "--others", "--exclude-standard").decode().split("\0")) - {""}
    files = {}
    for name in sorted(names):
        path = ROOT / name
        files[name] = hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None
    tree = hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()
    commit = git("rev-parse", "HEAD").decode().strip()
    dirty = bool(git("status", "--porcelain"))
    data = {
        "version": tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf8"))["workspace"]["package"]["version"],
        "commit": commit, "dirty": dirty, "source_sha256": tree,
        "build_id": commit[:12] + "+" + tree[:12] + ("-dirty" if dirty else ""),
        "build_time_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "source_files": files,
    }
    write(ROOT / "target/build-info.json", data)


def notices(destination):
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--offline", "--filter-platform", "x86_64-pc-windows-msvc", "--format-version=1"], cwd=ROOT))
    components, lines, missing = [], ["# Third-party licenses", "", "Generated from locked Cargo metadata and installed npm packages.",
        "The inventory includes build and other-platform dependencies; it is not a declaration that every listed package is linked.", ""], []

    def add(name, version, license_name, folder, ecosystem, explicit=None):
        slug = name.replace("@", "").replace("/", "_") + "-" + version
        target = destination / "licenses" / ecosystem / slug
        texts = set()
        for base in [folder, folder / "licenses", folder / "LICENSES"]:
            if not base.is_dir():
                continue
            for file in base.iterdir():
                if file.is_file() and file.name.lower().startswith(("license", "licence", "copying", "copyright", "notice", "unlicense")):
                    texts.add(file)
        if explicit and explicit.is_file():
            texts.add(explicit)
        supplemental = ROOT / "assets/licenses/upstream" / slug
        if supplemental.is_dir():
            texts.update(f for f in supplemental.iterdir() if f.is_file() and f.name != 'provenance.json')
        if ecosystem == 'npm' and name.startswith('onnxruntime-'):
            texts.add(ROOT / 'assets/licenses/ONNX-Runtime-MIT.txt')
        for index, file in enumerate(sorted(texts)):
            target.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(file, target / (str(index) + "-" + file.name))
        if not texts:
            missing.append({"name": name, "version": version, "ecosystem": ecosystem, "license": license_name})
        components.append({"type": "library", "name": name, "version": version,
            "purl": f"pkg:{ecosystem}/{quote(name, safe='/')}@{quote(version, safe='')}", "properties": [{"name": "declared-license", "value": str(license_name)}]})
        lines.append(f"- **{name} {version}** — {license_name}; retained texts: `licenses/{ecosystem}/{slug}/`" + (" (no upstream license text located)" if not texts else ""))

    for package in metadata["packages"]:
        if package.get("source") is None and package["name"].startswith("moriq-"):
            continue
        folder = Path(package["manifest_path"]).parent
        explicit = Path(package["license_file"]) if package.get("license_file") else None
        add(package["name"], package["version"], package.get("license") or "NOASSERTION", folder, "cargo", explicit)
    modules = ROOT / "target/ai-runtime/node_modules"
    for manifest in sorted(modules.rglob("package.json")):
        relative = manifest.relative_to(modules)
        # Only installed package roots; skip test fixtures and example package.json files.
        parts = relative.parts
        if len(parts) != (3 if parts[0].startswith("@") else 2) and "node_modules" not in parts[:-1]:
            continue
        try:
            package = json.loads(manifest.read_text(encoding="utf8"))
        except (ValueError, UnicodeError):
            continue
        if package.get("name") and package.get("version"):
            add(package["name"], package["version"], package.get("license", "NOASSERTION"), manifest.parent, "npm")
    shutil.copytree(ROOT / "assets/licenses", destination / "licenses/additional", dirs_exist_ok=True)
    (destination / "THIRD_PARTY_LICENSES.md").write_text("\n".join(lines) + "\n", encoding="utf8")
    write(destination / "sbom.cdx.json", {"bomFormat": "CycloneDX", "specVersion": "1.5", "version": 1, "components": components})
    write(destination / "license-gaps.json", missing)
    print(json.dumps({"components": len(components), "missing_texts": len(missing)}))


if __name__ == "__main__":
    if len(sys.argv) == 1 or sys.argv[1] == "identity":
        identity()
    elif sys.argv[1] == "notices":
        notices(Path(sys.argv[2]))
    else:
        raise SystemExit("Expected identity or notices DIRECTORY")
