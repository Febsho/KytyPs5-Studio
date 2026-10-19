#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
package_dir="$(mktemp -d)"
trap 'rm -rf "$package_dir"' EXIT
pkgver="$(sed -n 's/^pkgver=//p' "$repo_root/packaging/arch/PKGBUILD")"
archive="$package_dir/kytyps5-studio-$pkgver.tar.gz"
export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-0}"
tar -C "$(dirname "$repo_root")" \
  --sort=name \
  --format=gnu \
  --mtime="@${SOURCE_DATE_EPOCH}" \
  --owner=0 --group=0 --numeric-owner \
  --exclude="$(basename "$repo_root")/node_modules" \
  --exclude="$(basename "$repo_root")/dist" \
  --exclude="$(basename "$repo_root")/src-tauri/target" \
  --exclude="$(basename "$repo_root")/.git" \
  --exclude="$(basename "$repo_root")/packaging/arch/*.pkg.tar.*" \
  --transform="s|^$(basename "$repo_root")|kytyps5-studio-$pkgver|" \
  -cf - "$(basename "$repo_root")" | gzip -n > "$archive"
cp "$repo_root/packaging/arch/PKGBUILD" "$package_dir/PKGBUILD"
checksum="$(sha256sum "$archive" | cut -d' ' -f1)"
sed -i "s/sha256sums=('SKIP')/sha256sums=('$checksum')/" "$package_dir/PKGBUILD"
(cd "$package_dir" && makepkg --cleanbuild --syncdeps -f "$@")
cp "$package_dir"/*.pkg.tar.* "$repo_root/packaging/arch/"
