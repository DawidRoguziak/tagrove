#!/usr/bin/env bash
set -euo pipefail
[[ $# == 1 ]] || { echo 'Usage: install-gitleaks.sh DESTINATION' >&2; exit 2; }
[[ "$(uname -s):$(uname -m)" == Linux:x86_64 ]]
mkdir -p "$1"
destination="$(cd "$1" && pwd)"
temporary="$(mktemp -d)"
trap 'rm -rf -- "$temporary"' EXIT
cd "$temporary"
curl --fail --location --silent --show-error --proto '=https' --tlsv1.2 \
  https://github.com/gitleaks/gitleaks/releases/download/v8.30.1/gitleaks_8.30.1_linux_x64.tar.gz -o gitleaks.tar.gz
printf '%s  gitleaks.tar.gz\n' 551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb | sha256sum --check --strict
tar -xzf gitleaks.tar.gz gitleaks
install -m 755 gitleaks "$destination/gitleaks"
