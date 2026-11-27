# Usage Guide

[简体中文](usage.zh-CN.md) | English

## First run

1. Start the server:

   ```bash
   photoself serve -c config.toml
   ```

2. Open `http://127.0.0.1:8787/`.
3. Press **Scan** (or run `photoself scan -c config.toml` from the CLI).
4. The timeline is grouped by day. Use search, favorites, and the lightbox.

If the optional manager is enabled (see [Libraries](#libraries)), directories can
be added from the **Libraries** tab without editing the config or restarting.

The scanner only reads files. It never moves, renames or deletes photos.

## UI overview

| Area | What it does |
|---|---|
| Timeline | Day-grouped grid, lazy thumbnails, infinite scroll |
| Albums | Create albums, add/remove items, cover view |
| Trash | Restore or permanently delete removed items |
| Libraries | (v0.5+, if `manage.enabled`) add/disable/remove libraries, per-library scan, server-side directory picker |
| Search / tag filter | Filename and tag filters |
| Favorites | Filter items marked as favorite |
| Scan | Starts a background scan job for all enabled libraries; progress is shown |
| Lightbox | Image preview with zoom/pan, video playback, metadata, tags, albums, download |
| Language | 中文 / English switcher; Chinese is the default and the choice is remembered |

### Lightbox and mobile layout

- **Zoom:** mouse wheel, pinch, double-click/double-tap, or the `−` / `+`
  controls (the percentage button resets). Range is 1×–5×.
- **Pan:** drag while zoomed. Navigation arrows and keyboard arrows change
  items; zoom resets when the item changes.
- **Keyboard:** `+` / `-` zoom, `0` resets, `←` / `→` previous/next,
  `Esc` closes.
- Above roughly **125%** the original file is layered over the preview so
  zoomed detail is sharp. For images the lightbox normally uses a **1280 px
  JPEG preview** (`preview_url`), which keeps HEIC files viewable in browsers
  that cannot decode HEIC. The download link always points at the original.
- **Portrait phones:** metadata and tags move into a collapsible bottom sheet
  (collapsed by default, tap the filename bar to expand). Swipe left/right to
  move between photos; the sheet does not block scrolling its own content.
- Desktop keeps the metadata panel on the right; zoom is clipped to the viewer
  so it never covers the panel or the arrows.

## Libraries

Each library has an `id`, a path and ignore rules:

```toml
[[libraries]]
id = "family"
name = "Family photos"
path = "/photos"
read_only = true
ignore_globs = ["._*", ".DS_Store", "@eaDir", "#recycle"]
```

Multiple libraries are allowed. Sidecars are scoped per library:

```text
<sidecar_dir>/<library-id>/<relative-path>.<size>.jpg
<sidecar_dir>/<library-id>/<relative-path>.meta.json
```

This avoids collisions when two libraries contain a file with the same name.

### Managing libraries from the UI (v0.5+)

The manager is **off by default**. After the first import, libraries are stored
in SQLite and can be added, disabled or removed from the **Libraries** tab.
Enable it once in the config and restart:

```toml
[manage]
enabled = true
allowed_roots = ["/mnt/sda1", "/mnt/photos"]
require_auth = true
```

Workflow:

1. Open the **Libraries** tab.
2. Press **Add library**, then either type an absolute path or press
   **Browse** and navigate the server-side directory picker.
3. Optionally set a display name and keep **Read-only** checked.
4. Confirm with **Add and scan**. The library appears immediately and a scan
   job starts; progress is shown in the status bar.

Per card you can **Scan**, **Enable/Disable**, or **Remove**. `config.toml` is
no longer edited by the UI.

- The UI can only add absolute paths inside `allowed_roots`; the directory
  picker cannot browse outside them.
- The server rejects paths that overlap `data.dir`, the thumbnail cache,
  sidecar/trash directories, or another library (nested libraries would index
  files twice). Symlinks are resolved and must stay inside the allow-list.
- **Remove** deletes index rows, tags and album links. Source files and
  pre-generated sidecars are never touched. Trash index rows for that library
  are dropped, but trash files stay on disk.
- Disabling a library keeps its index but excludes it from scans and the
  timeline. Disabling an index-backed library persists across restarts.
- `config.toml` is only used to seed new ids and to upgrade pre-v0.5 rows.
  **Re-import from config** forcibly reapplies config values (name, path,
  enabled state, ignore globs) to matching ids and clears removed tombstones;
  libraries added in the UI are untouched.
- With `auth.mode = "none"`, management is refused by default. Set
  `require_auth = false` explicitly if you really want an unauthenticated LAN
  manager.

## Scanning

- **Manual:** press **Scan** in the header to scan every enabled library, or
  **Scan** on a card in the **Libraries** tab to scan just that one. The API is
  `POST /api/v1/scan` (all) or `POST /api/v1/libraries/{id}/scan` (one).
- **CLI:** `photoself scan -c config.toml` (optionally `--library family`).
  The CLI imports config libraries into the database first, then scans the
  database registry, so it sees the same libraries as the UI.
- **On start:** `scan.on_start = true` (default in the example config).
- **Incremental:** unchanged size+mtime reuses the cached hash; scans on slow
  USB/NAND devices stay cheap.
- **Missing files:** rows are marked missing and excluded from the API. They are
  not removed from the database.

A scan job reports progress and counts:

```json
{
  "state": "completed",
  "progress": 100,
  "message": "scanned 12345, errors 0, missing 0"
}
```

## Favorites and search

```bash
# All POST/PUT/PATCH/DELETE requests need X-Requested-With: photoSelf
# favorite
curl -u admin:password -X PATCH \
  -H 'X-Requested-With: photoSelf' \
  -H 'Content-Type: application/json' \
  -d '{"favorite":true}' \
  http://127.0.0.1:8787/api/v1/items/<id>

# search
curl -u admin:password \
  'http://127.0.0.1:8787/api/v1/items?q=IMG_2023&limit=50'
```

## API

All endpoints are under `/api/v1`. Authentication follows `auth.mode`.

```bash
PASS='your-password'

curl -u "admin:$PASS" http://127.0.0.1:8787/api/v1/info
curl -u "admin:$PASS" 'http://127.0.0.1:8787/api/v1/items?limit=100'
curl -u "admin:$PASS" -H 'X-Requested-With: photoSelf' -X POST http://127.0.0.1:8787/api/v1/scan
curl -u "admin:$PASS" http://127.0.0.1:8787/api/v1/jobs/<job-id>
```

Cursor pagination: pass the returned `next_cursor` as `cursor`. Filters:
`library`, `kind` (`image`/`video`), `q`, `favorite`, `from`, `to`, `limit`.

`GET /api/v1/info` reports `manage_enabled` so clients can hide the manager.

### Library management API (v0.5+)

Requires `[manage] enabled = true`; writes also need `X-Requested-With:
photoSelf`. With `auth.mode = "none"` and `require_auth = true` every endpoint
below returns `403`.

```bash
AUTH='-u admin:password'
J='-H Content-Type:application/json -H X-Requested-With:photoSelf'
B=http://127.0.0.1:8787/api/v1

# list libraries (path, source, enabled, item_count, ...)
curl $AUTH "$B/libraries"

# add a library; triggers a scan job
curl $AUTH $J -X POST -d '{"path":"/mnt/sda1/photos","name":"Photos","read_only":true}' \
  "$B/libraries"

# rename / disable / enable / change ignore globs
curl $AUTH $J -X PATCH -d '{"name":"Family","enabled":false}' "$B/libraries/<id>"

# scan one library
curl $AUTH $J -X POST "$B/libraries/<id>/scan"

# explicitly re-apply config.toml values to matching ids
curl $AUTH $J -X POST "$B/libraries/import-config"

# remove index only (source files stay)
curl $AUTH $J -X DELETE "$B/libraries/<id>"

# server-side directory browser (confined to manage.allowed_roots)
curl $AUTH "$B/fs/dirs?path=/mnt/sda1"
curl $AUTH "$B/fs/dirs"        # list the allowed roots
```

`POST /api/v1/libraries` accepts `path` (required), `name`, `read_only`
(default `true`) and `ignore_globs` (default `["._*", ".DS_Store", "@eaDir",
"#recycle"]`). Paths are canonicalized; `data.dir`, thumbnail/sidecar/trash
trees and overlapping libraries are rejected.

The media endpoint supports HTTP Range requests:

```bash
curl -u "admin:$PASS" -H 'Range: bytes=0-99' \
  http://127.0.0.1:8787/api/v1/items/<id>/media
# 206 Partial Content
```

## Sidecar previews

For HEIC/HEVC media on devices without software decoders, generate previews on a
PC/NAS:

```bash
./scripts/make-sidecars.py \
  --source /mnt/router/picture \
  --output /mnt/router/usb/photo-app/sidecars/family \
  --sizes 320,1280 --video-sizes 320 --jobs 2
```

Then set:

```toml
[thumbnails]
sidecar_dir = "/mnt/usb/photo-app/sidecars"
```

- The server serves a sidecar before attempting to decode the original.
- `.meta.json` fills width/height/duration/date when ffprobe cannot decode.
- Re-run after importing new HEIC/HEVC files. `--meta-only` refreshes metadata
  only; existing previews are skipped unless `--force`.

## Tags, albums, upload and trash

- **Tags:** open an item, type/select a tag in the lightbox, then use the tag
  filter in the header. `POST /api/v1/tags` creates tags; attach with
  `POST /api/v1/items/{id}/tags`.
- **Albums:** switch to **Albums**, create one, then add the current item from
  the lightbox. Album membership is virtual; no files are moved.
- **Upload:** with `[write] enabled = true` and a writable library, press
  **Upload** and select image/video files. The server writes a temporary file
  and indexes it immediately.
- **Delete/trash:** the lightbox **Delete** button moves the file into
  `<data.dir>/trash/<library-id>/` and marks the row missing. Open **Trash** to
  restore or permanently delete.
- **Search filters:** filename query, favorites, and tag filter apply to the
  timeline. `kind`, `library`, `from`, `to`, `album` are available on the API.

## Web video versions

Set `video.transcode_enabled = true` and make sure `ffmpeg` has `libx264` and
`aac`. In the lightbox, use **Generate web version**. The server queues a job
under `data/cache/derived/` and exposes `play_url` (an H.264/AAC MP4).

## Scheduled scans and PWA

- `scan.interval = "15m"` runs a background scan every 15 minutes. Supported
  suffixes: `s`, `m`, `h`, `d`; `"0"` disables.
- The UI ships a PWA manifest and service worker. Install it from Chrome/Edge
  (HTTPS or localhost) to get a standalone window and an offline app shell.

## Authentication

`auth.mode = "none"` is for a trusted LAN only. For a single admin:

```bash
photoself hash-password --password 'choose-a-password'
```

```toml
[auth]
mode = "basic"
username = "admin"
password_hash = "$argon2id$..."
```

Behind a trusted reverse proxy:

```toml
[auth]
mode = "proxy"
```

The proxy must authenticate and set `X-Forwarded-User`. Only peers in
`server.trusted_proxies` (CIDR/bare IP) may send it; never trust this header from
the internet directly.

## Data, backup and restore

| Path | Contents |
|---|---|
| `data.dir/photoself.db` | SQLite index, user metadata and the runtime library registry |
| `data.dir/cache/thumbs/` | generated thumbnails (disposable) |
| `data.dir/cache/tmp/` | temporary files |
| `thumbnails.sidecar_dir` | pre-generated previews/metadata (optional) |

- Back up `data.dir` if favorites, tags, albums or the UI-managed library list
  matter.
- Deleting the database is safe: a scan rebuilds the index; favorites/tags and
  UI-added libraries are lost (config `[[libraries]]` are re-imported on the
  next start).
- Photo roots are never modified, including when a library is removed in the
  UI.
- Sidecars can be regenerated at any time.

## Performance tips for 512 MB devices

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

- Prefer sidecars over on-device HEVC/HEIF decoding.
- Keep caches and SQLite on a writable USB disk, not NAND.
- Keep media serving at the reverse proxy; avoid funneling video through Rust.
- Expect ~5 MB idle RSS; scanning peaks around 20–30 MB plus external ffmpeg.

## Known behavior and limitations

- Write mode is off by default; enabling it lets the API modify the library.
- UI library management is off by default and requires an explicit
  `[manage].allowed_roots` allow-list plus authentication.
- Removing a library deletes its index, tags and album links permanently; there
  is no undo. The source directory is left untouched.
- Trash retention is manual; purge old entries yourself.
- Video playback uses the original unless a web MP4 is generated; MOV/HEVC
  compatibility depends on the browser (Safari/Apple often works; Chrome/Firefox may not).
- HEIC files can be indexed and previewed through sidecars; the original remains
  HEIC.
- EXIF dates without timezone are stored as naive local timestamps.
- `search` is a filename `LIKE` query, not full-text search.
- The UI shows all libraries in one timeline; per-library filtering is planned.
- Tags/albums are metadata only; no automatic file organization.
- The UI language defaults to Simplified Chinese; switching to English is
  remembered per browser (`localStorage`). Server error text remains English.
- Library rename is available through `PATCH /api/v1/libraries/{id}`; the UI
  currently exposes enable/disable, scan and remove.
