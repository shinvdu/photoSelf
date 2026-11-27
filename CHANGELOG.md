# Changelog

## 0.3.0 — 2026-11-27

- Multilingual UI: Simplified Chinese and English, Chinese by default.
- Language choice is remembered in `localStorage`; dates follow the active
  locale and English uses proper singular/plural forms.
- Fixed a duplicate `lang` key in the PWA manifest.

## 0.2.3 — 2026-11-27

Security and data-safety fixes from the code review:

- Upload overwrite uses an atomic rename instead of deleting the old file first.
- Trash delete records the DB entry before moving the file and removes it on
  move failure; restore is transactional and rolls back the copied file on DB
  failure.
- `auth.mode = "proxy"` now only accepts `X-Forwarded-User` from peers in
  `server.trusted_proxies` (CIDR/bare IP).
- Upload rejects parent directories that resolve outside the library root.
- Sidecar temp files are unique per process/request.
- All POST/PUT/PATCH/DELETE API calls require `X-Requested-With: photoSelf`.
- Trash list no longer exposes the absolute `trash_path`.
- Album/tag endpoints return 404 for missing items/tags/albums.
- `video.max_height` is now honored during transcode.
- Added server integration tests for upload/trash/restore/CSRF, tags/albums,
  transcode queue, interval parsing and trusted proxy matching.

## 0.2.2 — 2026-11-27

- OpenWrt `postinst`: restore `config.toml-opkg.backup` when upgrading from
  packages that shipped the config as a conffile, so an upgrade cannot reset a
  customized configuration to the example.

## 0.2.1 — 2026-11-27

- OpenWrt package: ship `config.toml.example` and create `config.toml` only on
  first install, so upgrades no longer overwrite a customized configuration.

## 0.2.0 — 2026-11-27

- Tags: create/delete, attach/detach, tag filter on the timeline and API.
- Albums: create/delete, add/remove items, cover image, album view.
- Write mode: multipart upload, safe delete to a trash directory, restore and
  permanent purge. Disabled by default; libraries must be writable.
- Search filters: filename, favorites, tag, album, kind, library and date range.
- Video derivatives: background web-MP4 (H.264/AAC) queue with `play_url` and a
  Range-capable `/derived` endpoint.
- Scheduled scans via `scan.interval` (`s`/`m`/`h`/`d`).
- PWA manifest and service worker; installable app shell.
- UI: Albums and Trash views, upload button, tag editing, web-video action.

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
