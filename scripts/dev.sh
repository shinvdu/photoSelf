#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

if [ ! -d ui/node_modules ]; then
	(cd ui && npm install --no-audit --no-fund)
fi

# Backend on 8787; the Vite dev server proxies /api to it.
cargo run -p photoself-server -- serve -c "${PHOTOSELF_CONFIG:-config.toml}" &
api_pid=$!
trap 'kill "$api_pid" 2>/dev/null || true' EXIT INT TERM

(cd ui && npm run dev)
