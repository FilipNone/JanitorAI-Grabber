use super::handler::AppState;
use crate::config::Mode;
use crate::store::Store;
use std::net::SocketAddr;

/// Handle for a running proxy server. Dropping it does not stop the server; call `stop`.
pub struct ProxyHandle {
    pub addr: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

impl ProxyHandle {
    /// Stop accepting requests. In-flight requests may end early because the
    /// desktop app stops the server immediately when it quits.
    pub fn stop(self) {
        self.task.abort();
    }
}

/// Bind the proxy and run it in a background task on the current runtime.
pub async fn spawn(
    listen_addr: &str,
    mode: Mode,
    upstream_base: String,
    store: Store,
) -> anyhow::Result<ProxyHandle> {
    let addr: SocketAddr = listen_addr.parse()?;
    let state = AppState {
        upstream_base,
        mode,
        store,
    };

    let router = super::router::build_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;

    let task = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            tracing::error!(error = %e, "proxy server stopped with error");
        }
    });

    // Check that the listener accepts connections before returning.
    match tokio::net::TcpStream::connect(bound).await {
        Ok(probe) => drop(probe),
        Err(e) => tracing::warn!(error = %e, "listener probe failed (may still be starting)"),
    }

    tracing::info!(addr = %bound, "proxy listening");
    Ok(ProxyHandle { addr: bound, task })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn binds_ephemeral_and_shuts_down() {
        let tmp = tempdir().unwrap();
        let store = Store::open(&tmp.path().join("t.db")).await.unwrap();
        let handle = spawn(
            "127.0.0.1:0",
            Mode::Forward,
            "http://127.0.0.1:1".into(),
            store,
        )
        .await
        .unwrap();
        assert_ne!(handle.addr.port(), 0);
        handle.stop();
    }
}
