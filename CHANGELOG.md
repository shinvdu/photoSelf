# Changelog

## 0.7.2 — 2026-11-27

- Albums: **New album** is now a button that opens a modal instead of an
  always-visible inline name field.
- Album delete button is hidden by default and fades in on hover (always
  visible on touch devices); the hidden button no longer captures clicks.
- Deleting an album now requires a **second confirmation**; the second prompt
  makes clear that it cannot be undone and that the photos are not deleted.
- Uploading while inside an album automatically adds the uploaded photos to
  that album, with a toast.
- The global item count is only shown on the timeline; album/trash/library
  views no longer display it. Transient upload text clears when done or when
  switching views.
- Fix: the landscape lightbox close button was covered by the full-bleed
  viewer and could not be clicked.
- Fix: album item counts and cover selection now ignore missing/deleted items.
- The version number moved from the top bar to the page footer.

## 0.7.1 — 2026-11-27

- Uploading files now shows a toast on completion ("Uploaded N files") and a
  red toast for partial failures; previously only the status bar updated.
- Landscape/desktop lightbox is full-bleed: the photo fills the window with a
  5 px margin, the metadata panel floats over the right edge with an ⓘ toggle,
  and the next arrow moves left of the panel so it is never covered.

## 0.7.0 — 2026-11-27

- Thumbnail context menu on right-click (or touch long-press): view, download,
  add to album, delete. Right-clicking an unselected tile selects it first;
  right-clicking inside a multi-selection keeps it.
- Multi-select: toolbar **Select** mode, `Ctrl`/`Cmd`-click to toggle and
  `Shift`-click for a range; a bottom selection bar offers batch view,
  download, add to album, delete and clear.
- Batch delete is enabled only when every selected item is in a writable
  library and `[write]`/`[trash]` are enabled; items are deleted one by one and
  partial failures are reported. Batch album adds are metadata-only and work for
  read-only libraries too.
- Selection is scoped to the current timeline/album view and cleared when the
  view or search changes.
- Added `docs/local-development.md` with a reproducible local backend + Vite
  development environment.

## 0.6.0 — 2026-11-27

- Redesigned UI with a softer rose/lavender palette, rounded cards, pill tabs
  and subtle gradients.
- Light and dark themes. The first visit follows the OS preference; the header
  toggle persists the choice in `localStorage` and updates the PWA theme color.
- Portrait layouts are now selected by **orientation as well as width**, so
  wide portrait windows also get the mobile layout: metadata, tags and actions
  live in a collapsible bottom sheet (collapsed by default, tap the filename
  bar to expand). Rotation switches layouts automatically.
- Mobile/portrait topbar reworked into a grid: brand + tools, tabs, search,
  actions, instead of cramped wrapping.

## 0.5.1 — 2026-11-27

- Album actions in the lightbox now show a toast for success and failures;
  previously the confirmation was written to the status bar, which is hidden
  behind the lightbox overlay.
- Fix: removing a non-cover album item no longer re-picks the album cover. The
  cover is only recomputed when the cover item itself is removed.

## 0.5.0 — 2026-11-27

- Library management in the UI behind an optional `[manage]` section: list,
  add (server-side directory picker or manual path), rename, enable/disable,
  per-library scan and remove.
- Libraries now live in SQLite. `config.toml` seeds new ids and upgrades
  pre-0.5 rows once; removed libraries are tombstoned so a restart cannot
  resurrect them. "Re-import from config" forces config values explicitly.
- Path safety: absolute paths only, `manage.allowed_roots` allow-list, refusal
  of `data.dir` / thumbnail / sidecar / trash trees, duplicate and nested
  library rejection, and 5 s filesystem timeouts so stale mounts cannot wedge
  the API. Management is refused with `auth.mode = "none"` unless
  `require_auth = false` is set explicitly.
- New endpoints: `GET/POST /api/v1/libraries`, `PATCH/DELETE
  /api/v1/libraries/{id}`, `POST /api/v1/libraries/{id}/scan`,
  `POST /api/v1/libraries/import-config`, `GET /api/v1/fs/dirs`.
- `[[libraries]]` in config is no longer required to be non-empty; the DB is
  authoritative after the first import.
- Removing a library deletes its index, tags and album links only; source files
  and pre-generated sidecars are never touched. Trash index rows for that
  library are dropped, but trash files stay on disk.

## 0.4.0 — 2026-11-27

- Lightbox zoom: mouse wheel, pinch, double-tap/double-click, on-screen
  controls and the `+`/`-`/`0` keys. Drag to pan while zoomed; the original
  file is layered in above ~125% so zoomed detail stays sharp.
- Portrait phones: metadata now lives in a collapsible bottom sheet that is
  collapsed by default, leaving the photo full-height. Swipe left/right to
  move between photos; the sheet shows the same tags/albums/actions.
- Desktop keeps the side metadata panel; zoom is clipped to the viewer so it
  never covers the panel or navigation arrows.

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
