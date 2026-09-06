/// Resolves on SIGINT or (on Unix) SIGTERM (FR-6.4). Shared by `stream` and
/// `serve` — both need to flush/close and exit within the 10s budget.
pub async fn signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        let mut sig = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("installing SIGTERM handler");
        sig.recv().await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
