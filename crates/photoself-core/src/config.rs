use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

fn default_ignore_globs() -> Vec<String> {
    vec![
        "._*".to_string(),
        ".DS_Store".to_string(),
        "@eaDir".to_string(),
        "#recycle".to_string(),
    ]
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub data: DataConfig,
    pub libraries: Vec<LibraryConfig>,
    pub manage: ManageConfig,
    pub media: MediaConfig,
    pub thumbnails: ThumbConfig,
    pub ffmpeg: FfmpegConfig,
    pub video: VideoConfig,
    pub auth: AuthConfig,
    pub scan: ScanConfig,
    pub trash: TrashConfig,
    pub write: WriteConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// `standalone` or `reverse-proxy`.
    pub mode: String,
    pub listen: String,
    pub base_path: String,
    pub ui_embedded: bool,
    pub ui_dir: String,
    pub trusted_proxies: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            mode: "standalone".into(),
            listen: "127.0.0.1:8787".into(),
            base_path: "/".into(),
            ui_embedded: false,
            ui_dir: "ui/dist".into(),
            trusted_proxies: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DataConfig {
    pub dir: String,
}

impl Default for DataConfig {
    fn default() -> Self {
        Self {
            dir: "./data".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LibraryConfig {
    pub id: String,
    pub name: String,
    pub path: String,
    pub read_only: bool,
    pub enabled: bool,
    pub ignore_globs: Vec<String>,
}

impl Default for LibraryConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            path: String::new(),
            read_only: true,
            enabled: true,
            ignore_globs: default_ignore_globs(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MediaConfig {
    /// `standalone` (served by this server) or `proxy` (served by a reverse proxy).
    pub serve_mode: String,
    pub public_media_path: String,
    pub public_thumb_path: String,
}

/// Runtime library management from the web UI.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ManageConfig {
    /// Master switch. Disabled by default so upgrades never expose a path
    /// picker unexpectedly.
    pub enabled: bool,
    /// Absolute directories that may be added as libraries from the UI.
    /// An empty list means "nothing can be added from the UI".
    pub allowed_roots: Vec<String>,
    /// Refuse management endpoints when `auth.mode = "none"`.
    pub require_auth: bool,
}

impl Default for ManageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_roots: Vec::new(),
            require_auth: true,
        }
    }
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            serve_mode: "standalone".into(),
            public_media_path: "/media".into(),
            public_thumb_path: "/thumbs".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ThumbConfig {
    pub dir: Option<String>,
    /// Optional directory of pre-generated previews named
    /// `<relative-path>.<size>.jpg`. Useful for HEIC / HEVC media on devices
    /// whose ffmpeg lacks those decoders.
    pub sidecar_dir: Option<String>,
    pub sizes: Vec<u32>,
    pub format: String,
    pub quality: u8,
    pub concurrency: usize,
    pub max_pixels: u64,
    pub prewarm_on_scan: bool,
}

impl Default for ThumbConfig {
    fn default() -> Self {
        Self {
            dir: None,
            sidecar_dir: None,
            sizes: vec![320, 1280],
            format: "jpeg".into(),
            quality: 82,
            concurrency: 2,
            max_pixels: 100_000_000,
            prewarm_on_scan: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FfmpegConfig {
    pub enabled: bool,
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    pub concurrency: usize,
    pub timeout_seconds: u64,
}

impl Default for FfmpegConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ffmpeg_path: "ffmpeg".into(),
            ffprobe_path: "ffprobe".into(),
            concurrency: 1,
            timeout_seconds: 120,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct VideoConfig {
    pub poster_enabled: bool,
    pub transcode_enabled: bool,
    pub max_height: u32,
    pub transcode_concurrency: usize,
    pub transcode_max_width: u32,
    pub transcode_crf: u8,
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            poster_enabled: true,
            transcode_enabled: false,
            max_height: 1080,
            transcode_concurrency: 1,
            transcode_max_width: 1280,
            transcode_crf: 23,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    /// `none`, `basic` or `proxy`.
    pub mode: String,
    pub username: Option<String>,
    pub password_hash: Option<String>,
    pub session_ttl_hours: u64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            mode: "none".into(),
            username: None,
            password_hash: None,
            session_ttl_hours: 720,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ScanConfig {
    pub on_start: bool,
    pub interval: String,
    pub concurrency: usize,
    pub follow_symlinks: bool,
    pub missing_retention_days: u32,
    pub hash: bool,
    pub probe_videos: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            on_start: true,
            interval: "0".into(),
            concurrency: 2,
            follow_symlinks: false,
            missing_retention_days: 30,
            hash: true,
            probe_videos: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TrashConfig {
    pub enabled: bool,
    pub retention_days: u32,
    pub dir: Option<String>,
}

impl Default for TrashConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            retention_days: 30,
            dir: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WriteConfig {
    pub enabled: bool,
    pub max_upload_bytes: u64,
}

impl Default for WriteConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_upload_bytes: 512 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
            format: "text".into(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let config: Config = toml::from_str(&raw)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if !self.server.base_path.starts_with('/') {
            bail!("server.base_path must start with '/'");
        }
        if !matches!(self.auth.mode.as_str(), "none" | "basic" | "proxy") {
            bail!("auth.mode must be one of: none, basic, proxy");
        }
        if self.auth.mode == "basic"
            && (self.auth.username.is_none() || self.auth.password_hash.is_none())
        {
            bail!("auth.mode = basic requires auth.username and auth.password_hash");
        }

        // Libraries seeded from config are optional now: after the first
        // import the UI-managed SQLite registry is authoritative, and a user
        // may legitimately keep every library in the database only.
        let mut ids = std::collections::HashSet::new();
        for lib in self.libraries.iter().filter(|lib| lib.enabled) {
            if lib.id.trim().is_empty() {
                bail!("library id must not be empty");
            }
            if !ids.insert(lib.id.clone()) {
                bail!("duplicate library id: {}", lib.id);
            }
            if lib.path.trim().is_empty() {
                bail!("library '{}' has an empty path", lib.id);
            }
        }

        for root in &self.manage.allowed_roots {
            if !root.trim().starts_with('/') {
                bail!("manage.allowed_roots entries must be absolute paths");
            }
        }

        if self.thumbnails.sizes.is_empty() {
            bail!("thumbnails.sizes must not be empty");
        }
        if self.thumbnails.sizes.iter().any(|s| *s == 0 || *s > 4096) {
            bail!("thumbnail sizes must be in 1..=4096");
        }
        if self.thumbnails.concurrency == 0 {
            bail!("thumbnails.concurrency must be at least 1");
        }
        if self.ffmpeg.concurrency == 0 {
            bail!("ffmpeg.concurrency must be at least 1");
        }
        if self.video.transcode_concurrency == 0 {
            bail!("video.transcode_concurrency must be at least 1");
        }
        if self.write.max_upload_bytes == 0 {
            bail!("write.max_upload_bytes must be positive");
        }
        Ok(())
    }

    pub fn data_dir(&self) -> PathBuf {
        PathBuf::from(&self.data.dir)
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir().join("photoself.db")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.data_dir().join("cache")
    }

    pub fn thumb_dir(&self) -> PathBuf {
        self.thumbnails
            .dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.cache_dir().join("thumbs"))
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.cache_dir().join("tmp")
    }

    pub fn trash_dir(&self) -> PathBuf {
        self.trash
            .dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.data_dir().join("trash"))
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [
            self.data_dir(),
            self.cache_dir(),
            self.thumb_dir(),
            self.tmp_dir(),
        ] {
            fs::create_dir_all(&dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }
        if self.trash.enabled {
            fs::create_dir_all(self.trash_dir())
                .with_context(|| format!("failed to create {}", self.trash_dir().display()))?;
        }
        Ok(())
    }
}

impl LibraryConfig {
    pub fn to_library(&self) -> crate::model::Library {
        crate::model::Library {
            id: self.id.clone(),
            name: if self.name.is_empty() {
                self.id.clone()
            } else {
                self.name.clone()
            },
            root_path: PathBuf::from(&self.path),
            read_only: self.read_only,
            enabled: self.enabled,
            ignore_globs: if self.ignore_globs.is_empty() {
                default_ignore_globs()
            } else {
                self.ignore_globs.clone()
            },
        }
    }
}
