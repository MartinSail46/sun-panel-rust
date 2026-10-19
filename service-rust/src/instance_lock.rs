//! Lifetime-scoped OS locks shared by the server and offline maintenance commands.
use crate::config::Config;
use anyhow::{Context, Result, bail};
use std::{
    ffi::OsString,
    fs::{self, File},
    path::{Path, PathBuf},
};

pub struct InstanceLock {
    _files: Vec<File>,
    pub database: PathBuf,
    pub uploads: PathBuf,
}

impl InstanceLock {
    pub fn acquire(config: &Config) -> Result<Self> {
        reject_symlink(&config.database_path)?;
        reject_symlink(&config.uploads_dir)?;
        let database = resolved_path(&config.database_path)?;
        let uploads = resolved_path(&config.uploads_dir)?;
        if database.starts_with(&uploads) || uploads.starts_with(&database) {
            bail!("Database and uploads paths must not overlap");
        }
        let mut paths = vec![lock_path(&database)?, lock_path(&uploads)?];
        paths.sort();
        paths.dedup();
        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            fs::create_dir_all(path.parent().context("Lock file requires a parent")?)?;
            reject_symlink(&path)?;
            let mut options = File::options();
            options.read(true).write(true).create(true).truncate(false);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let file = options.open(&path)?;
            file.try_lock().with_context(|| {
                format!(
                    "Instance is in use or cannot be locked ({}); stop the server before offline backup/restore",
                    path.display()
                )
            })?;
            files.push(file);
        }
        Ok(Self {
            _files: files,
            database,
            uploads,
        })
    }
}

fn lock_path(resource: &Path) -> Result<PathBuf> {
    let mut name = OsString::from(".");
    name.push(resource.file_name().context("Data path must have a name")?);
    name.push(".sun-panel.lock");
    Ok(resource
        .parent()
        .context("Data path must have a parent")?
        .join(name))
}

pub(crate) fn reject_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            bail!("Symlink is not permitted: {}", path.display())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Resolve existing ancestors so aliases share a lock even before data exists.
pub(crate) fn resolved_path(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    if absolute.exists() {
        return absolute.canonicalize().map_err(Into::into);
    }
    let parent = absolute.parent().context("Data path must have a parent")?;
    let name = absolute.file_name().context("Data path must have a name")?;
    Ok(resolved_path(parent)?.join(name))
}
