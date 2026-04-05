#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ $# -lt 1 ]]; then
  cat <<EOF
Usage: $0 <version> [arch]

Arguments:
  version  Release version without the leading v (for example: 0.1.3)
  arch     x86_64 | aarch64 | all  (default: all)

This script builds AppImages for the specified architecture(s) and publishes a GitHub release.
EOF
  exit 1
fi

VERSION="$1"
ARCH="${2:-all}"

if [[ "$ARCH" != "x86_64" && "$ARCH" != "aarch64" && "$ARCH" != "all" ]]; then
  echo "Unsupported arch: $ARCH" >&2
  echo "Supported values: x86_64, aarch64, all" >&2
  exit 1
fi

if [[ -n "$(git -C "$ROOT" status --porcelain)" ]]; then
  echo "Git working tree is not clean. Please commit or stash changes before releasing." >&2
  git -C "$ROOT" status --short
  exit 1
fi

bash "$ROOT/.github/skills/termtube-release/scripts/release.sh" "$VERSION" "$ARCH"
