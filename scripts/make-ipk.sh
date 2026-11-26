#!/bin/sh
# Build an OpenWrt .ipk from a cross-compiled photoself binary.
#
# Usage:
#   scripts/make-ipk.sh <binary> <architecture> <version> <output-dir>
#
# Example:
#   scripts/make-ipk.sh target/aarch64-unknown-linux-musl/release/photoself \
#       aarch64_cortex-a53 0.1.0-1 release
#
# The package installs:
#   /usr/bin/photoself
#   /etc/init.d/photoself
#   /etc/photoself/config.toml  (edit before starting)
#
# .ipk is a gzip-compressed tar containing debian-binary, control.tar.gz and
# data.tar.gz, compatible with OpenWrt opkg (23.05/24.10).
set -eu

if [ "$#" -ne 4 ]; then
    echo "usage: $0 <binary> <architecture> <version> <output-dir>" >&2
    exit 2
fi

BINARY=$1
ARCH=$2
VERSION=$3
OUT_DIR=$4
ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
INIT="$ROOT_DIR/deploy/openwrt/photoself.init"
CONFIG="$ROOT_DIR/deploy/openwrt/config.release.toml"

[ -x "$BINARY" ] || { echo "binary not found/executable: $BINARY" >&2; exit 1; }
[ -f "$INIT" ] || { echo "init script not found: $INIT" >&2; exit 1; }
[ -f "$CONFIG" ] || { echo "config not found: $CONFIG" >&2; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
PKG_ROOT="$TMP/root"
CONTROL="$TMP/control"
mkdir -p "$PKG_ROOT/usr/bin" "$PKG_ROOT/etc/init.d" "$PKG_ROOT/etc/photoself" "$CONTROL"

install -m 0755 "$BINARY" "$PKG_ROOT/usr/bin/photoself"
install -m 0755 "$INIT" "$PKG_ROOT/etc/init.d/photoself"
install -m 0600 "$CONFIG" "$PKG_ROOT/etc/photoself/config.toml"

SIZE=$(find "$PKG_ROOT" -type f -printf '%s\n' | awk '{sum += $1} END {print sum + 0}')

cat > "$CONTROL/control" <<EOF
Package: photoself
Version: $VERSION
Depends:
Source: photoSelf
SourceName: photoself
License: MIT OR Apache-2.0
Section: utils
Section-Priority: optional
Maintainer: photoSelf contributors
Architecture: $ARCH
Installed-Size: $SIZE
Description: Lightweight self-hosted photo and video gallery (Rust + Vue).
EOF

printf '%s\n' '/etc/photoself/config.toml' > "$CONTROL/conffiles"
printf '2.0\n' > "$TMP/debian-binary"

(cd "$PKG_ROOT" && tar --format=gnu --numeric-owner --owner=0 --group=0 \
    --sort=name -cpf - . | gzip -n - > "$TMP/data.tar.gz")
(cd "$CONTROL" && tar --format=gnu --numeric-owner --owner=0 --group=0 \
    --sort=name -cpf - . | gzip -n - > "$TMP/control.tar.gz")

mkdir -p "$OUT_DIR"
OUT_DIR=$(cd "$OUT_DIR" && pwd)
OUT="$OUT_DIR/photoself_${VERSION}_${ARCH}.ipk"
(cd "$TMP" && tar --format=gnu --numeric-owner --owner=0 --group=0 \
    --sort=name -cf - ./debian-binary ./control.tar.gz ./data.tar.gz | \
    gzip -n - > "$OUT")

echo "built: $OUT"
