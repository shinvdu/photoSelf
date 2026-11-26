use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Component, Path};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use tracing::{debug, warn};
use walkdir::WalkDir;

use crate::config::Config;
use crate::db::Db;
use crate::model::{Library, MediaItem, MediaKind, ScanStats};
use crate::thumb::probe_video;
use crate::util;

fn build_ignore_set(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        match Glob::new(pattern) {
            Ok(glob) => {
                builder.add(glob);
            }
            Err(error) => warn!(pattern, %error, "invalid ignore glob, skipping"),
        }
    }
    Ok(builder.build()?)
}

fn file_hash(path: &Path) -> Result<String> {
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

struct ImageMeta {
    width: Option<i64>,
    height: Option<i64>,
    taken_at: Option<String>,
    camera: Option<String>,
    orientation: Option<i64>,
}

fn image_metadata(path: &Path) -> ImageMeta {
    let dimensions = image::image_dimensions(path).ok();
    let mut taken_at = None;
    let mut camera = None;
    let mut orientation = None;

    if let Ok(file) = File::open(path) {
        let reader = exif::Reader::new();
        if let Ok(exif) = reader.read_from_container(&mut BufReader::new(file)) {
            if let Some(field) = exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY) {
                taken_at = util::normalize_exif_datetime(&field.display_value().to_string());
            }
            if taken_at.is_none() {
                if let Some(field) = exif.get_field(exif::Tag::DateTime, exif::In::PRIMARY) {
                    taken_at = util::normalize_exif_datetime(&field.display_value().to_string());
                }
            }
            if let Some(field) = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY) {
                orientation = field.value.get_uint(0).map(|value| value as i64);
            }

            let make = exif
                .get_field(exif::Tag::Make, exif::In::PRIMARY)
                .map(|field| field.display_value().to_string())
                .unwrap_or_default();
            let model = exif
                .get_field(exif::Tag::Model, exif::In::PRIMARY)
                .map(|field| field.display_value().to_string())
                .unwrap_or_default();
            let make = make.trim().trim_matches('"').trim();
            let model = model.trim().trim_matches('"').trim();
            let combined = format!("{make} {model}");
            let combined = combined.trim();
            if !combined.is_empty() {
                camera = Some(combined.to_string());
            }
        }
    }

    let (width, height) = dimensions
        .map(|(width, height)| (Some(width as i64), Some(height as i64)))
        .unwrap_or((None, None));
    ImageMeta {
        width,
        height,
        taken_at,
        camera,
        orientation,
    }
}

#[derive(serde::Deserialize)]
struct SidecarMeta {
    width: Option<i64>,
    height: Option<i64>,
    duration_ms: Option<i64>,
    taken_at: Option<String>,
}

fn read_sidecar_meta(config: &Config, library_id: &str, rel_path: &str) -> Option<SidecarMeta> {
    let dir = config.thumbnails.sidecar_dir.as_ref()?;
    let library = Path::new(library_id);
    if library.is_absolute()
        || library
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    let path = Path::new(dir)
        .join(library)
        .join(format!("{rel_path}.meta.json"));
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

pub fn scan_library(
    config: &Config,
    db: &Db,
    library: &Library,
    progress: Option<&dyn Fn(usize, usize)>,
) -> Result<ScanStats> {
    let ignore = build_ignore_set(&library.ignore_globs)?;
    let mut files = Vec::new();

    for entry in WalkDir::new(&library.root_path)
        .follow_links(config.scan.follow_symlinks)
        .into_iter()
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                warn!(library = %library.id, %error, "walk error");
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let rel_path = path
            .strip_prefix(&library.root_path)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let file_name = entry.file_name().to_string_lossy();

        if ignore.is_match(file_name.as_ref()) || ignore.is_match(rel_path.as_str()) {
            continue;
        }
        if util::is_supported_path(path).is_none() {
            continue;
        }
        files.push((path.to_path_buf(), rel_path));
    }

    let total = files.len();
    db.mark_all_missing(&library.id)?;
    if let Some(progress) = progress {
        progress(0, total);
    }

    let mut stats = ScanStats::default();
    for (index, (path, rel_path)) in files.iter().enumerate() {
        match scan_one(config, db, library, path, rel_path) {
            Ok(()) => stats.scanned += 1,
            Err(error) => {
                stats.errors += 1;
                warn!(
                    library = %library.id,
                    path = %path.display(),
                    %error,
                    "failed to index file"
                );
            }
        }

        if let Some(progress) = progress {
            if index % 25 == 0 || index + 1 == total {
                progress(index + 1, total);
            }
        }
    }

    stats.missing = db.count_missing(&library.id)?;
    debug!(
        library = %library.id,
        scanned = stats.scanned,
        errors = stats.errors,
        missing = stats.missing,
        "scan finished"
    );
    Ok(stats)
}

fn scan_one(
    config: &Config,
    db: &Db,
    library: &Library,
    path: &Path,
    rel_path: &str,
) -> Result<()> {
    let metadata =
        fs::metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    let size = metadata.len() as i64;
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let kind = util::is_supported_path(path).unwrap_or(MediaKind::Other);

    let existing = db.find_by_path(&library.id, rel_path)?;

    // Reuse the cached hash when size and mtime are unchanged. This keeps
    // rescans on slow USB/NAND devices cheap and avoids re-reading the whole
    // library just to refresh metadata.
    let content_hash = if let Some(item) = existing.as_ref() {
        if item.size == size && item.mtime == mtime {
            item.content_hash.clone()
        } else if config.scan.hash {
            Some(file_hash(path)?)
        } else {
            None
        }
    } else if config.scan.hash {
        Some(file_hash(path)?)
    } else {
        None
    };

    let mut width = None;
    let mut height = None;
    let mut duration_ms = None;
    let mut taken_at = None;
    let mut camera = None;
    let mut orientation = None;

    match kind {
        MediaKind::Image => {
            let meta = image_metadata(path);
            width = meta.width;
            height = meta.height;
            taken_at = meta.taken_at;
            camera = meta.camera;
            orientation = meta.orientation;

            // Some files are HEIC renamed to .JPG, or otherwise not decodable by
            // the image crate. If ffmpeg/ffprobe understands them, fill in the
            // dimensions so the UI can still show metadata.
            if width.is_none() && config.scan.probe_videos {
                if let Some(probe) = probe_video(path, &config.ffmpeg.ffprobe_path) {
                    width = probe.width;
                    height = probe.height;
                }
            }
        }
        MediaKind::Video => {
            if config.scan.probe_videos {
                if let Some(meta) = probe_video(path, &config.ffmpeg.ffprobe_path) {
                    width = meta.width;
                    height = meta.height;
                    duration_ms = meta.duration_ms;
                    taken_at = meta.creation_time;
                }
            }
        }
        MediaKind::Other => {}
    }

    if width.is_none() || height.is_none() || duration_ms.is_none() || taken_at.is_none() {
        if let Some(meta) = read_sidecar_meta(config, &library.id, rel_path) {
            width = width.or(meta.width);
            height = height.or(meta.height);
            duration_ms = duration_ms.or(meta.duration_ms);
            if taken_at.is_none() {
                taken_at = meta.taken_at;
            }
        }
    }

    let taken_at = taken_at.unwrap_or_else(|| util::mtime_to_utc(mtime));

    let adopted_id = if existing.is_none() {
        match content_hash.as_deref() {
            Some(hash) => db.find_missing_id_by_hash(hash)?,
            None => None,
        }
    } else {
        None
    };

    let item = MediaItem {
        id: existing
            .as_ref()
            .map(|item| item.id.clone())
            .or(adopted_id.clone())
            .unwrap_or_else(new_id),
        library_id: library.id.clone(),
        rel_path: rel_path.to_string(),
        content_hash,
        kind,
        size,
        mtime,
        width,
        height,
        duration_ms,
        taken_at: Some(taken_at),
        camera,
        orientation,
        favorite: existing.as_ref().map(|item| item.favorite).unwrap_or(false),
        missing: false,
    };

    if adopted_id.is_some() {
        db.update_item_by_id(&item)?;
    } else {
        db.upsert_item(&item)?;
    }
    Ok(())
}
