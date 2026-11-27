use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};
use serde::Serialize;

use crate::config::LibraryConfig;
use crate::model::{Album, ItemQuery, MediaItem, MediaKind, Tag, TrashEntry};
use crate::util;

const ITEM_COLUMNS: &str = "id, library_id, rel_path, content_hash, kind, size, mtime, \
    width, height, duration_ms, taken_at, camera, orientation, favorite, missing";

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub progress: i64,
    pub total: i64,
    pub message: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn now_str() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn row_to_item(row: &Row<'_>) -> rusqlite::Result<MediaItem> {
    let kind: String = row.get("kind")?;
    Ok(MediaItem {
        id: row.get("id")?,
        library_id: row.get("library_id")?,
        rel_path: row.get("rel_path")?,
        content_hash: row.get("content_hash")?,
        kind: MediaKind::from_db_str(&kind),
        size: row.get("size")?,
        mtime: row.get("mtime")?,
        width: row.get("width")?,
        height: row.get("height")?,
        duration_ms: row.get("duration_ms")?,
        taken_at: row.get("taken_at")?,
        camera: row.get("camera")?,
        orientation: row.get("orientation")?,
        favorite: row.get::<_, i64>("favorite")? != 0,
        missing: row.get::<_, i64>("missing")? != 0,
    })
}

fn row_to_tag(row: &Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: row.get("id")?,
        name: row.get("name")?,
        color: row.get("color")?,
        count: row.get("count")?,
    })
}

fn row_to_album(row: &Row<'_>) -> rusqlite::Result<Album> {
    Ok(Album {
        id: row.get("id")?,
        name: row.get("name")?,
        description: row.get("description")?,
        cover_item_id: row.get("cover_item_id")?,
        item_count: row.get("item_count")?,
        created_at: row.get("created_at")?,
    })
}

fn row_to_trash(row: &Row<'_>) -> rusqlite::Result<TrashEntry> {
    let rel_path: String = row.get("rel_path")?;
    let filename = rel_path
        .rsplit('/')
        .next()
        .unwrap_or(rel_path.as_str())
        .to_string();
    Ok(TrashEntry {
        id: row.get("id")?,
        item_id: row.get("item_id")?,
        library_id: row.get("library_id")?,
        rel_path,
        filename,
        trash_path: row.get("trash_path")?,
        deleted_at: row.get("deleted_at")?,
    })
}

fn row_to_job(row: &Row<'_>) -> rusqlite::Result<Job> {
    Ok(Job {
        id: row.get("id")?,
        kind: row.get("kind")?,
        state: row.get("state")?,
        progress: row.get("progress")?,
        total: row.get("total")?,
        message: row.get("message")?,
        error: row.get("error")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database {}", path.display()))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;
             PRAGMA busy_timeout=5000;",
        )?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS libraries (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                root_path TEXT NOT NULL,
                read_only INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS items (
                id TEXT PRIMARY KEY,
                library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
                rel_path TEXT NOT NULL,
                content_hash TEXT,
                kind TEXT NOT NULL,
                size INTEGER NOT NULL,
                mtime INTEGER NOT NULL,
                width INTEGER,
                height INTEGER,
                duration_ms INTEGER,
                taken_at TEXT,
                camera TEXT,
                orientation INTEGER,
                favorite INTEGER NOT NULL DEFAULT 0,
                missing INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(library_id, rel_path)
            );

            CREATE INDEX IF NOT EXISTS idx_items_timeline
                ON items(missing, taken_at DESC, id DESC);
            CREATE INDEX IF NOT EXISTS idx_items_hash ON items(content_hash);
            CREATE INDEX IF NOT EXISTS idx_items_library ON items(library_id, missing);

            CREATE TABLE IF NOT EXISTS tags (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                color TEXT
            );

            CREATE TABLE IF NOT EXISTS item_tags (
                item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
                tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                PRIMARY KEY (item_id, tag_id)
            );

            CREATE TABLE IF NOT EXISTS albums (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                cover_item_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS album_items (
                album_id TEXT NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
                item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
                position INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (album_id, item_id)
            );

            CREATE TABLE IF NOT EXISTS trash (
                id TEXT PRIMARY KEY,
                item_id TEXT NOT NULL,
                library_id TEXT NOT NULL,
                rel_path TEXT NOT NULL,
                trash_path TEXT NOT NULL,
                deleted_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_trash_deleted ON trash(deleted_at DESC);

            CREATE TABLE IF NOT EXISTS derivatives (
                item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
                kind TEXT NOT NULL,
                path TEXT NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY (item_id, kind)
            );

            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                state TEXT NOT NULL,
                progress INTEGER NOT NULL DEFAULT 0,
                total INTEGER NOT NULL DEFAULT 0,
                message TEXT,
                error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    pub fn ensure_libraries(&self, libraries: &[LibraryConfig]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let now = now_str();
        for lib in libraries.iter().filter(|lib| lib.enabled) {
            tx.execute(
                "INSERT INTO libraries(id, name, root_path, read_only, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    root_path = excluded.root_path,
                    read_only = excluded.read_only,
                    updated_at = excluded.updated_at",
                params![
                    lib.id,
                    if lib.name.is_empty() {
                        &lib.id
                    } else {
                        &lib.name
                    },
                    lib.path,
                    if lib.read_only { 1i64 } else { 0i64 },
                    now
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn find_by_path(&self, library_id: &str, rel_path: &str) -> Result<Option<MediaItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT {ITEM_COLUMNS} FROM items WHERE library_id = ?1 AND rel_path = ?2"
        ))?;
        let item = stmt
            .query_row(params![library_id, rel_path], row_to_item)
            .optional()?;
        Ok(item)
    }

    pub fn find_missing_id_by_hash(&self, content_hash: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let id = conn
            .query_row(
                "SELECT id FROM items WHERE content_hash = ?1 AND missing = 1 LIMIT 1",
                params![content_hash],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(id)
    }

    pub fn mark_all_missing(&self, library_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE items SET missing = 1 WHERE library_id = ?1 AND missing = 0",
            params![library_id],
        )?;
        Ok(())
    }

    pub fn count_missing(&self, library_id: &str) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM items WHERE library_id = ?1 AND missing = 1",
            params![library_id],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn upsert_item(&self, item: &MediaItem) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = now_str();
        conn.execute(
            "INSERT INTO items(
                id, library_id, rel_path, content_hash, kind, size, mtime,
                width, height, duration_ms, taken_at, camera, orientation,
                favorite, missing, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 0, ?15, ?15)
             ON CONFLICT(library_id, rel_path) DO UPDATE SET
                content_hash = excluded.content_hash,
                kind = excluded.kind,
                size = excluded.size,
                mtime = excluded.mtime,
                width = excluded.width,
                height = excluded.height,
                duration_ms = excluded.duration_ms,
                taken_at = excluded.taken_at,
                camera = excluded.camera,
                orientation = excluded.orientation,
                missing = 0,
                updated_at = excluded.updated_at",
            params![
                item.id,
                item.library_id,
                item.rel_path,
                item.content_hash,
                item.kind.as_str(),
                item.size,
                item.mtime,
                item.width,
                item.height,
                item.duration_ms,
                item.taken_at,
                item.camera,
                item.orientation,
                if item.favorite { 1i64 } else { 0i64 },
                now
            ],
        )?;
        Ok(())
    }

    /// Used when a file was renamed: reuse the old item id and point it at
    /// the new relative path.
    pub fn update_item_by_id(&self, item: &MediaItem) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = now_str();
        conn.execute(
            "UPDATE items SET
                library_id = ?2, rel_path = ?3, content_hash = ?4, kind = ?5,
                size = ?6, mtime = ?7, width = ?8, height = ?9,
                duration_ms = ?10, taken_at = ?11, camera = ?12,
                orientation = ?13, missing = 0, updated_at = ?14
             WHERE id = ?1",
            params![
                item.id,
                item.library_id,
                item.rel_path,
                item.content_hash,
                item.kind.as_str(),
                item.size,
                item.mtime,
                item.width,
                item.height,
                item.duration_ms,
                item.taken_at,
                item.camera,
                item.orientation,
                now
            ],
        )?;
        Ok(())
    }

    pub fn get_item(&self, id: &str) -> Result<Option<MediaItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!("SELECT {ITEM_COLUMNS} FROM items WHERE id = ?1"))?;
        let item = stmt.query_row(params![id], row_to_item).optional()?;
        Ok(item)
    }

    pub fn set_favorite(&self, id: &str, favorite: bool) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE items SET favorite = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, if favorite { 1i64 } else { 0i64 }, now_str()],
        )?;
        Ok(changed > 0)
    }

    pub fn list_items(&self, query: &ItemQuery) -> Result<(Vec<MediaItem>, Option<String>)> {
        let conn = self.conn.lock().unwrap();
        let mut sql = format!("SELECT {ITEM_COLUMNS} FROM items WHERE missing = 0");
        let mut values: Vec<Value> = Vec::new();

        if let Some(library_id) = &query.library_id {
            sql.push_str(" AND library_id = ?");
            values.push(library_id.clone().into());
        }
        if let Some(kind) = &query.kind {
            sql.push_str(" AND kind = ?");
            values.push(kind.clone().into());
        }
        if let Some(favorite) = query.favorite {
            sql.push_str(" AND favorite = ?");
            values.push(if favorite { 1i64 } else { 0i64 }.into());
        }
        if let Some(tag) = query.tag.as_ref().filter(|value| !value.trim().is_empty()) {
            sql.push_str(
                " AND EXISTS (SELECT 1 FROM item_tags it JOIN tags t ON t.id = it.tag_id
                   WHERE it.item_id = items.id AND (t.name = ? OR CAST(t.id AS TEXT) = ?))",
            );
            values.push(tag.trim().to_string().into());
            values.push(tag.trim().to_string().into());
        }
        if let Some(album_id) = &query.album_id {
            sql.push_str(
                " AND EXISTS (SELECT 1 FROM album_items ai
                   WHERE ai.album_id = ? AND ai.item_id = items.id)",
            );
            values.push(album_id.clone().into());
        }
        if let Some(text) = query.q.as_ref().filter(|text| !text.trim().is_empty()) {
            sql.push_str(" AND rel_path LIKE ?");
            values.push(format!("%{}%", text.trim()).into());
        }
        if let Some(from) = &query.from {
            sql.push_str(" AND COALESCE(taken_at, '1970-01-01T00:00:00') >= ?");
            values.push(from.clone().into());
        }
        if let Some(to) = &query.to {
            sql.push_str(" AND COALESCE(taken_at, '1970-01-01T00:00:00') <= ?");
            values.push(to.clone().into());
        }
        if let Some(cursor) = &query.cursor {
            if let Some((key, id)) = util::decode_cursor(cursor) {
                sql.push_str(
                    " AND (COALESCE(taken_at, '1970-01-01T00:00:00') < ?
                       OR (COALESCE(taken_at, '1970-01-01T00:00:00') = ? AND id < ?))",
                );
                values.push(key.clone().into());
                values.push(key.into());
                values.push(id.into());
            }
        }

        let limit = query.effective_limit();
        sql.push_str(" ORDER BY COALESCE(taken_at, '1970-01-01T00:00:00') DESC, id DESC LIMIT ?");
        values.push((limit + 1).into());

        let mut stmt = conn.prepare(&sql)?;
        let mut items = stmt
            .query_map(params_from_iter(values.iter()), row_to_item)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let next_cursor = if items.len() as i64 > limit {
            items.pop();
            items
                .last()
                .map(|item| util::encode_cursor(Some(item.sort_key().as_str()), &item.id))
        } else {
            None
        };

        Ok((items, next_cursor))
    }

    pub fn count_items(&self) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let count = conn.query_row("SELECT COUNT(*) FROM items WHERE missing = 0", [], |row| {
            row.get(0)
        })?;
        Ok(count)
    }

    pub fn create_job(&self, kind: &str) -> Result<String> {
        let id = uuid::Uuid::now_v7().to_string();
        let now = now_str();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO jobs(id, kind, state, progress, total, created_at, updated_at)
             VALUES (?1, ?2, 'queued', 0, 0, ?3, ?3)",
            params![id, kind, now],
        )?;
        Ok(id)
    }

    pub fn update_job(
        &self,
        id: &str,
        state: &str,
        progress: i64,
        total: i64,
        message: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE jobs SET state = ?2, progress = ?3, total = ?4,
                message = ?5, error = ?6, updated_at = ?7
             WHERE id = ?1",
            params![id, state, progress, total, message, error, now_str()],
        )?;
        Ok(())
    }

    pub fn get_job(&self, id: &str) -> Result<Option<Job>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, kind, state, progress, total, message, error, created_at, updated_at
             FROM jobs WHERE id = ?1",
        )?;
        let job = stmt.query_row(params![id], row_to_job).optional()?;
        Ok(job)
    }

    pub fn list_jobs(&self, limit: i64) -> Result<Vec<Job>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, kind, state, progress, total, message, error, created_at, updated_at
             FROM jobs ORDER BY created_at DESC LIMIT ?1",
        )?;
        let jobs = stmt
            .query_map(params![limit.clamp(1, 100)], row_to_job)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(jobs)
    }
    // ---- tags -----------------------------------------------------------

    pub fn list_tags(&self) -> Result<Vec<Tag>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.color, COUNT(it.item_id) AS count
             FROM tags t LEFT JOIN item_tags it ON it.tag_id = t.id
             GROUP BY t.id ORDER BY t.name COLLATE NOCASE",
        )?;
        let tags = stmt
            .query_map([], row_to_tag)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(tags)
    }

    pub fn create_tag(&self, name: &str, color: Option<&str>) -> Result<Tag> {
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("tag name must not be empty");
        }
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO tags(name, color) VALUES (?1, ?2)
             ON CONFLICT(name) DO UPDATE SET color = excluded.color",
            params![name, color],
        )?;
        let tag = conn.query_row(
            "SELECT t.id, t.name, t.color, COUNT(it.item_id) AS count
             FROM tags t LEFT JOIN item_tags it ON it.tag_id = t.id
             WHERE t.name = ?1 GROUP BY t.id",
            params![name],
            row_to_tag,
        )?;
        Ok(tag)
    }

    pub fn delete_tag(&self, id: i64) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute("DELETE FROM tags WHERE id = ?1", params![id])? > 0)
    }

    pub fn add_item_tag(&self, item_id: &str, tag_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO item_tags(item_id, tag_id) VALUES (?1, ?2)",
            params![item_id, tag_id],
        )?;
        Ok(())
    }

    pub fn remove_item_tag(&self, item_id: &str, tag_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM item_tags WHERE item_id = ?1 AND tag_id = ?2",
            params![item_id, tag_id],
        )?;
        Ok(())
    }

    pub fn tags_for_item(&self, item_id: &str) -> Result<Vec<Tag>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.color, 0 AS count
             FROM item_tags it JOIN tags t ON t.id = it.tag_id
             WHERE it.item_id = ?1 ORDER BY t.name COLLATE NOCASE",
        )?;
        let tags = stmt
            .query_map(params![item_id], row_to_tag)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(tags)
    }

    pub fn tags_for_items(&self, item_ids: &[String]) -> Result<HashMap<String, Vec<Tag>>> {
        let mut map = HashMap::new();
        for item_id in item_ids {
            map.insert(item_id.clone(), self.tags_for_item(item_id)?);
        }
        Ok(map)
    }

    // ---- albums ---------------------------------------------------------

    pub fn list_albums(&self) -> Result<Vec<Album>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT a.id, a.name, a.description, a.cover_item_id, a.created_at,
                    (SELECT COUNT(*) FROM album_items ai WHERE ai.album_id = a.id) AS item_count
             FROM albums a ORDER BY a.created_at DESC",
        )?;
        let albums = stmt
            .query_map([], row_to_album)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(albums)
    }

    pub fn get_album(&self, id: &str) -> Result<Option<Album>> {
        let conn = self.conn.lock().unwrap();
        let album = conn
            .query_row(
                "SELECT a.id, a.name, a.description, a.cover_item_id, a.created_at,
                        (SELECT COUNT(*) FROM album_items ai WHERE ai.album_id = a.id) AS item_count
                 FROM albums a WHERE a.id = ?1",
                params![id],
                row_to_album,
            )
            .optional()?;
        Ok(album)
    }

    pub fn create_album(&self, name: &str, description: Option<&str>) -> Result<Album> {
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("album name must not be empty");
        }
        let id = uuid::Uuid::now_v7().to_string();
        let now = now_str();
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO albums(id, name, description, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![id, name, description, now],
            )?;
        }
        self.get_album(&id)?
            .ok_or_else(|| anyhow::anyhow!("album not found"))
    }

    pub fn delete_album(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute("DELETE FROM albums WHERE id = ?1", params![id])? > 0)
    }

    pub fn add_album_item(&self, album_id: &str, item_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO album_items(album_id, item_id) VALUES (?1, ?2)",
            params![album_id, item_id],
        )?;
        conn.execute(
            "UPDATE albums SET cover_item_id = COALESCE(cover_item_id, ?2), updated_at = ?3
             WHERE id = ?1",
            params![album_id, item_id, now_str()],
        )?;
        Ok(())
    }

    pub fn remove_album_item(&self, album_id: &str, item_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM album_items WHERE album_id = ?1 AND item_id = ?2",
            params![album_id, item_id],
        )?;
        conn.execute(
            "UPDATE albums SET cover_item_id = (
                SELECT item_id FROM album_items WHERE album_id = ?1 LIMIT 1
             ), updated_at = ?2 WHERE id = ?1",
            params![album_id, now_str()],
        )?;
        Ok(())
    }

    // ---- trash ----------------------------------------------------------

    pub fn add_trash(&self, entry: &TrashEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO trash(id, item_id, library_id, rel_path, trash_path, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                entry.id,
                entry.item_id,
                entry.library_id,
                entry.rel_path,
                entry.trash_path,
                entry.deleted_at
            ],
        )?;
        Ok(())
    }

    pub fn list_trash(&self) -> Result<Vec<TrashEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, item_id, library_id, rel_path, trash_path, deleted_at
             FROM trash ORDER BY deleted_at DESC",
        )?;
        let entries = stmt
            .query_map([], row_to_trash)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(entries)
    }

    pub fn get_trash(&self, id: &str) -> Result<Option<TrashEntry>> {
        let conn = self.conn.lock().unwrap();
        let entry = conn
            .query_row(
                "SELECT id, item_id, library_id, rel_path, trash_path, deleted_at
                 FROM trash WHERE id = ?1",
                params![id],
                row_to_trash,
            )
            .optional()?;
        Ok(entry)
    }

    pub fn delete_trash_row(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM trash WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn set_missing(&self, item_id: &str, missing: bool) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute(
            "UPDATE items SET missing = ?2, updated_at = ?3 WHERE id = ?1",
            params![item_id, if missing { 1i64 } else { 0i64 }, now_str()],
        )? > 0)
    }

    // ---- video derivatives ---------------------------------------------

    pub fn set_derivative(&self, item_id: &str, kind: &str, path: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO derivatives(item_id, kind, path, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(item_id, kind) DO UPDATE SET path = excluded.path,
                created_at = excluded.created_at",
            params![item_id, kind, path, now_str()],
        )?;
        Ok(())
    }

    pub fn get_derivative(&self, item_id: &str, kind: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let path = conn
            .query_row(
                "SELECT path FROM derivatives WHERE item_id = ?1 AND kind = ?2",
                params![item_id, kind],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_item(id: &str, path: &str) -> MediaItem {
        MediaItem {
            id: id.to_string(),
            library_id: "lib".to_string(),
            rel_path: path.to_string(),
            content_hash: Some("hash".to_string()),
            kind: MediaKind::Image,
            size: 1,
            mtime: 0,
            width: Some(1),
            height: Some(1),
            duration_ms: None,
            taken_at: Some("2026-01-01T00:00:00".to_string()),
            camera: None,
            orientation: None,
            favorite: false,
            missing: false,
        }
    }

    #[test]
    fn upsert_and_list() {
        let db = Db::open_in_memory().unwrap();
        db.ensure_libraries(&[LibraryConfig {
            id: "lib".into(),
            name: "Library".into(),
            path: "/tmp".into(),
            ..Default::default()
        }])
        .unwrap();

        db.upsert_item(&sample_item("a", "one.jpg")).unwrap();
        db.upsert_item(&sample_item("b", "two.jpg")).unwrap();
        let (items, next) = db.list_items(&ItemQuery::default()).unwrap();
        assert_eq!(items.len(), 2);
        assert!(next.is_none());
    }

    #[test]
    fn tags_albums_and_trash() {
        let db = Db::open_in_memory().unwrap();
        db.ensure_libraries(&[LibraryConfig {
            id: "lib".into(),
            name: "Library".into(),
            path: "/tmp".into(),
            ..Default::default()
        }])
        .unwrap();
        db.upsert_item(&sample_item("a", "one.jpg")).unwrap();

        let tag = db.create_tag("travel", Some("#fff")).unwrap();
        db.add_item_tag("a", tag.id).unwrap();
        let tags = db.list_tags().unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].count, 1);
        assert_eq!(db.tags_for_item("a").unwrap().len(), 1);
        let filtered = db
            .list_items(&ItemQuery {
                tag: Some("travel".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(filtered.0.len(), 1);

        let album = db.create_album("Summer", None).unwrap();
        db.add_album_item(&album.id, "a").unwrap();
        let albums = db.list_albums().unwrap();
        assert_eq!(albums[0].item_count, 1);
        let album_items = db
            .list_items(&ItemQuery {
                album_id: Some(album.id.clone()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(album_items.0.len(), 1);

        let entry = crate::model::TrashEntry {
            id: "t1".into(),
            item_id: "a".into(),
            library_id: "lib".into(),
            rel_path: "one.jpg".into(),
            filename: "one.jpg".into(),
            trash_path: "/tmp/trash/one.jpg".into(),
            deleted_at: now_str(),
        };
        db.add_trash(&entry).unwrap();
        db.set_missing("a", true).unwrap();
        assert_eq!(db.list_trash().unwrap().len(), 1);
        assert_eq!(db.list_items(&ItemQuery::default()).unwrap().0.len(), 0);
        db.set_missing("a", false).unwrap();
        db.delete_trash_row("t1").unwrap();
        assert_eq!(db.list_trash().unwrap().len(), 0);

        db.set_derivative("a", "web", "/tmp/a-web.mp4").unwrap();
        assert_eq!(
            db.get_derivative("a", "web").unwrap().as_deref(),
            Some("/tmp/a-web.mp4")
        );
    }
}
