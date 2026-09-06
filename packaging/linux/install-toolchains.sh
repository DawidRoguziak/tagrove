#!/usr/bin/env bash
set -euo pipefail
cd /tmp
python3 /inputs/packaging/linux/download.py /inputs/packaging/linux/toolchains.json /tmp/toolchains
tar -xJf toolchains/node --strip-components=1 -C /usr/local
unzip -q toolchains/bun
install -m755 bun-linux-x64/bun /usr/local/bin/bun
mkdir rust-install
tar -xJf toolchains/rust --strip-components=1 -C rust-install
./rust-install/install.sh --prefix=/usr/local --disable-ldconfig --components=rustc,cargo,rust-std-x86_64-unknown-linux-gnu,rustfmt-preview,clippy-preview
rm -rf toolchains bun-linux-x64 rust-install
rustc -vV
bun --version
node --version
