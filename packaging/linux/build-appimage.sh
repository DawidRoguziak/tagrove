#!/usr/bin/env bash
set -euo pipefail
mkdir -p .packaging
python3 - <<'PYTHON'
import importlib.util, json
from pathlib import Path
root = Path.cwd()
spec = importlib.util.spec_from_file_location("generate", root / "packaging/flatpak/generate.py")
generate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generate)
publisher = json.loads((root / "packaging/flatpak/publisher.json").read_text())
publisher = generate.build_publisher(root, publisher)
metadata = generate.metadata(root, publisher).replace(publisher["appId"] + ".desktop", "Tagrove.desktop")
(root / ".packaging/Tagrove.appdata.xml").write_text(metadata)
desktop = (root / "src-tauri/linux/com.example.mediatagger.desktop").read_text().replace("Icon=tagrove", "Icon=media_tagger")
(root / ".packaging/tagrove.desktop").write_text(desktop)
PYTHON
python3 packaging/linux/notices.py .packaging/dependencies
export LDAI_RUNTIME_FILE=/root/.cache/tauri/runtime-x86_64
bun run tauri build -v --config src-tauri/tauri.appimage.conf.json -- --locked --offline
bash packaging/linux/export-appimage.sh
