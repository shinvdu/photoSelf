use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use ipnet::IpNet;
use photoself_core::model::Library;
use photoself_core::{Config, Db};
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Db,
    pub thumb_sem: Arc<Semaphore>,
    pub transcode_sem: Arc<Semaphore>,
    pub trusted_proxies: Arc<Vec<IpNet>>,
    pub scanning: Arc<AtomicBool>,
    pub auth_cache: Arc<Mutex<HashMap<String, Instant>>>,
    /// Runtime library registry. The SQLite `libraries` table is
    /// authoritative; this is a cached snapshot for scans and request
    /// handlers.
    pub libraries: Arc<RwLock<Vec<Library>>>,
    /// Canonicalized `manage.allowed_roots`.
    pub manage_roots: Arc<Vec<PathBuf>>,
}

impl AppState {
    pub fn new(config: Config, db: Db) -> Self {
        let permits = config.thumbnails.concurrency.max(1);
        let transcode_permits = config.video.transcode_concurrency.max(1);
        let trusted_proxies: Vec<IpNet> = config
            .server
            .trusted_proxies
            .iter()
            .filter_map(|raw| {
                let raw = raw.trim();
                if raw.is_empty() {
                    return None;
                }
                if let Ok(net) = raw.parse::<IpNet>() {
                    return Some(net);
                }
                if let Ok(ip) = raw.parse::<IpAddr>() {
                    let prefix = if ip.is_ipv4() { 32 } else { 128 };
                    return IpNet::new(ip, prefix).ok();
                }
                tracing::warn!(trusted_proxy = raw, "invalid trusted proxy CIDR");
                None
            })
            .collect();
        if config.auth.mode == "proxy" && trusted_proxies.is_empty() {
            tracing::warn!(
                "auth.mode = proxy but server.trusted_proxies is empty; proxy auth will reject requests"
            );
        }
        let libraries = match db.list_libraries() {
            Ok(rows) => rows.into_iter().map(|row| row.library).collect(),
            Err(error) => {
                tracing::warn!(%error, "failed to load libraries from database");
                Vec::new()
            }
        };
        let manage_roots: Vec<PathBuf> = config
            .manage
            .allowed_roots
            .iter()
            .filter_map(|raw| {
                let raw = raw.trim();
                if raw.is_empty() {
                    return None;
                }
                match std::fs::canonicalize(raw) {
                    Ok(path) if path.is_dir() => Some(path),
                    Ok(path) => {
                        tracing::warn!(root = %path.display(), "manage.allowed_roots entry is not a directory");
                        None
                    }
                    Err(error) => {
                        tracing::warn!(root = raw, %error, "cannot resolve manage.allowed_roots entry");
                        None
                    }
                }
            })
            .collect();
        Self {
            config: Arc::new(config),
            db,
            thumb_sem: Arc::new(Semaphore::new(permits)),
            transcode_sem: Arc::new(Semaphore::new(transcode_permits)),
            trusted_proxies: Arc::new(trusted_proxies),
            scanning: Arc::new(AtomicBool::new(false)),
            auth_cache: Arc::new(Mutex::new(HashMap::new())),
            libraries: Arc::new(RwLock::new(libraries)),
            manage_roots: Arc::new(manage_roots),
        }
    }

    pub fn is_trusted_proxy(&self, ip: IpAddr) -> bool {
        self.trusted_proxies.iter().any(|net| net.contains(&ip))
    }

    pub fn libraries_snapshot(&self) -> Vec<Library> {
        self.libraries.read().unwrap().clone()
    }

    pub fn library(&self, id: &str) -> Option<Library> {
        self.libraries
            .read()
            .unwrap()
            .iter()
            .find(|library| library.id == id)
            .cloned()
    }

    /// Reload the cached snapshot after a database mutation.
    pub fn reload_libraries(&self) {
        match self.db.list_libraries() {
            Ok(rows) => {
                *self.libraries.write().unwrap() =
                    rows.into_iter().map(|row| row.library).collect();
            }
            Err(error) => tracing::warn!(%error, "failed to reload libraries"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_proxy_matching() {
        let mut config = Config::default();
        config.server.trusted_proxies = vec![
            "127.0.0.1/32".into(),
            "::1/128".into(),
            "192.168.110.0/24".into(),
            "10.0.0.5".into(),
        ];
        let path =
            std::env::temp_dir().join(format!("photoself-state-{}-db", uuid::Uuid::now_v7()));
        let db = Db::open(&path).unwrap();
        let state = AppState::new(config, db);
        assert!(state.is_trusted_proxy("127.0.0.1".parse().unwrap()));
        assert!(state.is_trusted_proxy("192.168.110.20".parse().unwrap()));
        assert!(state.is_trusted_proxy("10.0.0.5".parse().unwrap()));
        assert!(!state.is_trusted_proxy("192.168.111.20".parse().unwrap()));
        assert!(!state.is_trusted_proxy("10.0.0.6".parse().unwrap()));
        let _ = std::fs::remove_file(path);
    }
}
