#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

if [ ! -d ui/node_modules ]; then
	(cd ui && npm install --no-audit --no-fund)
fi
(cd ui && npm run build)

cargo build -p photoself-server --release --features embed-ui
echo "binary: target/release/photoself"
