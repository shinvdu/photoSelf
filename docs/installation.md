# Installation

[简体中文](installation.zh-CN.md) | English

photoSelf runs as a single Rust binary with an optional embedded Vue UI. It reads
existing folders; it does not import or move your photos.

> There are no official prebuilt release artifacts yet. Build from source,
> Docker, or cross-compile for your device.

## Requirements

| Component | Notes |
|---|---|
| OS | Linux (glibc or musl), OpenWrt, macOS/Windows for development |
| CPU | x86_64, aarch64, armv7; OpenWrt builds should be static musl |
| Rust | stable (tested with 1.97) |
| Node.js | 20+ (build the UI only) |
| C toolchain | required for bundled SQLite (`gcc`, or `zig`/OpenWrt SDK for musl) |
| ffmpeg/ffprobe | optional; video posters and video metadata |
| Python 3 + Pillow | optional; sidecar preview generation on a PC |
| heif-convert | optional; HEIC decoding for sidecars (`libheif-examples`) |

Minimum practical runtime memory: **~50 MB RSS** for the API; thumbnails and
external ffmpeg jobs spike higher. A 512 MB device is usable with
`thumbnails.concurrency = 1`.

---

## Option A — build from source (recommended for development)

```bash
git clone https://github.com/shinvdu/photoSelf.git
cd photoSelf

# 1. Build the UI
cd ui
npm ci
npm run build
cd ..

# 2. Configure
cp config.example.toml config.toml
$EDITOR config.toml

# 3. Run
cargo run -p photoself-server -- serve -c config.toml
```

Open `http://127.0.0.1:8787/` and press **Scan**.

### Single binary with embedded UI

```bash
cd ui && npm ci && npm run build && cd ..
cargo build -p photoself-server --release --features embed-ui
# binary: target/release/photoself
./target/release/photoself serve -c config.toml
```

The `embed-ui` feature requires `ui/dist/` to exist at compile time. The binary
then serves the SPA without a separate UI directory.

### Cross-compile for ARM64 musl (OpenWrt / NAS)

Bundled SQLite needs a C compiler. `cargo-zigbuild` handles both Rust and the C
parts:

```bash
rustup target add aarch64-unknown-linux-musl
cargo install cargo-zigbuild --locked

# Zig 0.14.x; put it on PATH (or set ZIG)
export PATH="$HOME/opt/zig-x86_64-linux-0.14.1:$PATH"

(cd ui && npm ci && npm run build)
cargo zigbuild --release --target aarch64-unknown-linux-musl \
  -p photoself-server --features embed-ui

file target/aarch64-unknown-linux-musl/release/photoself
# ELF 64-bit LSB executable, ARM aarch64, statically linked
```

Alternatively use the OpenWrt SDK toolchain and point `CC` /
`CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER` at
`aarch64-openwrt-linux-musl-gcc`.

---

## Option B — Docker

```bash
docker build -t photoself .
cp deploy/docker/config.example.toml config.docker.toml
# edit paths: /photos and /data

docker run -d --name photoself \
  -p 8787:8787 \
  -v /srv/photos:/photos:ro \
  -v photoself-data:/data \
  -v "$PWD/config.docker.toml:/data/config.toml:ro" \
  photoself
```

The Dockerfile uses `public.ecr.aws/docker/library/*` (AWS Public ECR mirror of
the official images). If your build containers need an HTTP proxy:

```bash
docker build --network=host \
  --build-arg HTTP_PROXY=http://127.0.0.1:1081 \
  --build-arg HTTPS_PROXY=http://127.0.0.1:1081 \
  -t photoself .
```

---

## Option C — OpenWrt / low-power Linux

1. Cross-compile as shown above (`cargo zigbuild`).
2. Install the binary and init script:

   ```sh
   scp target/aarch64-unknown-linux-musl/release/photoself root@router:/usr/bin/photoself
   scp config.example.toml root@router:/etc/photoself/config.toml
   scp deploy/openwrt/photoself.init root@router:/etc/init.d/photoself
   ssh root@router 'chmod +x /etc/init.d/photoself
     /etc/init.d/photoself enable
     /etc/init.d/photoself start'
   ```

3. Keep the binary/config on NAND and `data.dir` on a writable USB disk.
4. Use `thumbnails.concurrency = 1`, `video.transcode_enabled = false`.
5. Put lighttpd/Nginx in front for TLS, auth and media serving. See
   [`../deploy/openwrt/README.md`](../deploy/openwrt/README.md) and
   [`../deploy/lighttpd/94-photoself.conf`](../deploy/lighttpd/94-photoself.conf).

To build an `.ipk` (opkg) or `.apk` (apk-based OpenWrt), use the OpenWrt SDK /
buildroot or package the binary and init script with your distribution tooling.
The included `deploy/openwrt/photoself.init` is a procd service.

---

## Configuration

Minimal `config.toml`:

```toml
[server]
listen = "127.0.0.1:8787"

[data]
dir = "/var/lib/photoself"

[[libraries]]
id = "photos"
name = "Photos"
path = "/photos"
read_only = true

[thumbnails]
sizes = [320, 1280]
concurrency = 1

[ffmpeg]
enabled = true
concurrency = 1

[auth]
mode = "none"
```

Important options:

| Key | Meaning |
|---|---|
| `server.listen` | `127.0.0.1:8787` behind a proxy; `0.0.0.0:8787` on a trusted LAN |
| `data.dir` | SQLite, thumbnail cache, temporary files |
| `[[libraries]].path` | photo root; mount read-only if possible |
| `media.serve_mode` | `standalone` or `proxy` |
| `auth.mode` | `none`, `basic`, or `proxy` |
| `thumbnails.sidecar_dir` | optional pre-generated preview root |
| `scan.hash` | BLAKE3 content hash for rename detection |

Run a scan from the UI or CLI:

```bash
photoself scan -c config.toml
```

### Authentication

Generate an Argon2id hash:

```bash
photoself hash-password --password 'choose-a-password'
```

Put it in the config:

```toml
[auth]
mode = "basic"
username = "admin"
password_hash = "$argon2id$..."
```

Basic auth is single-user. Behind a trusted reverse proxy, use
`auth.mode = "proxy"`; the proxy must authenticate and set `X-Forwarded-User`.

---

## systemd

```bash
sudo useradd --system --home /var/lib/photoself --shell /usr/sbin/nologin photoself 2>/dev/null || true
sudo install -m 0755 target/release/photoself /usr/local/bin/photoself
sudo install -d -o photoself -g photoself /var/lib/photoself
sudo install -d /etc/photoself
sudo install -m 0600 config.toml /etc/photoself/config.toml
sudo install -m 0644 deploy/systemd/photoself.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now photoself
```

See [`../deploy/systemd/photoself.service`](../deploy/systemd/photoself.service).

---

## Reverse proxy

- TLS and authentication can terminate at Nginx/Caddy/lighttpd.
- For `media.serve_mode = "proxy"`, map
  `/media/<library-id>/<relative-path>` to the library root and
  `/thumbs/...` to `data/cache/thumbs/...`.
- Keep the Rust API bound to `127.0.0.1` unless you intentionally expose it.
- Do not expose photoSelf directly to the internet without auth and TLS.

---

## HEIC / HEVC on low-power devices

Many OpenWrt ffmpeg builds lack software HEVC/HEIF decoders. If your library
contains HEIC (sometimes named `.JPG`) or HEVC MOV, generate sidecars on a
PC/NAS:

```bash
./scripts/make-sidecars.py \
  --source /mnt/router/picture \
  --output /mnt/router/usb/photo-app/sidecars/family \
  --sizes 320,1280 --video-sizes 320 --jobs 2
```

```toml
[thumbnails]
sidecar_dir = "/mnt/usb/photo-app/sidecars"
```

Sidecars live at `<sidecar_dir>/<library-id>/`; see
[`usage.md`](usage.md) and the script docstring for details.

---

## Upgrade and backup

- The SQLite database is a cache + user metadata. Back up `data.dir`.
- Thumbnails/cache are disposable; sidecars can be regenerated.
- Keep the previous binary for rollback.
- Photo libraries are never modified by scanning or viewing.

## Troubleshooting

| Symptom | Check |
|---|---|
| 401 on API | `auth.mode`, credentials, `password_hash` |
| UI loads but no items | library path, permissions, scan job status |
| HEIC thumbnails 500 | generate sidecars or use an ffmpeg with HEIF |
| Video poster 500 | ffmpeg decoder availability for the codec |
| Service exits on boot | USB mount not ready; `data.dir` exists |
| `Address already in use` | change `server.listen` |
