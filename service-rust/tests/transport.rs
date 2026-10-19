use serde_json::Value;
use sun_panel_rust::{app, config::Config, initialize};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn fragmented_post_bodies_preserve_http_keep_alive() {
    let dir = TempDir::new().unwrap();
    let state = initialize(Config {
        bind: "127.0.0.1:0".into(),
        database_path: dir.path().join("db.sqlite"),
        uploads_dir: dir.path().join("uploads"),
        web_dir: dir.path().join("web"),
        max_upload_bytes: 1024,
        monitor_interval_secs: 5,
        allow_private_favicon: false,
        worker_threads: 2,
        database_connections: 2,
    })
    .await
    .unwrap();
    let router = app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    socket.set_nodelay(true).unwrap();
    for _ in 0..3 {
        socket.write_all(b"POST /api/panel/navigation HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: keep-alive\r\n\r\n").await.unwrap();
        // Headers and body can arrive in separate packets; a read-only handler
        // must consume the body before this connection is reused.
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        socket.write_all(b"{}").await.unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            let read =
                tokio::time::timeout(std::time::Duration::from_secs(3), socket.read(&mut byte))
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(read, 1, "connection closed before a complete response");
            headers.push(byte[0]);
            assert!(headers.len() < 16384);
        }
        let headers = String::from_utf8(headers).unwrap().to_ascii_lowercase();
        assert!(headers.starts_with("http/1.1 200"), "{headers}");
        let length: usize = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let mut body = vec![0; length];
        socket.read_exact(&mut body).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["code"], 0);
    }
    drop(socket);
    let _ = stop.send(());
    server.await.unwrap();
    state.db.close().await;
}
