use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Image,
    Video,
    Other,
}

impl MediaKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaKind::Image => "image",
            MediaKind::Video => "video",
            MediaKind::Other => "other",
        }
    }

    pub fn from_db_str(value: &str) -> Self {
        match value {
            "image" => MediaKind::Image,
            "video" => MediaKind::Video,
            _ => MediaKind::Other,
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        let ext = ext.to_ascii_lowercase();
        match ext.as_str() {
            "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "tif" | "tiff" | "heic" | "heif"
            | "avif" => Some(MediaKind::Image),
            "mp4" | "m4v" | "mov" | "webm" | "ogv" | "mkv" | "avi" => Some(MediaKind::Video),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: String,
    pub library_id: String,
    pub rel_path: String,
    pub content_hash: Option<String>,
    pub kind: MediaKind,
    pub size: i64,
    pub mtime: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub taken_at: Option<String>,
    pub camera: Option<String>,
    pub orientation: Option<i64>,
    pub favorite: bool,
    pub missing: bool,
}

impl MediaItem {
    pub fn filename(&self) -> &str {
        self.rel_path
            .rsplit('/')
            .next()
            .unwrap_or(self.rel_path.as_str())
    }

    pub fn sort_key(&self) -> String {
        self.taken_at
            .clone()
            .unwrap_or_else(|| "1970-01-01T00:00:00".to_string())
    }

    pub fn cache_key(&self) -> String {
        self.content_hash.clone().unwrap_or_else(|| self.id.clone())
    }
}

#[derive(Debug, Clone)]
pub struct Library {
    pub id: String,
    pub name: String,
    pub root_path: PathBuf,
    pub read_only: bool,
    pub enabled: bool,
    pub ignore_globs: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ItemQuery {
    pub library_id: Option<String>,
    pub kind: Option<String>,
    pub q: Option<String>,
    pub favorite: Option<bool>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub cursor: Option<String>,
    pub limit: i64,
}

impl ItemQuery {
    pub fn effective_limit(&self) -> i64 {
        if self.limit <= 0 {
            100
        } else {
            self.limit.clamp(1, 500)
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScanStats {
    pub scanned: usize,
    pub errors: usize,
    pub missing: usize,
}
