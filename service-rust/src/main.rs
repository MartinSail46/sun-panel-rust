use axum::serve::ListenerExt;
use sun_panel_rust::config::Config;
use sun_panel_rust::instance_lock::InstanceLock;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sun_panel_rust=info,tower_http=warn".into()),
        )
        .init();
    let config = Config::from_env()?;
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.as_slice() == ["--healthcheck"] {
        return healthcheck(&config.bind);
    }
    if arguments.as_slice() == ["--help"] || arguments.as_slice() == ["-h"] {
        println!(
            "sun-panel-rust [serve | --healthcheck | backup <new-directory> | restore <backup-directory>]\n\nOffline maintenance uses SUN_PANEL_DATA_DIR, SUN_PANEL_DATABASE and SUN_PANEL_UPLOADS.\nStop the server first; restore only accepts an empty target."
        );
        return Ok(());
    }
    let command = match arguments.as_slice() {
        [] => None,
        [command] if command == "serve" => None,
        [command, path] if command == "backup" || command == "restore" => {
            Some((command.clone(), std::path::PathBuf::from(path)))
        }
        _ => anyhow::bail!("Invalid arguments; use --help"),
    };
    // Acquire before migrations or uploads initialization, and retain until
    // graceful shutdown has drained requests and closed the database pool.
    let _instance_lock = if command.is_none() {
        Some(InstanceLock::acquire(&config)?)
    } else {
        None
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(config.worker_threads)
        .max_blocking_threads(8)
        .thread_name("sun-panel")
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        if let Some((command, path)) = command {
            if command == "backup" {
                return sun_panel_rust::backup::create(&config, &path).await;
            }
            return sun_panel_rust::backup::restore(&config, &path).await;
        }
        let state = sun_panel_rust::initialize(config).await?;
        let listener = tokio::net::TcpListener::bind(&state.config.bind).await?;
        tracing::info!(address = %listener.local_addr()?, database = %state.config.database_path.display(), "Rust backend ready");
        let listener = listener.tap_io(|stream| {
            if let Err(error) = stream.set_nodelay(true) {
                tracing::debug!(%error, "Could not enable TCP_NODELAY");
            }
        });
        axum::serve(listener, sun_panel_rust::app(state.clone())).with_graceful_shutdown(shutdown()).await?;
        state.db.close().await;
        Ok(())
    })
}

fn healthcheck(bind: &str) -> anyhow::Result<()> {
    use std::io::{Read, Write};
    let mut address: std::net::SocketAddr = bind.parse()?;
    if address.ip().is_unspecified() {
        address.set_ip(if address.is_ipv4() {
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        } else {
            std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
        });
    }
    let timeout = std::time::Duration::from_secs(3);
    let mut stream = std::net::TcpStream::connect_timeout(&address, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream
        .write_all(b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut response = String::new();
    stream.take(16384).read_to_string(&mut response)?;
    if !response.starts_with("HTTP/1.1 200") || !response.contains("\"status\":\"ok\"") {
        anyhow::bail!("Backend health check failed");
    }
    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
