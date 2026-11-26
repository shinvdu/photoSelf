# Contributing

Thanks for taking a look at photoSelf.

## Development setup

```bash
rustup toolchain install stable --component rustfmt clippy
cd ui && npm install && npm run build && cd ..
cp config.example.toml config.toml
# edit config.toml to point at a folder with test media
cargo run -p photoself-server -- serve -c config.toml
```

For UI work:

```bash
cd ui && npm run dev
# Vite proxies /api and /healthz to 127.0.0.1:8787
```

## Before opening a PR

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
(cd ui && npm ci && npm run build)
```

If you change the embedded UI or static hosting, also check:

```bash
cargo build -p photoself-server --release --features embed-ui
```

## Scope

The project deliberately stays small. Before starting a large feature, open an
issue describing the use case. In particular:

- keep the filesystem as the source of truth;
- do not make the router/OpenWrt case special in code — use configuration;
- keep media streaming out of Rust when a reverse proxy can do it;
- keep ffmpeg an external process, never link it;
- preserve read-only-by-default behavior and safe writes (trash, atomic rename).

## Test media

Use only synthetic or CC0 images/videos. A quick fixture:

```bash
mkdir -p /tmp/photoself-media/2024
ffmpeg -f lavfi -i testsrc2=size=1200x800:rate=1 -frames:v 1 /tmp/photoself-media/2024/a.jpg
ffmpeg -f lavfi -i testsrc2=size=640x360:rate=15 -f lavfi -i sine=frequency=440 \
  -t 2 -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest /tmp/photoself-media/2024/a.mp4
```

## Commit style

Conventional Commits are appreciated (`feat:`, `fix:`, `docs:`, `refactor:`).
Keep commits focused; include tests for behavior changes.

## License

By contributing you agree that your contribution is licensed under
MIT OR Apache-2.0, matching the project.

## Releases

Tags matching `v*` (for example `v0.1.1`) trigger
[`.github/workflows/release.yml`](.github/workflows/release.yml). It builds the
UI, cross-compiles static x86_64/aarch64/armv7 musl binaries, and publishes
OpenWrt `.ipk` and `openwrt-*.tar.gz` assets with checksums.
