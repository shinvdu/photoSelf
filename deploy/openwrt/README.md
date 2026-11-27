# photoSelf on OpenWrt

Tested design target: aarch64 / musl, 512 MB RAM, USB storage for data.

## Build

SQLite is compiled with the `bundled` feature, so the musl build needs a C
cross compiler. `cargo-zigbuild` is the easiest option:

```bash
rustup target add aarch64-unknown-linux-musl
cargo install cargo-zigbuild --locked

cd ui && npm ci && npm run build && cd ..
cargo zigbuild --release --target aarch64-unknown-linux-musl \
  -p photoself-server --features embed-ui
```

Alternatively, build with the OpenWrt SDK toolchain and point `CC` /
`CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER` at
`aarch64-openwrt-linux-musl-gcc`.

## Install

```sh
opkg install ffmpeg ffprobe        # optional but recommended for video posters
scp target/aarch64-unknown-linux-musl/release/photoself root@router:/usr/bin/
scp config.example.toml root@router:/etc/photoself/config.toml
scp deploy/openwrt/photoself.init root@router:/etc/init.d/photoself
ssh root@router 'chmod +x /etc/init.d/photoself && /etc/init.d/photoself enable && /etc/init.d/photoself start'
```

Keep the binary and config on NAND and point `data.dir` at USB storage:

```toml
[data]
dir = "/mnt/usb/photo-app/data"

[thumbnails]
concurrency = 1
sizes = [320, 1280]

[ffmpeg]
concurrency = 1

[video]
transcode_enabled = false
```

## Reverse proxy

Use [`../lighttpd/94-photoself.conf`](../lighttpd/94-photoself.conf) as a starting
point. It serves the SPA at `/photo/`, proxies `/api/` to `127.0.0.1:8787`, and
maps `/photo/media/<library-id>/` and `/photo/thumbs/` to disk.

Set:

```toml
[server]
mode = "reverse-proxy"
listen = "127.0.0.1:8787"

[media]
serve_mode = "proxy"
public_media_path = "/photo/media"
public_thumb_path = "/photo/thumbs"

[auth]
mode = "proxy"     # lighttpd Basic Auth sets the identity; or use basic/none carefully
```

## HEIC / HEVC previews

The OpenWrt ffmpeg package on 23.05 can demux MP4/MOV but usually has only
`hevc_v4l2m2m` (hardware) and **no software HEVC/HEIF decoder**. iPhones also
often store HEIC files with `.JPG` names. On this class of device, generate
previews on a PC/NAS and point photoSelf at them:

```bash
# run on a PC; source may be sshfs/NFS/SMB
./scripts/make-sidecars.py \
  --source /mnt/router/picture \
  --output /mnt/router/usb/photo-app/sidecars/family \
  --sizes 320,1280 --video-sizes 320 --jobs 2
```

```toml
[thumbnails]
sidecar_dir = "/mnt/usb/photo-app/sidecars"
```

The output directory is `<sidecar_dir>/<library-id>/`. photoSelf serves
`<relative-path>.<size>.jpg` from it before
attempting to decode the original. `.meta.json` sidecars fill width/height/
duration/date when ffprobe cannot decode the codec. Re-run the script after
importing new HEIC/HEVC media.

If the router's USB disk is unreliable, keep `data.dir` and `sidecar_dir` on a
healthy disk and mount the photo library read-only.

## Memory notes

- `thumbnails.concurrency = 1` and `ffmpeg.concurrency = 1` on 512 MB.
- Prefer `[320]` thumbnails if the UI is only used on phones.
- Keep caches and SQLite on USB; avoid writing thumbnails to NAND.
- Do not expose the router directly to the internet; use LAN or WireGuard.
