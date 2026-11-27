use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use wait_timeout::ChildExt;

use crate::config::{FfmpegConfig, VideoConfig};

/// Transcode `source` to a browser-friendly H.264/AAC MP4.
///
/// The result is written atomically next to `destination`.
pub fn transcode(
    source: &Path,
    destination: &Path,
    ffmpeg: &FfmpegConfig,
    video: &VideoConfig,
) -> Result<()> {
    if destination.is_file() {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let tmp = destination.with_file_name(format!(
        ".tmp-{}-{}.mp4",
        uuid::Uuid::now_v7(),
        std::process::id()
    ));

    let width = video.transcode_max_width.max(320);
    let height = video.max_height.max(240);
    let scale = format!(
        "scale='min({width},iw)':'min({height},ih)':force_original_aspect_ratio=decrease,scale=trunc(iw/2)*2:trunc(ih/2)*2"
    );
    let mut child = Command::new(&ffmpeg.ffmpeg_path)
        .args(["-v", "error", "-y", "-i"])
        .arg(source)
        .args([
            "-map",
            "0:v:0",
            "-map",
            "0:a?",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            &video.transcode_crf.max(1).to_string(),
            "-vf",
            &scale,
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-movflags",
            "+faststart",
        ])
        .arg(&tmp)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn {}", ffmpeg.ffmpeg_path))?;

    let status = child
        .wait_timeout(Duration::from_secs(ffmpeg.timeout_seconds.max(10)))
        .context("failed to wait for ffmpeg")?;

    match status {
        Some(status) if status.success() => {
            fs::rename(&tmp, destination).with_context(|| {
                format!(
                    "failed to move {} -> {}",
                    tmp.display(),
                    destination.display()
                )
            })?;
            Ok(())
        }
        Some(status) => {
            let _ = fs::remove_file(&tmp);
            let mut error = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                use std::io::Read;
                let _ = stderr.read_to_string(&mut error);
            }
            bail!("ffmpeg exited with {status}: {}", error.trim());
        }
        None => {
            let _ = child.kill();
            let _ = fs::remove_file(&tmp);
            bail!("ffmpeg timed out after {}s", ffmpeg.timeout_seconds.max(10));
        }
    }
}
