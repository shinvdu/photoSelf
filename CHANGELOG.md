# Changelog

## 0.1.0 — 2026-11-26

Initial MVP.

- Rust core: config, SQLite index, recursive scanner, EXIF/video metadata,
  BLAKE3 hashes, rename detection, image/video thumbnails.
- Axum server: timeline API, item detail, favorites, scan jobs, thumbnail and
  Range-capable media endpoints, health check.
- Auth modes: none, HTTP Basic (Argon2id), trusted reverse proxy.
- Vue 3 UI: timeline grid, lazy thumbnails, lightbox, video playback, search,
  favorites, scan progress, responsive dark theme.
- Deployment: Dockerfile, systemd unit, OpenWrt procd script, Nginx and lighttpd
  examples, low-memory configuration.
- Optional `embed-ui` feature for a single binary with the SPA embedded.
- Sidecar previews/metadata (`thumbnails.sidecar_dir`) for HEIC/HEVC media on
  devices without software decoders; generator in `scripts/make-sidecars.py`; sidecars are scoped per library id.
- Reuse cached hashes for unchanged files to make rescans cheap on USB/NAND.
- HEIC/HEIF/AVIF extensions are indexed; sidecar previews/metadata cover them.
