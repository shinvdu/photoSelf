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

The scanner only reads files. It never moves, renames or deletes photos.

## UI overview

| Area | What it does |
|---|---|
| Timeline | Day-grouped grid, lazy thumbnails, infinite scroll |
| Albums | Create albums, add/remove items, cover view |
| Trash | Restore or permanently delete removed items |
| Search / tag filter | Filename and tag filters |
| Favorites | Filter items marked as favorite |
| Scan | Starts a background scan job; progress is shown |
| Lightbox | Image preview (large JPEG thumbnail), video playback, metadata, download |
| ♥/★ | Toggle favorite on an item |

For images, the lightbox uses a **1280 px JPEG preview** (`preview_url`) instead
of the original. This keeps HEIC files viewable in browsers that cannot decode
HEIC. The download link always points at the original file.

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

## Scanning

- **Manual:** press **Scan** in the UI or `POST /api/v1/scan`.
- **CLI:** `photoself scan -c config.toml` (optionally `--library family`).
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
| `data.dir/photoself.db` | SQLite index + user metadata |
| `data.dir/cache/thumbs/` | generated thumbnails (disposable) |
| `data.dir/cache/tmp/` | temporary files |
| `thumbnails.sidecar_dir` | pre-generated previews/metadata (optional) |

- Back up `data.dir` if favorites/metadata matter.
- Deleting the database is safe: a scan rebuilds the index; favorites are lost.
- Photo roots are never modified.
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
- Trash retention is manual; purge old entries yourself.
- Video playback uses the original unless a web MP4 is generated; MOV/HEVC
  compatibility depends on the browser (Safari/Apple often works; Chrome/Firefox may not).
- HEIC files can be indexed and previewed through sidecars; the original remains
  HEIC.
- EXIF dates without timezone are stored as naive local timestamps.
- `search` is a filename `LIKE` query, not full-text search.
- The UI shows all libraries in one timeline; per-library filtering is planned.
- Tags/albums are metadata only; no automatic file organization.
