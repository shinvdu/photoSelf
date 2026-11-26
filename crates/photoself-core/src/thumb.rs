use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use image::DynamicImage;
use wait_timeout::ChildExt;

use crate::config::{FfmpegConfig, ThumbConfig};
use crate::model::{MediaItem, MediaKind};
use crate::util;

#[derive(Debug, Clone, Default)]
pub struct VideoMeta {
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub creation_time: Option<String>,
}

pub fn cache_path(cache_dir: &Path, key: &str, size: u32, format: &str) -> PathBuf {
    let a = key.get(0..2).unwrap_or("00");
    let b = key.get(2..4).unwrap_or("00");
    cache_dir
        .join(a)
        .join(b)
        .join(format!("{key}-{size}.{format}"))
}

pub fn ensure_thumbnail(
    item: &MediaItem,
    source: &Path,
    destination: &Path,
    size: u32,
    thumbs: &ThumbConfig,
    ffmpeg: &FfmpegConfig,
) -> Result<()> {
    if destination.is_file() {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let tmp = temporary_path(destination);
    let result = match item.kind {
        MediaKind::Image => match generate_image(source, &tmp, size, thumbs, item.orientation) {
            Ok(()) => Ok(()),
            Err(image_error) => {
                if ffmpeg.enabled {
                    generate_ffmpeg_thumb(source, &tmp, size, ffmpeg).map_err(|ffmpeg_error| {
                        anyhow!(
                            "image decode failed ({image_error}); ffmpeg fallback failed ({ffmpeg_error})"
                        )
                    })
                } else {
                    Err(image_error)
                }
            }
        },
        MediaKind::Video => {
            if !ffmpeg.enabled {
                bail!("video thumbnails require ffmpeg, which is disabled");
            }
            generate_ffmpeg_thumb(source, &tmp, size, ffmpeg)
        }
        MediaKind::Other => bail!("unsupported media type"),
    };

    match result {
        Ok(()) => {
            fs::rename(&tmp, destination).with_context(|| {
                format!(
                    "failed to move thumbnail {} -> {}",
                    tmp.display(),
                    destination.display()
                )
            })?;
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(&tmp);
            Err(error)
        }
    }
}

fn temporary_path(destination: &Path) -> PathBuf {
    let ext = destination
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("jpg");
    destination.with_file_name(format!(
        ".tmp-{}-{}.{}",
        uuid::Uuid::now_v7(),
        std::process::id(),
        ext
    ))
}

fn generate_image(
    source: &Path,
    destination: &Path,
    size: u32,
    thumbs: &ThumbConfig,
    orientation: Option<i64>,
) -> Result<()> {
    let image = image::open(source)
        .with_context(|| format!("failed to decode image {}", source.display()))?;
    let pixels = image.width() as u64 * image.height() as u64;
    if pixels > thumbs.max_pixels {
        bail!(
            "image has {} pixels, above thumbnails.max_pixels = {}",
            pixels,
            thumbs.max_pixels
        );
    }

    let thumb = image.thumbnail(size, size);
    let oriented = apply_orientation(thumb, orientation);

    match thumbs.format.to_ascii_lowercase().as_str() {
        "jpeg" | "jpg" => {
            let file = File::create(destination)?;
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(file, thumbs.quality);
            encoder.encode_image(&oriented)?;
        }
        "webp" => {
            oriented.save_with_format(destination, image::ImageFormat::WebP)?;
        }
        other => bail!("unsupported thumbnail format: {other}"),
    }
    Ok(())
}

fn generate_ffmpeg_thumb(
    source: &Path,
    destination: &Path,
    size: u32,
    ffmpeg: &FfmpegConfig,
) -> Result<()> {
    let attempt = |seek: &str| -> Result<()> {
        let mut child = Command::new(&ffmpeg.ffmpeg_path)
            .args(["-v", "error", "-ss", seek, "-i"])
            .arg(source)
            .args([
                "-frames:v",
                "1",
                "-vf",
                &format!("scale={size}:-2"),
                "-q:v",
                "4",
                "-y",
            ])
            .arg(destination)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to spawn {}", ffmpeg.ffmpeg_path))?;

        let status = child
            .wait_timeout(Duration::from_secs(ffmpeg.timeout_seconds))
            .context("failed to wait for ffmpeg")?;
        match status {
            Some(status) if status.success() => Ok(()),
            Some(status) => bail!("ffmpeg exited with {status}"),
            None => {
                let _ = child.kill();
                bail!("ffmpeg timed out after {}s", ffmpeg.timeout_seconds);
            }
        }
    };

    // Videos: try 1s in (avoids a black first frame). Images and short videos
    // fall back to the very first frame.
    match attempt("00:00:01") {
        Ok(()) => Ok(()),
        Err(first) => attempt("00:00:00").map_err(|_| first),
    }
}

pub fn apply_orientation(image: DynamicImage, orientation: Option<i64>) -> DynamicImage {
    match orientation {
        Some(2) => image.fliph(),
        Some(3) => image.rotate180(),
        Some(4) => image.flipv(),
        Some(5) => image.rotate90().fliph(),
        Some(6) => image.rotate90(),
        Some(7) => image.rotate270().fliph(),
        Some(8) => image.rotate270(),
        _ => image,
    }
}

pub fn probe_video(path: &Path, ffprobe_path: &str) -> Option<VideoMeta> {
    let output = Command::new(ffprobe_path)
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let mut meta = VideoMeta::default();

    if let Some(streams) = value.get("streams").and_then(|value| value.as_array()) {
        if let Some(video) = streams
            .iter()
            .find(|stream| stream.get("codec_type").and_then(|v| v.as_str()) == Some("video"))
        {
            meta.width = video.get("width").and_then(|v| v.as_i64());
            meta.height = video.get("height").and_then(|v| v.as_i64());
            if let Some(created) = video
                .get("tags")
                .and_then(|tags| tags.get("creation_time"))
                .and_then(|v| v.as_str())
            {
                meta.creation_time = util::rfc3339_to_utc(created);
            }
        }
    }

    if let Some(format) = value.get("format") {
        if let Some(duration) = format.get("duration").and_then(|v| v.as_str()) {
            if let Ok(seconds) = duration.parse::<f64>() {
                meta.duration_ms = Some((seconds * 1000.0) as i64);
            }
        }
        if meta.creation_time.is_none() {
            if let Some(created) = format
                .get("tags")
                .and_then(|tags| tags.get("creation_time"))
                .and_then(|v| v.as_str())
            {
                meta.creation_time = util::rfc3339_to_utc(created);
            }
        }
    }

    Some(meta)
}
