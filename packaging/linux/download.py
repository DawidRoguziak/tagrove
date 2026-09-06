"""Download only inputs whose digest is committed to the packaging definition."""
import hashlib
import json
from pathlib import Path
import sys
import urllib.request


def download(specification, destination):
    destination.mkdir(parents=True, exist_ok=True)
    for name, source in specification.items():
        target = destination / name
        with urllib.request.urlopen(source["url"], timeout=120) as response:
            with target.open("wb") as output:
                while chunk := response.read(1024 * 1024):
                    output.write(chunk)
        actual = hashlib.file_digest(target.open("rb"), "sha256").hexdigest()
        if actual != source["sha256"]:
            target.unlink()
            raise ValueError(f"Checksum mismatch for {source['url']}: {actual}")


if __name__ == "__main__":
    download(json.loads(Path(sys.argv[1]).read_text()), Path(sys.argv[2]))
