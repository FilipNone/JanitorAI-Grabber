//! JanitorAI Grabber — local LLM proxy that captures chat-completion traffic.

mod config;
mod proxy;
mod store;
mod ui;

use config::Config;
use store::Store;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cfg = Config::load();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let data_dir = cfg.data_dir();
    let db_path = data_dir.join("captures.db");
    let store = rt.block_on(Store::open(&db_path))?;
    tracing::info!(db = %db_path.display(), "store ready");

    // Keep the runtime alive while the UI runs on the main thread.
    let ui_rt = rt.handle().clone();
    let result = ui::run(cfg, store, ui_rt);
    rt.shutdown_timeout(std::time::Duration::from_secs(2));
    result
}
