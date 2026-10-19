//! Portable, verified offline instance snapshots. No HTTP maintenance surface.
use crate::{
    config::Config,
    instance_lock::{InstanceLock, reject_symlink, resolved_path},
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const DATABASE: &str = "database.sqlite3";
const MANIFEST: &str = "manifest.json";
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    format_version: u32,
    application_version: String,
    created_at: String,
    directories: BTreeSet<String>,
    files: BTreeMap<String, FileRecord>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct FileRecord {
    bytes: u64,
    sha256: String,
}

/// Hold both instance locks until all data and checksums have been published.
pub async fn create(config: &Config, destination: &Path) -> Result<()> {
    let instance = InstanceLock::acquire(config)?;
    ensure!(instance.database.is_file(), "Database does not exist");
    ensure!(
        instance.uploads.is_dir(),
        "Uploads directory does not exist"
    );
    reject_symlink(destination)?;
    let destination = resolved_path(destination)?;
    ensure!(!destination.exists(), "Backup destination already exists");
    ensure!(
        !destination.starts_with(&instance.uploads),
        "Backup destination must be outside uploads"
    );
    let parent = destination.parent().context("Backup requires a parent")?;
    fs::create_dir_all(parent)?;
    let stage = Stage::new(parent)?;
    let snapshot = stage.path.join(DATABASE);
    let mut source = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&instance.database)
            .read_only(true),
    )
    .await?;
    let vacuum_result = sqlx::query("VACUUM INTO ?")
        .bind(snapshot.to_str().context("Backup path must be UTF-8")?)
        .execute(&mut source)
        .await;
    source.close().await?;
    vacuum_result.context("Cannot create consistent SQLite snapshot")?;
    File::open(&snapshot)?.sync_all()?;
    check_database(&snapshot).await?;
    let mut manifest = Manifest {
        format: "sun-panel-instance-backup".into(),
        format_version: 1,
        application_version: env!("CARGO_PKG_VERSION").into(),
        created_at: chrono::Utc::now().to_rfc3339(),
        directories: BTreeSet::from(["uploads".into()]),
        files: BTreeMap::from([(DATABASE.into(), hash_file(&snapshot)?)]),
    };
    fs::create_dir(stage.path.join("uploads"))?;
    copy_uploads(
        &instance.uploads,
        &stage.path,
        Path::new("uploads"),
        &mut manifest,
    )?;
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    ensure!(
        manifest_bytes.len() as u64 <= MAX_MANIFEST_BYTES,
        "Backup manifest exceeds the supported restore limit"
    );
    let mut file = File::create_new(stage.path.join(MANIFEST))?;
    file.write_all(&manifest_bytes)?;
    file.sync_all()?;
    sync_directories(&stage.path, &manifest.directories)?;
    // No incomplete bundle is ever exposed under the requested backup name.
    ensure!(!destination.exists(), "Backup destination already exists");
    fs::rename(&stage.path, &destination)?;
    sync_directory(parent).with_context(|| {
        format!(
            "Backup was published at {}, but syncing its parent directory failed; the completed bundle has been retained",
            destination.display()
        )
    })?;
    tracing::info!(path = %destination.display(), files = manifest.files.len(), "Instance backup created");
    Ok(())
}

/// Restore into an empty configured instance, without migrating the backup.
pub async fn restore(config: &Config, source: &Path) -> Result<()> {
    let instance = InstanceLock::acquire(config)?;
    ensure_empty_target(&instance)?;
    reject_symlink(source)?;
    let source = source
        .canonicalize()
        .context("Backup directory does not exist")?;
    ensure!(source.is_dir(), "Backup source must be a directory");
    ensure!(
        !instance.database.starts_with(&source) && !instance.uploads.starts_with(&source),
        "Restore targets must be outside the backup directory"
    );
    let manifest = read_manifest(&source)?;
    validate_manifest(&manifest)?;
    validate_file_set(&source, &manifest)?;
    let database_parent = instance
        .database
        .parent()
        .context("Database parent missing")?;
    let uploads_parent = instance
        .uploads
        .parent()
        .context("Uploads parent missing")?;
    let database_stage = Stage::new(database_parent)?;
    let uploads_stage = Stage::new(uploads_parent)?;
    let staged_database = database_stage.path.join(DATABASE);
    let staged_uploads = uploads_stage.path.join("uploads");
    fs::create_dir(&staged_uploads)?;
    for directory in &manifest.directories {
        fs::create_dir_all(uploads_stage.path.join(directory))?;
    }
    for (path, expected) in &manifest.files {
        let target = if path == DATABASE {
            staged_database.clone()
        } else {
            uploads_stage.path.join(path)
        };
        let actual = copy_file(&source.join(path), &target)?;
        ensure!(&actual == expected, "Backup checksum mismatch: {path}");
    }
    check_database(&staged_database).await?;
    sync_directories(&uploads_stage.path, &manifest.directories)?;
    sync_directory(&database_stage.path)?;
    publish_staged(&instance, &staged_database, &staged_uploads)?;
    tracing::info!(database = %instance.database.display(), files = manifest.files.len(), "Instance backup restored");
    Ok(())
}

fn publish_staged(
    instance: &InstanceLock,
    staged_database: &Path,
    staged_uploads: &Path,
) -> Result<()> {
    let database_parent = instance
        .database
        .parent()
        .context("Database parent missing")?;
    let uploads_parent = instance
        .uploads
        .parent()
        .context("Uploads parent missing")?;
    // Check again after verification, before making either target visible.
    ensure_empty_target(instance)?;
    let had_uploads = instance.uploads.exists();
    if had_uploads {
        fs::remove_dir(&instance.uploads)?;
    }
    let mut publication = RestorePublication {
        database: &instance.database,
        uploads: &instance.uploads,
        had_uploads,
        published_database: false,
        published_uploads: false,
        complete: false,
    };
    if let Err(error) = fs::rename(staged_uploads, &instance.uploads) {
        return Err(error).context("Cannot publish restored uploads");
    }
    publication.published_uploads = true;
    // Publish the DB last. On an ordinary filesystem error remove only files
    // created by this restore, leaving the original empty instance retryable.
    // A hard link within the same filesystem atomically refuses an existing
    // target, even if a file appeared after the empty-target check.
    if let Err(error) = fs::hard_link(staged_database, &instance.database) {
        return Err(error).context("Cannot publish restored database");
    }
    publication.published_database = true;
    sync_directory(uploads_parent)?;
    sync_directory(database_parent)?;
    publication.complete = true;
    Ok(())
}

fn copy_uploads(
    root: &Path,
    destination: &Path,
    relative: &Path,
    manifest: &mut Manifest,
) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        let child = relative.join(entry.file_name());
        let name = portable_path(&child)?;
        if metadata.is_dir() {
            fs::create_dir(destination.join(&child))?;
            manifest.directories.insert(name);
            copy_uploads(&entry.path(), destination, &child, manifest)?;
        } else if metadata.is_file() {
            manifest
                .files
                .insert(name, copy_file(&entry.path(), &destination.join(&child))?);
        } else {
            bail!(
                "Uploads contains a symlink or special file: {}",
                entry.path().display()
            );
        }
    }
    Ok(())
}

fn ensure_empty_target(instance: &InstanceLock) -> Result<()> {
    for path in [
        instance.database.clone(),
        sqlite_sidecar(&instance.database, "-wal"),
        sqlite_sidecar(&instance.database, "-shm"),
        sqlite_sidecar(&instance.database, "-journal"),
    ] {
        ensure!(
            fs::symlink_metadata(&path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
            "Restore refuses to overwrite an existing database or sidecar: {}",
            path.display()
        );
    }
    if instance.uploads.exists() {
        ensure!(
            instance.uploads.is_dir(),
            "Uploads target is not a directory"
        );
        ensure!(
            fs::read_dir(&instance.uploads)?.next().is_none(),
            "Restore requires an empty uploads directory"
        );
    }
    Ok(())
}

fn sqlite_sidecar(database: &Path, suffix: &str) -> PathBuf {
    let mut value = database.as_os_str().to_owned();
    value.push(suffix);
    value.into()
}

fn read_manifest(source: &Path) -> Result<Manifest> {
    let path = source.join(MANIFEST);
    let metadata = fs::symlink_metadata(&path)?;
    ensure!(metadata.is_file(), "Manifest must be a regular file");
    ensure!(
        metadata.len() <= MAX_MANIFEST_BYTES,
        "Manifest is too large"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_MANIFEST_BYTES,
        "Manifest is too large"
    );
    serde_json::from_slice(&bytes).context("Invalid backup manifest")
}

fn validate_manifest(manifest: &Manifest) -> Result<()> {
    ensure!(
        manifest.format == "sun-panel-instance-backup" && manifest.format_version == 1,
        "Unsupported backup format/version"
    );
    ensure!(
        !manifest.application_version.is_empty(),
        "Missing application version"
    );
    chrono::DateTime::parse_from_rfc3339(&manifest.created_at)
        .context("Invalid backup creation time")?;
    ensure!(
        manifest.files.contains_key(DATABASE),
        "Backup is missing its database"
    );
    ensure!(
        manifest.directories.contains("uploads"),
        "Backup is missing uploads directory"
    );
    for path in manifest.directories.iter().chain(manifest.files.keys()) {
        safe_relative(path)?;
        ensure!(
            path == DATABASE || path == "uploads" || path.starts_with("uploads/"),
            "Unexpected backup path: {path}"
        );
        let mut parent = Path::new(path).parent();
        while let Some(value) = parent.filter(|p| !p.as_os_str().is_empty()) {
            ensure!(
                manifest
                    .directories
                    .contains(value.to_str().context("Invalid path")?),
                "Backup omits a parent directory: {path}"
            );
            parent = value.parent();
        }
    }
    for (path, record) in &manifest.files {
        ensure!(
            !manifest.directories.contains(path),
            "Path is both a file and a directory: {path}"
        );
        ensure!(
            record.sha256.len() == 64
                && record
                    .sha256
                    .bytes()
                    .all(|v| v.is_ascii_hexdigit() && !v.is_ascii_uppercase()),
            "Invalid SHA256 checksum: {path}"
        );
        ensure!(path != "uploads", "Uploads must be a directory");
    }
    ensure!(
        !manifest.directories.contains(DATABASE),
        "Database must be a file"
    );
    Ok(())
}

fn validate_file_set(source: &Path, manifest: &Manifest) -> Result<()> {
    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::new();
    collect_file_set(source, Path::new(""), &mut files, &mut directories)?;
    let mut expected_files = manifest.files.keys().cloned().collect::<BTreeSet<_>>();
    expected_files.insert(MANIFEST.into());
    ensure!(
        files == expected_files && directories == manifest.directories,
        "Backup file/directory set does not match its manifest"
    );
    Ok(())
}

fn collect_file_set(
    root: &Path,
    relative: &Path,
    files: &mut BTreeSet<String>,
    directories: &mut BTreeSet<String>,
) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let child = relative.join(entry.file_name());
        let name = portable_path(&child)?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.is_dir() {
            directories.insert(name);
            collect_file_set(&entry.path(), &child, files, directories)?;
        } else if metadata.is_file() {
            files.insert(name);
        } else {
            bail!(
                "Backup contains a symlink or special file: {}",
                entry.path().display()
            );
        }
    }
    Ok(())
}

fn portable_path(path: &Path) -> Result<String> {
    let path = path.to_str().context("Backup paths must be UTF-8")?;
    safe_relative(path)?;
    Ok(path.into())
}

fn safe_relative(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && !path.contains(['\\', ':', '\0'])
            && !path.ends_with('/')
            && !path.contains("//"),
        "Invalid backup path: {path}"
    );
    ensure!(
        Path::new(path)
            .components()
            .all(|p| matches!(p, Component::Normal(_))),
        "Backup paths must be relative and cannot traverse parents: {path}"
    );
    // Path::components normalizes an embedded ./; reject it for a single
    // canonical portable representation in the file list and checksum map.
    ensure!(
        !path.split('/').any(|p| p == "." || p == ".."),
        "Invalid backup path: {path}"
    );
    Ok(())
}

fn hash_file(path: &Path) -> Result<FileRecord> {
    let mut source = File::open(path)?;
    digest_stream(&mut source, None)
}

fn copy_file(source: &Path, destination: &Path) -> Result<FileRecord> {
    let metadata = fs::symlink_metadata(source)?;
    ensure!(
        metadata.is_file(),
        "Only regular files can be copied: {}",
        source.display()
    );
    let mut source = File::open(source)?;
    let mut destination = File::create_new(destination)?;
    let result = digest_stream(&mut source, Some(&mut destination))?;
    destination.sync_all()?;
    Ok(result)
}

fn digest_stream(source: &mut File, mut destination: Option<&mut File>) -> Result<FileRecord> {
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 65536];
    loop {
        let length = source.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        digest.update(&buffer[..length]);
        if let Some(target) = destination.as_deref_mut() {
            target.write_all(&buffer[..length])?;
        }
        bytes += length as u64;
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut sha256 = String::with_capacity(64);
    for value in digest.finalize() {
        sha256.push(char::from(HEX[usize::from(value >> 4)]));
        sha256.push(char::from(HEX[usize::from(value & 15)]));
    }
    Ok(FileRecord { bytes, sha256 })
}

async fn check_database(path: &Path) -> Result<()> {
    let mut connection =
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(path).read_only(true))
            .await?;
    let result = sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
        .fetch_all(&mut connection)
        .await;
    connection.close().await?;
    ensure!(result? == ["ok"], "SQLite integrity check failed");
    Ok(())
}

fn sync_directories(root: &Path, directories: &BTreeSet<String>) -> Result<()> {
    for directory in directories.iter().rev() {
        sync_directory(&root.join(directory))?;
    }
    sync_directory(root)
}

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

struct Stage {
    path: PathBuf,
}

struct RestorePublication<'a> {
    database: &'a Path,
    uploads: &'a Path,
    had_uploads: bool,
    published_database: bool,
    published_uploads: bool,
    complete: bool,
}

impl Drop for RestorePublication<'_> {
    fn drop(&mut self) {
        if self.complete {
            return;
        }
        if self.published_database
            && let Err(error) = fs::remove_file(self.database)
        {
            tracing::error!(%error, path = %self.database.display(), "Could not roll back restored database");
        }
        if self.published_uploads
            && let Err(error) = fs::remove_dir_all(self.uploads)
        {
            tracing::error!(%error, path = %self.uploads.display(), "Could not roll back restored uploads");
        }
        if self.had_uploads && !self.uploads.exists() {
            let _ = fs::create_dir(self.uploads);
        }
    }
}

impl Stage {
    fn new(parent: &Path) -> Result<Self> {
        let path = parent.join(format!(".sun-panel-stage-{}", uuid::Uuid::new_v4()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path)?;
        Ok(Self { path })
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_database_failure_rolls_back_uploads_and_leaves_target_retryable() {
        for had_uploads in [false, true] {
            let temporary = tempfile::tempdir().unwrap();
            let target = temporary.path().join("target");
            let config = Config {
                bind: "127.0.0.1:0".into(),
                database_path: target.join("database.db"),
                uploads_dir: target.join("uploads"),
                web_dir: target.join("web"),
                max_upload_bytes: 1024,
                monitor_interval_secs: 5,
                allow_private_favicon: false,
                worker_threads: 1,
                database_connections: 1,
            };
            let instance = InstanceLock::acquire(&config).unwrap();
            if had_uploads {
                fs::create_dir(&instance.uploads).unwrap();
            }
            let stage = Stage::new(&target).unwrap();
            let uploads = stage.path.join("uploads");
            fs::create_dir(&uploads).unwrap();
            fs::write(uploads.join("image.png"), b"staged-image").unwrap();
            // Force a genuine filesystem failure after uploads were published.
            let error =
                publish_staged(&instance, &stage.path.join("missing.db"), &uploads).unwrap_err();
            assert!(error.to_string().contains("publish restored database"));
            assert_eq!(instance.uploads.exists(), had_uploads);
            ensure_empty_target(&instance).unwrap();
            // Retry through the same publication path; no stale partial data.
            fs::create_dir(&uploads).unwrap();
            fs::write(uploads.join("image.png"), b"retry-image").unwrap();
            let database = stage.path.join("database.db");
            fs::write(&database, b"verified-before-publication").unwrap();
            publish_staged(&instance, &database, &uploads).unwrap();
            assert_eq!(
                fs::read(&instance.database).unwrap(),
                b"verified-before-publication"
            );
            assert_eq!(
                fs::read(instance.uploads.join("image.png")).unwrap(),
                b"retry-image"
            );
        }
    }
}
