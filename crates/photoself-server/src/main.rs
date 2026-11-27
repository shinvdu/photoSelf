mod api;
mod auth;
mod state;
mod ui;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use photoself_core::config::Config;
use photoself_core::db::Db;
use photoself_core::model::Library;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "photoself",
    version,
    about = "A lightweight self-hosted photo and video gallery"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the HTTP server (default).
    Serve {
        #[arg(short, long, default_value = "config.toml")]
        config: PathBuf,
    },
    /// Scan configured libraries and exit.
    Scan {
        #[arg(short, long, default_value = "config.toml")]
        config: PathBuf,
        /// Only scan this library id.
        #[arg(long)]
        library: Option<String>,
    },
    /// Print an Argon2id hash for auth.password_hash.
    HashPassword {
        #[arg(long)]
        password: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve {
        config: PathBuf::from("config.toml"),
    }) {
        Command::Serve { config } => serve(config).await,
        Command::Scan { config, library } => scan(config, library),
        Command::HashPassword { password } => hash_password(&password),
    }
}

fn init_logging(config: &Config) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.logging.level.clone()));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

async fn serve(config_path: PathBuf) -> Result<()> {
    let config = Config::load(&config_path)?;
    config.ensure_dirs()?;
    init_logging(&config);

    let db = Db::open(&config.db_path())?;
    db.seed_config_libraries(&config.libraries)?;

    let state = state::AppState::new(config.clone(), db);
    spawn_scan_scheduler(state.clone());
    if config.scan.on_start {
        if let Err(error) = api::start_scan(state.clone(), None) {
            tracing::warn!(%error, "startup scan not started");
        }
    }

    let library_count = state
        .libraries_snapshot()
        .iter()
        .filter(|lib| lib.enabled)
        .count();
    let app = api::router(state);
    let listener = tokio::net::TcpListener::bind(&config.server.listen)
        .await
        .with_context(|| format!("failed to bind {}", config.server.listen))?;

    tracing::info!(
        listen = %config.server.listen,
        mode = %config.server.mode,
        auth = %config.auth.mode,
        libraries = library_count,
        "photoSelf starting"
    );

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("server error")
}

fn scan(config_path: PathBuf, library_filter: Option<String>) -> Result<()> {
    let config = Config::load(&config_path)?;
    config.ensure_dirs()?;
    init_logging(&config);

    let db = Db::open(&config.db_path())?;
    db.seed_config_libraries(&config.libraries)?;

    let libraries: Vec<Library> = db
        .list_libraries()?
        .into_iter()
        .map(|row| row.library)
        .filter(|lib| lib.enabled)
        .filter(|lib| {
            library_filter
                .as_ref()
                .map(|id| id == &lib.id)
                .unwrap_or(true)
        })
        .collect();

    if libraries.is_empty() {
        anyhow::bail!("no matching libraries to scan");
    }

    for library in libraries {
        tracing::info!(library = %library.id, path = %library.root_path.display(), "scanning");
        let stats = photoself_core::scan::scan_library(&config, &db, &library, None)?;
        println!(
            "{}: scanned {}, errors {}, missing {}",
            library.id, stats.scanned, stats.errors, stats.missing
        );
    }
    Ok(())
}

fn hash_password(password: &str) -> Result<()> {
    use argon2::password_hash::{PasswordHasher, SaltString};
    use argon2::Argon2;
    use rand_core::OsRng;

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    println!("{hash}");
    Ok(())
}

fn parse_scan_interval(raw: &str) -> Option<Duration> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "0" {
        return None;
    }
    let (value, unit) = raw.split_at(raw.find(|character: char| !character.is_ascii_digit())?);
    let value: u64 = value.parse().ok()?;
    if value == 0 {
        return None;
    }
    let multiplier = match unit.trim().to_ascii_lowercase().as_str() {
        "s" | "sec" | "secs" => 1,
        "m" | "min" | "mins" => 60,
        "h" | "hr" | "hrs" => 60 * 60,
        "d" | "day" | "days" => 24 * 60 * 60,
        _ => return None,
    };
    Some(Duration::from_secs(value.checked_mul(multiplier)?))
}

fn spawn_scan_scheduler(state: state::AppState) {
    let Some(interval) = parse_scan_interval(&state.config.scan.interval) else {
        return;
    };
    tracing::info!(seconds = interval.as_secs(), "scheduled scans enabled");
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            match api::start_scan(state.clone(), None) {
                Ok(job) => tracing::info!(job = %job, "scheduled scan started"),
                Err(error) => tracing::debug!(%error, "scheduled scan skipped"),
            }
        }
    });
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_intervals() {
        assert_eq!(parse_scan_interval("0"), None);
        assert_eq!(parse_scan_interval("15m"), Some(Duration::from_secs(900)));
        assert_eq!(parse_scan_interval("1h"), Some(Duration::from_secs(3600)));
        assert_eq!(parse_scan_interval("2d"), Some(Duration::from_secs(172800)));
        assert_eq!(parse_scan_interval("15"), None);
        assert_eq!(parse_scan_interval("15x"), None);
        assert_eq!(parse_scan_interval("99999999999999999999d"), None);
    }
}
