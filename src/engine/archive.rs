use std::io;
use std::path::Path;
use std::path::PathBuf;

use tokio::io::AsyncSeekExt;

/// Use the user's cache filesystem, since /tmp is commonly backed by RAM on phones.
/// The anonymous file is removed automatically when a completed or cancelled upload closes it.
pub(crate) async fn temporary_file() -> io::Result<tokio::fs::File> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No user cache directory"))?
        .join("pods")
        .join("archives");

    tokio::fs::create_dir_all(&cache).await?;
    let file = tokio::task::spawn_blocking(move || tempfile::tempfile_in(cache))
        .await
        .map_err(io::Error::other)??;
    Ok(tokio::fs::File::from_std(file))
}

pub(crate) async fn directory(path: &Path) -> io::Result<tokio::fs::File> {
    let mut archive = tokio_tar::Builder::new(temporary_file().await?);
    archive.append_dir_all(".", path).await?;
    let mut file = archive.into_inner().await?;
    file.rewind().await?;
    Ok(file)
}
