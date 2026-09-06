"""Retain dependency manifests and license texts alongside each package."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys

destination = Path(sys.argv[1])
destination.mkdir(parents=True, exist_ok=True)
for name in ("package.json", "bun.lock", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"):
    shutil.copyfile(name, destination / Path(name).name)
roots = [Path("node_modules"), Path("cargo-vendor"), Path("/app/tools/share/doc/rust"),
         Path("/usr/local/share/doc/rust"),
         Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))) / "registry/src"]
records = []
for root in roots:
    if not root.exists():
        continue
    for folder, dirs, files in os.walk(root):
        dirs[:] = [name for name in dirs if name not in (".git", "target")]
        for name in files:
            if (name.lower().startswith(("license", "licence", "copying", "copyright", "notice"))
                    or name in ("Cargo.toml", "package.json")):
                source = Path(folder) / name
                digest = hashlib.sha256(source.read_bytes()).hexdigest()
                target = destination / "texts" / digest
                target.parent.mkdir(exist_ok=True)
                shutil.copyfile(source, target)
                records.append({"path": str(source), "sha256": digest, "text": "texts/" + digest})
(destination / "index.json").write_text(json.dumps(records, indent=2) + "\n")
