use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use photoself_core::{Config, Db};
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Db,
    pub thumb_sem: Arc<Semaphore>,
    pub transcode_sem: Arc<Semaphore>,
    pub scanning: Arc<AtomicBool>,
    pub auth_cache: Arc<Mutex<HashMap<String, Instant>>>,
}

impl AppState {
    pub fn new(config: Config, db: Db) -> Self {
        let permits = config.thumbnails.concurrency.max(1);
        let transcode_permits = config.video.transcode_concurrency.max(1);
        Self {
            config: Arc::new(config),
            db,
            thumb_sem: Arc::new(Semaphore::new(permits)),
            transcode_sem: Arc::new(Semaphore::new(transcode_permits)),
            scanning: Arc::new(AtomicBool::new(false)),
            auth_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}
