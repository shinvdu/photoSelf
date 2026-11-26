#!/usr/bin/env python3
"""Generate pre-built previews and metadata for photoSelf.

The built-in thumbnailer uses the Rust `image` crate for ordinary images and
ffmpeg for videos. Some devices (notably OpenWrt ffmpeg builds) lack software
HEVC and HEIF decoders, and HEIC files are sometimes named *.JPG. This script
runs on a PC/NAS instead and writes sidecars that photoSelf serves verbatim:

    <output>/<relative-path>.<size>.jpg    # previews
    <output>/<relative-path>.meta.json     # width/height/duration/date

<output> should be `<sidecar_dir>/<library-id>`; photoSelf looks previews up
under that per-library directory.

Requirements:
  - Python 3.9+ and Pillow (images)
  - ffmpeg/ffprobe (videos; and images if Pillow cannot decode them)
  - heif-convert from libheif-examples (HEIC/HEIF)

Example:
  ./scripts/make-sidecars.py \
      --source /mnt/router/picture \
      --output /mnt/router/drive/photo-app/sidecars \
      --sizes 320,1280 --video-sizes 320 --jobs 2
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
from pathlib import Path

from PIL import Image, ImageOps

IMAGE_EXTS = {
    ".jpg", ".jpeg", ".png", ".webp", ".gif", ".bmp",
    ".tif", ".tiff", ".heic", ".heif", ".avif",
}
VIDEO_EXTS = {".mp4", ".m4v", ".mov", ".webm", ".ogv", ".mkv", ".avi"}
HEIC_BRANDS = {b"heic", b"heix", b"hevc", b"hevx", b"mif1", b"msf1"}


def is_heic(path: Path) -> bool:
    try:
        with path.open("rb") as handle:
            head = handle.read(12)
    except OSError:
        return False
    return len(head) >= 12 and head[4:8] == b"ftyp" and head[8:12] in HEIC_BRANDS


def run(cmd: list[str]) -> None:
    result = subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    if result.returncode != 0:
        raise RuntimeError(
            f"{cmd[0]} failed ({result.returncode}): "
            + result.stderr.decode("utf-8", "replace").strip()
        )


def exif_date(image: Image.Image) -> str | None:
    try:
        value = image.getexif().get(36867)  # DateTimeOriginal
    except Exception:  # noqa: BLE001 - malformed EXIF should not fail indexing
        return None
    if not value:
        return None
    try:
        parsed = datetime.strptime(str(value)[:19], "%Y:%m:%d %H:%M:%S")
        return parsed.isoformat()
    except ValueError:
        return None


def image_meta(base: Path) -> dict:
    with Image.open(base) as image:
        image = ImageOps.exif_transpose(image)
        taken_at = exif_date(image)
        return {
            "width": image.width,
            "height": image.height,
            "duration_ms": None,
            "taken_at": taken_at,
        }


def video_meta(source: Path) -> dict:
    result = subprocess.run(
        [
            "ffprobe", "-v", "error", "-print_format", "json",
            "-show_format", "-show_streams", str(source),
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        raise RuntimeError(
            result.stderr.decode("utf-8", "replace").strip() or "ffprobe failed"
        )
    data = json.loads(result.stdout)
    video = next(
        (stream for stream in data.get("streams", [])
         if stream.get("codec_type") == "video"),
        {},
    )
    duration = data.get("format", {}).get("duration")
    created = (
        data.get("format", {}).get("tags", {}).get("creation_time")
        or video.get("tags", {}).get("creation_time")
    )
    taken_at = None
    if created:
        try:
            taken_at = (
                datetime.fromisoformat(created.replace("Z", "+00:00"))
                .astimezone(timezone.utc)
                .replace(tzinfo=None)
                .isoformat()
            )
        except ValueError:
            taken_at = None
    return {
        "width": video.get("width"),
        "height": video.get("height"),
        "duration_ms": int(float(duration) * 1000) if duration else None,
        "taken_at": taken_at,
    }


def write_json(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + ".tmp")
    try:
        tmp.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")
        os.replace(tmp, path)
    finally:
        tmp.unlink(missing_ok=True)


def save_image(source: Path, destination: Path, size: int) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    tmp = destination.with_name(destination.name + ".tmp")
    try:
        with Image.open(source) as image:
            image = ImageOps.exif_transpose(image)
            image.thumbnail((size, size), Image.Resampling.LANCZOS)
            if image.mode not in ("RGB", "L"):
                image = image.convert("RGB")
            image.save(tmp, "JPEG", quality=82, optimize=True)
        os.replace(tmp, destination)
    finally:
        tmp.unlink(missing_ok=True)


def video_poster(source: Path, destination: Path, size: int) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    tmp = destination.with_name(destination.name + ".tmp.jpg")
    base = [
        "ffmpeg", "-v", "error", "-y", "-threads", "2", "-ss", "1",
        "-i", str(source), "-an", "-sn", "-dn",
        "-frames:v", "1", "-vf", f"scale='min({size},iw)':-2",
        "-q:v", "4", str(tmp),
    ]
    try:
        run(base)
    except RuntimeError:
        try:
            run([
                "ffmpeg", "-v", "error", "-y", "-threads", "2", "-ss", "0",
                "-i", str(source), "-an", "-sn", "-dn",
                "-frames:v", "1", "-vf", f"scale='min({size},iw)':-2",
                "-q:v", "4", str(tmp),
            ])
        except RuntimeError:
            pass
    if not tmp.exists():
        raise RuntimeError("ffmpeg produced no poster")
    os.replace(tmp, destination)


def process_one(
    source_root: Path,
    output_root: Path,
    path: Path,
    sizes: list[int],
    video_sizes: list[int],
    force: bool,
    meta_only: bool,
) -> tuple[str, str | None]:
    rel = path.relative_to(source_root)
    suffix = path.suffix.lower()
    is_video = suffix in VIDEO_EXTS
    if not is_video and suffix not in IMAGE_EXTS:
        return "skip", None

    targets = video_sizes if is_video else sizes
    meta_path = output_root / f"{rel}.meta.json"
    meta_needed = (
        force
        or not meta_path.exists()
        or meta_path.stat().st_mtime < path.stat().st_mtime
    )
    thumbs_missing = []
    if not meta_only:
        for size in targets:
            out = output_root / f"{rel}.{size}.jpg"
            if force or not out.exists() or out.stat().st_mtime < path.stat().st_mtime:
                thumbs_missing.append((size, out))

    if not meta_needed and not thumbs_missing:
        return "skip", None

    temp_dir = None
    try:
        base = path
        if not is_video and is_heic(path) and (meta_needed or thumbs_missing):
            temp_dir = tempfile.mkdtemp(prefix="photoself-heic-")
            base = Path(temp_dir) / "full.jpg"
            run(["heif-convert", "-q", "85", str(path), str(base)])

        if meta_needed:
            write_json(meta_path, video_meta(path) if is_video else image_meta(base))

        for size, out in thumbs_missing:
            if is_video:
                video_poster(path, out, size)
            else:
                save_image(base, out, size)
        return "done", None
    except Exception as error:  # noqa: BLE001 - report per-file failures
        return "failed", f"{rel}: {error}"
    finally:
        if temp_dir:
            shutil.rmtree(temp_dir, ignore_errors=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--sizes", default="320,1280", help="image sizes, comma separated")
    parser.add_argument("--video-sizes", default="320", help="video poster sizes")
    parser.add_argument("--jobs", type=int, default=2)
    parser.add_argument("--limit", type=int, default=0)
    parser.add_argument("--force", action="store_true")
    parser.add_argument(
        "--meta-only",
        action="store_true",
        help="only write .meta.json files; do not generate JPEG previews",
    )
    args = parser.parse_args()

    source_root = args.source.resolve()
    output_root = args.output.resolve()
    sizes = [int(value) for value in args.sizes.split(",") if value.strip()]
    video_sizes = [int(value) for value in args.video_sizes.split(",") if value.strip()]
    output_root.mkdir(parents=True, exist_ok=True)

    files = sorted(
        path for path in source_root.rglob("*")
        if path.is_file() and not path.name.startswith("._")
    )
    if args.limit:
        files = files[: args.limit]

    done = skipped = failed = 0
    failures: list[str] = []
    with ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        futures = {
            pool.submit(
                process_one,
                source_root,
                output_root,
                path,
                sizes,
                video_sizes,
                args.force,
                args.meta_only,
            ): path
            for path in files
        }
        for index, future in enumerate(as_completed(futures), start=1):
            status, error = future.result()
            if status == "done":
                done += 1
            elif status == "skip":
                skipped += 1
            else:
                failed += 1
                failures.append(error or "unknown error")
            if index % 25 == 0 or index == len(files):
                print(
                    f"[{index}/{len(files)}] generated={done} skipped={skipped} failed={failed}",
                    flush=True,
                )

    for failure in failures:
        print(f"FAILED: {failure}", file=sys.stderr)
    print(f"done: generated={done} skipped={skipped} failed={failed}")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
