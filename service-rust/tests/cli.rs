use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Output},
    thread,
};
use tempfile::TempDir;

fn command(directory: &TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_sun-panel-rust"));
    for key in [
        "SUN_PANEL_DATABASE",
        "SUN_PANEL_UPLOADS",
        "SUN_PANEL_WEB_DIR",
        "SUN_PANEL_MAX_UPLOAD_BYTES",
        "SUN_PANEL_MONITOR_INTERVAL",
        "SUN_PANEL_WORKER_THREADS",
        "SUN_PANEL_DB_CONNECTIONS",
        "SUN_PANEL_ALLOW_PRIVATE_FAVICON",
    ] {
        command.env_remove(key);
    }
    command.env("SUN_PANEL_DATA_DIR", directory.path().join("data"));
    command.env("SUN_PANEL_BIND", "127.0.0.1:0");
    command
}

#[test]
fn help_and_invalid_arguments_never_initialize_storage() {
    let directory = TempDir::new().unwrap();
    for argument in ["--help", "-h"] {
        let output = command(&directory).arg(argument).output().unwrap();
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("--healthcheck"));
        assert!(help.contains("backup <new-directory>"));
        assert!(help.contains("restore <backup-directory>"));
    }
    for arguments in [vec!["unknown"], vec!["backup"], vec!["serve", "extra"]] {
        let output = command(&directory).args(arguments).output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Invalid arguments"));
    }
    assert!(!directory.path().join("data").exists());
}

#[test]
fn invalid_resource_settings_fail_before_storage_is_created() {
    let directory = TempDir::new().unwrap();
    for (key, values) in [
        (
            "SUN_PANEL_MAX_UPLOAD_BYTES",
            ["invalid", "1023", "67108865"],
        ),
        ("SUN_PANEL_MONITOR_INTERVAL", ["invalid", "0", "301"]),
        ("SUN_PANEL_WORKER_THREADS", ["invalid", "0", "65"]),
        ("SUN_PANEL_DB_CONNECTIONS", ["invalid", "0", "17"]),
    ] {
        for value in values {
            let output = command(&directory)
                .arg("serve")
                .env(key, value)
                .output()
                .unwrap();
            assert!(!output.status.success(), "{key}={value}");
            assert!(String::from_utf8_lossy(&output.stderr).contains(key));
        }
    }
    assert!(!directory.path().join("data").exists());
}

fn healthcheck(response: &'static str) -> Output {
    let directory = TempDir::new().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = [0; 4096];
        let bytes = stream.read(&mut request).unwrap();
        assert!(request[..bytes].starts_with(b"GET /api/health HTTP/1.1\r\n"));
        stream.write_all(response.as_bytes()).unwrap();
    });
    let output = command(&directory)
        .arg("--healthcheck")
        // Docker binds all interfaces, so the probe must connect via loopback.
        .env("SUN_PANEL_BIND", format!("0.0.0.0:{port}"))
        .output()
        .unwrap();
    server.join().unwrap();
    assert!(!directory.path().join("data").exists());
    output
}

#[test]
fn healthcheck_requires_success_status_and_healthy_payload() {
    let healthy = healthcheck(
        "HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"status\":\"ok\"}",
    );
    assert!(
        healthy.status.success(),
        "{}",
        String::from_utf8_lossy(&healthy.stderr)
    );
    for response in [
        "HTTP/1.1 503 Unavailable\r\nConnection: close\r\n\r\n{\"status\":\"ok\"}",
        "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{\"status\":\"failed\"}",
    ] {
        let output = healthcheck(response);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("health check failed"));
    }
}

#[test]
fn healthcheck_rejects_invalid_bind_without_creating_storage() {
    let directory = TempDir::new().unwrap();
    let output = command(&directory)
        .arg("--healthcheck")
        .env("SUN_PANEL_BIND", "invalid-bind")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!directory.path().join("data").exists());
}
