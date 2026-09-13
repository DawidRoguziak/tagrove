"""Verify that the public release supplies the manifest's authoritative inputs."""
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import tarfile
import tempfile

REQUIRED_INPUTS = ["package.json", "bun.lock", "bunfig.toml", "src-tauri/tauri.conf.json",
                   "src-tauri/linux/com.example.mediatagger.desktop", "packaging/flatpak/generate.py", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock",
                   "src-tauri/tauri.flatpak.conf.json", "packaging/flatpak/install.sh",
                   "packaging/linux/toolchains.json", "packaging/flatpak/media.json",
                   "packaging/linux/notices.py", "packaging/linux/check-runtime.sh", "LICENSE", "PRIVACY.md",
                   "src-tauri/capabilities/default.json"]
PUBLISHER_INPUT = "packaging/flatpak/publisher.json"


def verify_archive(root, archive_path):
    with tarfile.open(archive_path, "r:*") as archive:
        members = archive.getmembers()
        top_levels = {member.name.split("/")[0] for member in members}
        if len(top_levels) != 1:
            raise ValueError("Release archive must have one top-level source directory")
        prefix = top_levels.pop()
        if len({member.name for member in members}) != len(members):
            raise ValueError("Release archive contains duplicate paths")
        for name in REQUIRED_INPUTS:
            member = archive.getmember(prefix + "/" + name)
            if not member.isfile():
                raise ValueError(f"Release input is not a regular file: {name}")
            actual = hashlib.sha256(archive.extractfile(member).read()).digest()
            expected = hashlib.sha256((root / name).read_bytes()).digest()
            if actual != expected:
                raise ValueError(f"Release and checkout differ: {name}. Generate from the release checkout.")
        member = archive.getmember(prefix + "/" + PUBLISHER_INPUT)
        if not member.isfile():
            raise ValueError(f"Release input is not a regular file: {PUBLISHER_INPUT}")
        actual = json.load(archive.extractfile(member))
        expected = json.loads((root / PUBLISHER_INPUT).read_text())
        # The release archive cannot contain its own final checksum. Compare only
        # identity fields shared by the embedded UI and generated metadata.
        for key in ("appId", "developerId", "developerName", "repository"):
            if actual.get(key) != expected.get(key):
                raise ValueError(f"Release and checkout differ: publisher {key}")


if __name__ == "__main__":
    root = Path(sys.argv[1])
    spec = importlib.util.spec_from_file_location("generate", root / "packaging/flatpak/generate.py")
    generate = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(generate)
    publisher = json.loads((root / "packaging/flatpak/publisher.json").read_text())
    generate.validate_publisher(publisher, True)
    spec = importlib.util.spec_from_file_location("download", root / "packaging/linux/download.py")
    download = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(download)
    with tempfile.TemporaryDirectory(prefix="tagrove-release-") as temporary:
        destination = Path(temporary)
        download.download({"release": {"url": publisher["releaseUrl"], "sha256": publisher["releaseSha256"]}}, destination)
        verify_archive(root, destination / "release")
    print("Verified public release digest, lockfiles, version and packaging inputs")
