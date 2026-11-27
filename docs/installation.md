# Installation

[简体中文](installation.zh-CN.md) | English

photoSelf runs as a single Rust binary with an optional embedded Vue UI. It reads
existing folders; it does not import or move your photos.

> Prebuilt Linux binaries (x86_64, aarch64, armv7) and OpenWrt `.ipk`
> packages are published under [Releases](https://github.com/shinvdu/photoSelf/releases).
> Build from source for other architectures.

### Prebuilt release assets

| Asset | Use |
|---|---|
| `photoself-<version>-x86_64-unknown-linux-musl.tar.gz` | statically linked Linux x86_64 binary |
| `photoself-<version>-aarch64-unknown-linux-musl.tar.gz` | ARM64 / aarch64 Linux |
| `photoself-<version>-armv7-unknown-linux-musleabihf.tar.gz` | ARMv7 hard-float |
| `photoself_<version>_aarch64_cortex-a53.ipk` | OpenWrt 23.05/24.10 opkg, common ARM64 targets |
| `photoself_<version>_arm_cortex-a7_neon-vfpv4.ipk` | OpenWrt 23.05/24.10 opkg, common ARMv7 targets |
| `photoself_<version>_x86_64.ipk` | OpenWrt x86_64 |
| `photoself-<version>-openwrt-aarch64.tar.gz` | OpenWrt apk-based systems / manual install |
| `photoself-<version>-openwrt-armv7.tar.gz` | OpenWrt apk-based systems / manual install |
| `photoself-<version>-openwrt-x86_64.tar.gz` | OpenWrt x86_64 apk-based / manual install |

The `.ipk` installs `/usr/bin/photoself`, `/etc/init.d/photoself`,
`/etc/photoself/config.toml.example` and, on first install, creates
`/etc/photoself/config.toml`. **Edit the config before starting** (paths are
placeholders; default auth is `none` for a trusted LAN). Upgrades preserve an
existing config.

```sh
opkg install photoself_<version>_aarch64_cortex-a53.ipk
vi /etc/photoself/config.toml
/etc/init.d/photoself enable
/etc/init.d/photoself start
```

For apk-based OpenWrt (25.12+), extract the `openwrt-*.tar.gz` into `/` (or
build from source) and edit the config the same way. For architectures not
listed, use the raw binary tarball or build from source.

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
The application only accepts that header from peers listed in
`server.trusted_proxies` (CIDR or bare IP); the default is loopback only.


### Write mode, trash and video derivatives

Write operations are disabled by default:

```toml
[write]
enabled = true
max_upload_bytes = 536870912

[[libraries]]
id = "photos"
path = "/photos"
read_only = false     # required for upload/delete

[trash]
enabled = true
retention_days = 30
```

- `POST /api/v1/upload` accepts multipart `library` + `file`.
- `DELETE /api/v1/items/{id}` moves the file to `<data.dir>/trash/<library-id>/`
  and marks the item missing; restore/purge via `/api/v1/trash`.
- Enabling write mode means the API (and therefore authentication) can modify
  the library. Keep it behind auth and never expose it publicly.
- Video web versions: set `video.transcode_enabled = true` and install an
  ffmpeg with a H.264 encoder. The UI/API queues a job and exposes `play_url`.
- Scheduled scans: `scan.interval = "15m"` (supports `s`, `m`, `h`, `d`).
- The UI is a PWA; install it from a browser over HTTPS (or localhost).

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
