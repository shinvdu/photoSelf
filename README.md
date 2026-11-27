# photoSelf

English | [简体中文](README.zh-CN.md)

[![CI](https://github.com/shinvdu/photoSelf/actions/workflows/ci.yml/badge.svg)](https://github.com/shinvdu/photoSelf/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Release](https://img.shields.io/github/v/release/shinvdu/photoSelf?sort=semver)](https://github.com/shinvdu/photoSelf/releases)

A lightweight, config-driven, self-hosted photo and video gallery.

- **Backend:** Rust (single binary)
- **Frontend:** Vue 3 + Vite
- **Storage:** your existing folders; SQLite is only an index
- **Design:** read-only by default, low memory, reverse-proxy friendly
- **Status:** v0.2 — browsing, tags, albums, upload/delete + trash, web-video derivative queue, scheduled scans and PWA

![photoSelf timeline](docs/screenshot.png)

The design document (Chinese) lives at [`docs/design.md`](docs/design.md).

---

## Documentation

- [Installation](docs/installation.md) · [安装文档](docs/installation.zh-CN.md)
- [Usage](docs/usage.md) · [使用说明](docs/usage.zh-CN.md)
- [Local development environment](docs/local-development.md)（中文，本地开发环境）
- [AI installation prompt](docs/ai-install-prompt.md) · [AI 安装提示词](docs/ai-install-prompt.zh-CN.md)
- [All docs](docs/README.md)
- [Technical design](docs/design.md)（中文）
- [OpenWrt notes](deploy/openwrt/README.md)
- [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md)

## Why another gallery?

Most self-hosted galleries assume a powerful server and want to import your library. photoSelf is aimed at a different point:

- run on a **512 MB–2 GB** ARM box, NAS, mini PC or router;
- index **existing directories** without moving files;
- serve media through **Nginx / Caddy / lighttpd**, not through the app;
- keep the core **generic**; device-specific behavior is configuration;
- stay small enough to audit and easy to cross-compile.

---

## Features

### Working in v0.2

- Recursive scan of one or more libraries, with ignore globs
- Indexes JPEG/PNG/WebP/GIF/BMP/TIFF plus HEIC/HEIF/AVIF by extension
- Incremental metadata: size/mtime, EXIF date/camera/orientation, video probe via ffprobe
- Content hashes (BLAKE3) for stable thumbnails and rename detection
- Timeline grid grouped by day, lazy thumbnails, lightbox, video playback
- Image thumbnails and video poster frames (ffmpeg)
- Favorites
- Search by filename
- Manual and scheduled scanning (`scan.interval`, e.g. `15m`)
- Tags: create/delete, attach/detach, filter timeline by tag
- Albums: create/delete, add/remove items, cover image
- Upload, delete-to-trash, restore and permanent purge (write mode)
- Background web-MP4 transcoding queue with job status (`play_url`)
- PWA / installable offline shell
- Multilingual UI (Simplified Chinese and English, Chinese by default)
- Lightbox zoom/pan (wheel, pinch, double-tap, buttons) with original-resolution
  loading; portrait screens get a collapsible bottom sheet for metadata/actions
- Pastel light and dark themes (system-aware on first visit, toggle in the header)
- Thumbnail context menu (right-click / long-press) and multi-select batch actions:
  view, download, add to album, delete
- Optional UI library manager: add/disable/remove directories, trigger per-library
  scans, and browse the server filesystem inside a configured allow-list
- `none` / `basic` / `proxy` authentication
- Standalone mode (Rust serves API, media and UI) or reverse-proxy mode
- Embedded-UI release build (optional feature)
- SQLite index with WAL; deleting the DB is safe — it rebuilds from files
- Graceful handling of missing files; `.DS_Store` / `._*` ignored by default

### Planned

- Multi-user accounts and share links
- Full-text search
- Rename / move UI and thumbnail pre-warming
- Automatic trash retention sweep

---

## Quick start

Requirements: Rust stable (tested with 1.97), Node 20+, `ffmpeg`/`ffprobe` for video posters.

```bash
# 1. Build the UI
cd ui
npm install
npm run build
cd ..

# 2. Configure
cp config.example.toml config.toml
# edit config.toml: at least [[libraries]] path, listen address and data dir

# 3. Run
cargo run -p photoself-server -- serve -c config.toml
```

Open <http://127.0.0.1:8787/> and press **Scan**.

Useful commands:

```bash
photoself serve -c config.toml            # start server
photoself scan -c config.toml             # scan and exit
photoself scan -c config.toml --library family
photoself hash-password --password '...'  # Argon2id hash for auth.mode = "basic"
```

Prebuilt Linux binaries (x86_64 / aarch64 / armv7) and OpenWrt `.ipk` packages are available under [Releases](https://github.com/shinvdu/photoSelf/releases).

For a single-file release build with the UI embedded:

```bash
cargo build -p photoself-server --release --features embed-ui
# requires ui/dist to exist; output: target/release/photoself
```

---

## Install with an AI agent

Copy this into Claude Code, Cursor, or another coding agent:

```text
Install photoSelf from https://github.com/shinvdu/photoSelf.
Read docs/installation.md and docs/usage.md first.

Ask me for: install target (source/Docker/OpenWrt), photo library path(s),
writable data directory, port/proxy, auth mode, and whether HEIC/HEVC sidecars
are needed. Keep the photo library read-only; never move or delete photos.

Build and start the service, run a scan, verify /healthz and one thumbnail,
then report the URL, service name, data directory, library IDs and scan results.
```

Full prompt and safety rules: [`docs/ai-install-prompt.md`](docs/ai-install-prompt.md).

## Configuration

See [`config.example.toml`](config.example.toml) for every option. The important parts:

```toml
[[libraries]]
id = "family"
name = "Family photos"
path = "/photos"
read_only = true
```

- `server.listen` — bind address, e.g. `127.0.0.1:8787` behind a proxy or `0.0.0.0:8787` on a trusted LAN.
- `data.dir` — SQLite, thumbnail cache and temporary files.
- `media.serve_mode` — `standalone` (this server serves media) or `proxy` (Nginx/lighttpd serves `/media` and `/thumbs`).
- `auth.mode` — `none`, `basic` (single user, Argon2id), or `proxy` (trust `X-Forwarded-User` only from `server.trusted_proxies` CIDRs).
- `thumbnails.concurrency` — keep `1` on 512 MB devices.
- `scan.hash` — content hashing enables rename detection and stable thumbnails; disable only for very slow storage.

### Managing libraries from the UI

Libraries are imported from `[[libraries]]` on first start; after that the
SQLite registry is authoritative. To add/remove directories from the web
interface, enable the optional manager and give it an allow-list:

```toml
[manage]
enabled = true
allowed_roots = ["/mnt/sda1", "/mnt/photos"]
require_auth = true
```

The UI can only add paths inside `allowed_roots`, and the directory picker
cannot browse outside them. Removing a library deletes its index (and tag /
album links) but never the source files. `config.toml` keeps seeding new ids;
use **Re-import from config** in the UI to force config values over existing
records. Management endpoints are refused when `auth.mode = "none"` and
`require_auth = true`.

### Pre-generated previews (HEIC / HEVC on low-power devices)

OpenWrt's ffmpeg package often has no software HEVC/HEIF decoder, and iPhones
sometimes store HEIC files with a `.JPG` name. Generate previews on a PC/NAS
and let photoSelf serve them directly:

```bash
# source can be a local folder, NFS/SMB mount, or sshfs
./scripts/make-sidecars.py \
  --source /mnt/router/picture \
  --output /mnt/router/usb/photo-app/sidecars/family \
  --sizes 320,1280 --video-sizes 320 --jobs 2

# add to config.toml
[thumbnails]
sidecar_dir = "/mnt/usb/photo-app/sidecars"
```

The output directory should be `<sidecar_dir>/<library-id>/`. The script writes
`<relative-path>.<size>.jpg` and `<relative-path>.meta.json`
(width/height/duration/date). Existing thumbnails are skipped unless `--force`;
`--meta-only` refreshes only metadata. Requirements: Python 3 + Pillow,
ffmpeg/ffprobe, and `heif-convert` (libheif-examples) for HEIC.

### Low-memory profile

```toml
[thumbnails]
sizes = [320]
concurrency = 1

[ffmpeg]
concurrency = 1

[video]
transcode_enabled = false

[scan]
on_start = false
concurrency = 1
```

---

## HTTP API (v0.2)

Base path: `/api/v1`. Errors use `application/problem+json`.
All `POST`/`PUT`/`PATCH`/`DELETE` requests must send
`X-Requested-With: photoSelf` (CSRF guard).

| Method | Path | Purpose |
|---|---|---|
| GET | `/healthz` | liveness (unauthenticated) |
| GET | `/api/v1/info` | version, mode, libraries, counts |
| GET | `/api/v1/items` | timeline; `library`, `kind`, `q`, `favorite`, `tag`, `album`, `from`, `to`, `cursor`, `limit` |
| GET | `/api/v1/items/{id}` | item detail (includes tags and `play_url`) |
| PATCH | `/api/v1/items/{id}` | `{ "favorite": true }` |
| DELETE | `/api/v1/items/{id}` | move to trash (write mode) |
| GET/POST | `/api/v1/tags` | list/create tags |
| DELETE | `/api/v1/tags/{id}` | delete a tag |
| POST/DELETE | `/api/v1/items/{id}/tags[/{tag_id}]` | attach/detach a tag |
| GET/POST | `/api/v1/albums` | list/create albums |
| GET/DELETE | `/api/v1/albums/{id}` | album detail/delete |
| GET/POST | `/api/v1/albums/{id}/items` | list/add album items |
| DELETE | `/api/v1/albums/{id}/items/{item_id}` | remove from album |
| POST | `/api/v1/upload` | multipart upload (`library`, `file`) |
| GET | `/api/v1/trash` | list trash |
| POST/DELETE | `/api/v1/trash/{id}[/restore]` | restore or purge |
| POST | `/api/v1/items/{id}/transcode` | queue a web MP4 |
| GET | `/api/v1/items/{id}/derived` | serve the web MP4 (Range) |
| GET | `/api/v1/items/{id}/thumb/{size}` | JPEG thumbnail |
| GET | `/api/v1/items/{id}/media` | original media, supports HTTP Range |
| POST | `/api/v1/scan` | start a scan job |
| GET | `/api/v1/jobs` | recent jobs |
| GET | `/api/v1/jobs/{id}` | job status/progress |

List responses use cursor pagination:

```json
{ "items": [ ... ], "next_cursor": "..." }
```

---

## Deployment

### Docker

```bash
docker build -t photoself .
docker run -d --name photoself \
  -p 8787:8787 \
  -v /srv/photos:/photos:ro \
  -v photoself-data:/data \
  -v "$PWD/config.docker.toml:/data/config.toml:ro" \
  photoself
```

Start from [`deploy/docker/config.example.toml`](deploy/docker/config.example.toml) and
copy it to `config.docker.toml`. The image includes ffmpeg for video posters.

The Dockerfile uses `public.ecr.aws/docker/library/*`, an AWS-hosted mirror of the
official Docker Hub images, which is often reachable when `registry-1.docker.io`
is not. If `npm ci`, `cargo` or `apt-get` need a proxy, build with:

```bash
docker build --network=host \
  --build-arg HTTP_PROXY=http://127.0.0.1:1081 \
  --build-arg HTTPS_PROXY=http://127.0.0.1:1081 \
  -t photoself .
```

(`--network=host` makes `127.0.0.1` inside the build container refer to the
host; without it, use your Docker bridge address such as `172.17.0.1`.)

### systemd

See [`deploy/systemd/photoself.service`](deploy/systemd/photoself.service). It assumes:

- binary: `/usr/local/bin/photoself`
- config: `/etc/photoself/config.toml`
- data: `/var/lib/photoself`
- photos: `/photos` (read-only)

### OpenWrt / low-power Linux

A typical low-memory deployment:

1. Build a static ARM64 binary (SQLite bundled needs a C cross compiler; `cargo-zigbuild` works):

   ```bash
   rustup target add aarch64-unknown-linux-musl
   cargo install cargo-zigbuild --locked
   cd ui && npm ci && npm run build && cd ..
   cargo zigbuild --release --target aarch64-unknown-linux-musl \
     -p photoself-server --features embed-ui
   ```

2. Install:

   ```sh
   # Optional: OpenWrt ffmpeg can demux but often cannot *decode* HEVC.
   # For HEIC/HEVC libraries, generate sidecar previews on a PC instead.
   opkg install ffmpeg ffprobe        # optional; poster fallback for supported codecs
   scp target/aarch64-unknown-linux-musl/release/photoself root@router:/usr/bin/photoself
   scp config.example.toml root@router:/etc/photoself/config.toml
   scp deploy/openwrt/photoself.init root@router:/etc/init.d/photoself
   ssh root@router 'chmod +x /etc/init.d/photoself; /etc/init.d/photoself enable; /etc/init.d/photoself start'
   ```

3. Put config, SQLite and thumbnails on USB storage (`data.dir`), and keep only the binary/config on NAND. Set `thumbnails.concurrency = 1`, `ffmpeg.concurrency = 1`, `video.transcode_enabled = false`.

4. Put lighttpd/Nginx in front for TLS, auth and media serving. See
   [`deploy/lighttpd/94-photoself.conf`](deploy/lighttpd/94-photoself.conf) for a lighttpd example;
   the design doc has a detailed config walkthrough.

Example OpenWrt config (paths are configuration, not code):

```toml
[server]
mode = "reverse-proxy"
listen = "127.0.0.1:8787"

[data]
dir = "/mnt/usb/photo-app/data"

[[libraries]]
id = "family"
path = "/mnt/photos"
read_only = true

[media]
serve_mode = "proxy"
public_media_path = "/photo/media"
public_thumb_path = "/photo/thumbs"

[thumbnails]
sizes = [320, 1280]
concurrency = 1

[ffmpeg]
concurrency = 1

[video]
transcode_enabled = false

[auth]
mode = "proxy"

[scan]
on_start = false
```

### Reverse proxy notes

- `auth.mode = "proxy"`: the proxy must authenticate and set `X-Forwarded-User`.
- `media.serve_mode = "proxy"`: the proxy must map
  `/media/<library-id>/<relative-path>` to the library root, and
  `/thumbs/...` to `data/cache/thumbs/...`.
- Media URLs are content-addressed for thumbnails; long cache headers are safe.
- Do **not** expose the app directly to the internet without auth and TLS.

---

## Project layout

```text
photoSelf/
├── crates/
│   ├── photoself-core/     # config, SQLite, scanner, metadata, thumbnails
│   └── photoself-server/   # axum API, auth, static UI, CLI
├── ui/                     # Vue 3 + Vite SPA
├── deploy/                 # systemd, procd, nginx, lighttpd
├── docs/                   # design doc + screenshot
├── config.example.toml
├── scripts/
│   └── make-sidecars.py    # HEIC/HEVC preview generation for low-power devices
├── Dockerfile
└── .github/workflows/ci.yml
```

## Development

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# UI dev server (proxies /api to 127.0.0.1:8787)
cd ui && npm install && npm run dev
```

Testing uses synthetic images/videos generated with ffmpeg; no personal data is required.

---

## Status and known limitations

This is a **v0.2 MVP**, not a finished product:

- Write mode (upload/delete/trash) is disabled by default; enable it deliberately and keep
  the API behind authentication. The photo library should be writable only when you need it.
- Video playback uses the original unless a web MP4 is generated; MOV/HEVC compatibility
  depends on the browser and on ffmpeg having a decoder/encoder.
- Trash retention is manual: purge old entries yourself (no automatic sweep yet).
- Reserved/not fully enforced yet: `scan.concurrency`, `missing_retention_days`,
  `thumbnails.prewarm_on_scan`, `auth.session_ttl_hours`, `server.base_path`
  and `server.ui_embedded`.
- `auth.mode = "proxy"` trusts `X-Forwarded-User` only from peers listed in
  `server.trusted_proxies`; only use it behind a trusted proxy that strips client-supplied headers.
- EXIF dates without timezone are stored as naive local timestamps.
- HEIC/RAW are indexed but thumbnails may fail unless ffmpeg supports them.
- SQLite is compiled with `bundled`; cross builds need a C compiler (use zigbuild or the OpenWrt SDK).
- The embedded UI feature must be built after `ui/dist` exists.

## Roadmap

- **v0.2/v0.3 (current):** tags, albums, upload/delete + trash, search filters,
  web-MP4 queue, scheduled scans, PWA
- **Next:** multi-user/share links, full-text search, rename/move UI, automatic trash retention
- **v1.0:** stable API, migrations, packaging, security review

## Design

- [`docs/design.md`](docs/design.md) — architecture, data model, API, deployment, risks (Chinese)

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

`ffmpeg` is **not** bundled in the Rust binary or release archive. It is an optional external
program; if you bundle it in a container or distribution, respect its LGPL/GPL obligations.
