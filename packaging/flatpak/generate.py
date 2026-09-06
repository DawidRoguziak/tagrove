"""Generate both local and publication manifests from the same locked inputs."""
import argparse
import base64
from datetime import date
import json
from pathlib import Path
import re
import tomllib
from urllib.parse import unquote, urlparse
import xml.etree.ElementTree as ET


def read_bun_lock(path):
    # Bun's text lockfile is JSON with trailing commas. Preserve string contents.
    text = re.sub(r'("(?:\\.|[^"\\])*")|,\s*(?=[}\]])',
                  lambda match: match[1] or "", path.read_text())
    return json.loads(text)


def inline_file(name, contents, dest=None):
    source = {"type": "inline", "dest-filename": name, "contents": contents}
    if dest:
        source["dest"] = dest
    return source


def dependency_sources(root):
    sources = []
    seen = set()
    for package in read_bun_lock(root / "bun.lock")["packages"].values():
        locator, registry, _, integrity = package
        name, version = locator.rsplit("@", 1)
        if locator in seen:
            continue
        seen.add(locator)
        if registry or not re.fullmatch(r"(?:@[\w.-]+/)?[\w.-]+", name):
            raise ValueError(f"Unsupported non-registry Bun dependency: {locator}")
        if not re.fullmatch(r"\d+\.\d+\.\d+", version):
            raise ValueError(f"Bun cache naming needs explicit support for {locator}")
        algorithm, digest = integrity.split("-", 1)
        if algorithm not in ("sha512", "sha256"):
            raise ValueError(f"Unsupported integrity algorithm: {algorithm}")
        sources.append({
            "type": "archive",
            "url": f"https://registry.npmjs.org/{name}/-/{name.split('/')[-1]}-{version}.tgz",
            algorithm: base64.b64decode(digest, validate=True).hex(),
            "dest": f"bun-cache/{name}@{version}@@@1",
        })
    lock = tomllib.loads((root / "src-tauri/Cargo.lock").read_text())
    for package in lock["package"]:
        if "source" not in package:
            continue
        if package["source"] != "registry+https://github.com/rust-lang/crates.io-index":
            raise ValueError(f"Unsupported Cargo source: {package['source']}")
        name, version = package["name"], package["version"]
        dest = f"cargo-vendor/{name}-{version}"
        sources.extend([{
            "type": "archive",
            "archive-type": "tar-gzip",
            "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
            "sha256": package["checksum"],
            "dest": dest,
        }, inline_file(".cargo-checksum.json", json.dumps({
            "package": package["checksum"], "files": {},
        }), dest)])
    sources.append(inline_file("config.toml", '''[source.crates-io]
replace-with = "vendored-sources"
[source.vendored-sources]
directory = "../cargo-vendor"
[net]
offline = true
''', "src-tauri/.cargo"))
    return sources


def validate_publisher(publisher, publication=False):
    app_id = publisher["appId"]
    if (len(app_id) > 255
            or not re.fullmatch(r"[a-z_][a-z0-9_]*(?:\.[a-z_][a-z0-9_]*){1,3}\.[A-Za-z_][A-Za-z0-9_-]*", app_id)
            or app_id.endswith((".dev", ".e2e"))):
        raise ValueError("Release appId must be a reverse-DNS identity outside dev/E2E")
    if not publication:
        return
    if any(word in app_id.lower().split(".") for word in ("example", "test", "placeholder")):
        raise ValueError("Publication requires a non-placeholder appId")
    if app_id.rsplit(".", 1)[1].lower() in ("desktop", "app", "linux"):
        raise ValueError("Publication appId must not end in a generic platform term")
    for key in ("repository", "releaseUrl", "releaseRef", "releaseDate", "developerId", "developerName"):
        if not publisher.get(key):
            raise ValueError(f"Publication requires {key}")
    if not re.fullmatch(r"[0-9a-f]{64}", publisher.get("releaseSha256") or ""):
        raise ValueError("Publication requires a pinned releaseSha256")
    if not publisher.get("screenshots"):
        raise ValueError("Publication requires public screenshot URLs")
    for url in [publisher["repository"], publisher["releaseUrl"], *publisher["screenshots"]]:
        parsed = urlparse(url)
        if (parsed.scheme != "https" or not parsed.hostname or parsed.username
                or "." not in parsed.hostname or parsed.hostname.endswith((".local", ".localhost"))
                or parsed.hostname in ("example.com", "example.org", "127.0.0.1", "0.0.0.0")
                or re.fullmatch(r"[\d.:]+", parsed.hostname)):
            raise ValueError(f"Publication requires public HTTPS sources: {url}")
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", publisher["releaseDate"]):
        raise ValueError("releaseDate must be YYYY-MM-DD")
    date.fromisoformat(publisher["releaseDate"])
    for prefix, host in (("io.github.", "github.com"), ("io.gitlab.", "gitlab.com"),
                         ("page.codeberg.", "codeberg.org"), ("io.frama.", "framagit.org")):
        if app_id.startswith(prefix):
            components = app_id[len(prefix):].split(".")
            if len(components) < 2:
                raise ValueError("Code-hosting appId requires an owner and repository")
            namespace = [part.lstrip("_").replace("_", "-") for part in components[:-1]]
            product = re.sub(r"^_(?=[0-9])", "", components[-1])
            expected = "/" + "/".join([*namespace, product])
            repository = urlparse(publisher["repository"])
            if repository.hostname != host or repository.path.rstrip("/").removesuffix(".git") != expected:
                raise ValueError(f"Code-hosting appId requires repository https://{host}{expected}")
    release_names = [publisher["releaseRef"] + suffix for suffix in ("", ".tar.gz", ".tar.xz", ".tgz")]
    if not any(part in release_names for part in unquote(urlparse(publisher["releaseUrl"]).path).split("/")):
        raise ValueError("releaseUrl must identify releaseRef in its path")


def metadata(root, publisher):
    component = ET.Element("component", type="desktop-application")
    for key, value in [
        ("id", publisher["appId"]), ("name", "Tagrove"),
        ("summary", "Browse and tag images, videos and GIFs"),
        ("metadata_license", "CC0-1.0"), ("project_license", "GPL-3.0-or-later"),
    ]:
        ET.SubElement(component, key).text = value
    description = ET.SubElement(component, "description")
    ET.SubElement(description, "p").text = (
        "Organize large local media collections with tags, favorites and media groups. "
        "Browse images and GIFs, play videos, and search your library without moving your files.")
    ET.SubElement(component, "launchable", type="desktop-id").text = publisher["appId"] + ".desktop"
    ET.SubElement(component, "content_rating", type="oars-1.1")
    if publisher.get("repository"):
        ET.SubElement(component, "url", type="homepage").text = publisher["repository"]
    if publisher.get("developerName"):
        developer = ET.SubElement(component, "developer", id=publisher["developerId"])
        ET.SubElement(developer, "name").text = publisher["developerName"]
    version = json.loads((root / "package.json").read_text())["version"]
    release = {"version": version, "date": publisher.get("releaseDate") or date.today().isoformat()}
    ET.SubElement(ET.SubElement(component, "releases"), "release", release)
    if publisher.get("screenshots"):
        screenshots = ET.SubElement(component, "screenshots")
        for index, url in enumerate(publisher["screenshots"]):
            screenshot = ET.SubElement(screenshots, "screenshot", {"type": "default"} if index == 0 else {})
            ET.SubElement(screenshot, "image", type="source").text = url
    ET.indent(component)
    return '<?xml version="1.0" encoding="UTF-8"?>\n' + ET.tostring(component, encoding="unicode") + "\n"


def generate(root, publisher, publication=False):
    validate_publisher(publisher, publication)
    tools = json.loads((root / "packaging/linux/toolchains.json").read_text())
    tool_sources = [dict(type="archive", dest=f"toolchain-{name}", **source)
                    for name, source in tools.items()]
    app_sources = [{"type": "archive", "url": publisher["releaseUrl"],
                    "sha256": publisher["releaseSha256"]}] if publication else [
                        {"type": "dir", "path": str(root.resolve())}]
    app_sources += dependency_sources(root)
    overlay = json.loads((root / "src-tauri/tauri.flatpak.conf.json").read_text())
    overlay["identifier"] = publisher["appId"]
    desktop = (root / "src-tauri/linux/com.example.mediatagger.desktop").read_text()
    desktop = desktop.replace("Icon=tagrove", "Icon=" + publisher["appId"])
    app_sources.extend([
        inline_file("tauri.flatpak.generated.json", json.dumps(overlay), "src-tauri"),
        inline_file("tagrove.desktop", desktop),
        inline_file("tagrove.metainfo.xml", metadata(root, publisher)),
        inline_file("publisher.json", json.dumps(publisher)),
    ])
    return {
        "app-id": publisher["appId"], "runtime": "org.gnome.Platform",
        "runtime-version": "50", "sdk": "org.gnome.Sdk", "command": "media_tagger",
        "default-branch": "stable",
        "finish-args": ["--share=ipc", "--socket=wayland", "--socket=fallback-x11",
                        "--socket=pulseaudio", "--device=dri", "--filesystem=home",
                        "--filesystem=/mnt", "--filesystem=/media", "--filesystem=/run/media"],
        "build-options": {"build-args": ["--unshare=network"], "env": {
            "PATH": "/app/tools/bin:/app/bin:/usr/bin", "CARGO_NET_OFFLINE": "true",
            "CARGO_BUILD_JOBS": "4", "CARGO_PROFILE_RELEASE_DEBUG": "0",
            "RUSTFLAGS": "-L native=/app/lib -C link-arg=-Wl,-rpath,/app/lib",
        }},
        "cleanup": ["/tools", "/include", "/lib/pkgconfig", "*.a", "*.la", "/share/man"],
        "modules": [{
            "name": "build-tools", "buildsystem": "simple", "cleanup": ["*"],
            "build-options": {"no-debuginfo": True, "strip": False},
            "build-commands": [
                "mkdir -p /app/tools/bin",
                "install -m755 toolchain-bun/bun /app/tools/bin/bun",
                "cp -a toolchain-node/. /app/tools/",
                "toolchain-rust/install.sh --prefix=/app/tools --disable-ldconfig --components=rustc,cargo,rust-std-x86_64-unknown-linux-gnu",
            ], "sources": tool_sources,
        }, *json.loads((root / "packaging/flatpak/media.json").read_text()), {
            "name": "tagrove", "buildsystem": "simple",
            "build-commands": ["bash packaging/flatpak/install.sh"], "sources": app_sources,
        }],
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--publisher", type=Path)
    parser.add_argument("--publication", action="store_true")
    parser.add_argument("--metadata-output", type=Path)
    args = parser.parse_args()
    publisher_path = args.publisher or args.root / "packaging/flatpak/publisher.json"
    publisher = json.loads(publisher_path.read_text())
    manifest = generate(args.root, publisher, args.publication)
    args.output.write_text(json.dumps(manifest, indent=4) + "\n")
    if args.metadata_output:
        args.metadata_output.write_text(metadata(args.root, publisher))
