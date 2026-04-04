#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE_NAME="termtube-arm64-builder"

cd "$ROOT"

echo "Building ARM64 Docker image..."
docker buildx build --platform linux/arm64 -f "$ROOT/packaging/Dockerfile.arm64" -t "$IMAGE_NAME" "$ROOT" --load

echo "Building ARM64 binary inside container..."
docker run --rm \
  --platform linux/arm64 \
  --user "$(id -u):$(id -g)" \
  --device /dev/fuse:/dev/fuse \
  --cap-add SYS_ADMIN \
  -e TARGET_ARCH=aarch64 \
  -e SKIP_PACKAGE=true \
  -v "$ROOT":/workspace \
  -w /workspace \
  "$IMAGE_NAME" \
  bash packaging/build-appimage.sh

echo "Packaging ARM64 AppImage on host using x86 linuxdeploy..."
bash packaging/build-arm64-host.sh

echo "ARM64 AppImage build complete."
