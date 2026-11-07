#!/usr/bin/env bash
# Points pkg/homebrew/termtube.rb and pkg/aur/{PKGBUILD,.SRCINFO} at a released version.
# Usage: scripts/update-packages.sh <version>   (e.g. 0.2.2; the tag v<version> must exist)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${1:?usage: $0 <version>}"
URL="https://github.com/javierlongoriaroche/termtube/archive/refs/tags/v$VERSION.tar.gz"

SHA="$(curl -fsSL "$URL" | shasum -a 256 | cut -d' ' -f1)"
echo "v$VERSION source tarball sha256: $SHA"

sed -i.bak -E \
    -e "s#archive/refs/tags/v[0-9.]+\.tar\.gz#archive/refs/tags/v$VERSION.tar.gz#" \
    -e "s#sha256 \"[0-9a-f]{64}\"#sha256 \"$SHA\"#" \
    "$ROOT/pkg/homebrew/termtube.rb"
sed -i.bak -E \
    -e "s#^pkgver=.*#pkgver=$VERSION#" \
    -e "s#^pkgrel=.*#pkgrel=1#" \
    -e "s#^sha256sums=.*#sha256sums=('$SHA')#" \
    "$ROOT/pkg/aur/PKGBUILD"
sed -i.bak -E \
    -e "s#pkgver = .*#pkgver = $VERSION#" \
    -e "s#pkgrel = .*#pkgrel = 1#" \
    -e "s#termtube-[0-9.]+\.tar\.gz::#termtube-$VERSION.tar.gz::#" \
    -e "s#refs/tags/v[0-9.]+\.tar\.gz#refs/tags/v$VERSION.tar.gz#" \
    -e "s#sha256sums = .*#sha256sums = $SHA#" \
    "$ROOT/pkg/aur/.SRCINFO"
rm -f "$ROOT"/pkg/homebrew/*.bak "$ROOT"/pkg/aur/*.bak "$ROOT"/pkg/aur/.*.bak
echo "Updated pkg/homebrew/termtube.rb and pkg/aur/ for v$VERSION"
