use std::io;
use std::io::Seek;
use std::path::Path;
use std::path::PathBuf;

/// Spool uploads on disk instead of holding a complete tar in mobile RAM.
/// Anonymous files are removed when their last handle is closed.
pub(crate) fn temporary_file() -> io::Result<std::fs::File> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No user cache directory"))?
        .join("pods/archives");
    std::fs::create_dir_all(&cache)?;
    tempfile::tempfile_in(cache)
}

pub(crate) fn directory(path: &Path) -> io::Result<tokio::fs::File> {
    let mut builder = tar::Builder::new(temporary_file()?);
    builder.append_dir_all(".", path)?;
    builder.finish()?;
    let mut file = builder.into_inner()?;
    file.rewind()?;
    Ok(tokio::fs::File::from_std(file))
}
