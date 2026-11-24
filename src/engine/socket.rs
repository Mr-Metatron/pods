use std::fmt;
use std::io;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::path::PathBuf;

use crate::gettext::gettext;
use crate::gettext::gettext_f;

const SYSTEM_DOCKER_SOCKET: &str = "unix:///var/run/docker.sock";

/// Resolve the local Docker choice without redirecting saved connections.
/// Remote DOCKER_HOST values belong in the explicit custom connection field.
pub(crate) fn docker_socket_url() -> String {
    docker_socket_url_from(
        std::env::var("DOCKER_HOST").ok().as_deref(),
        std::env::var_os("XDG_RUNTIME_DIR")
            .as_deref()
            .map(Path::new),
    )
}

fn docker_socket_url_from(host: Option<&str>, runtime_dir: Option<&Path>) -> String {
    if let Some(host) = host.filter(|host| unix_socket_path(host).is_some()) {
        return host.to_owned();
    }

    if let Some(path) = runtime_dir.map(|dir| dir.join("docker.sock"))
        && path.is_absolute()
        && is_socket(&path)
    {
        return format!("unix://{}", path.display());
    }

    SYSTEM_DOCKER_SOCKET.to_owned()
}

pub(crate) fn socket_exists(url: &str) -> bool {
    unix_socket_path(url).is_some_and(is_socket)
}

fn unix_socket_path(url: &str) -> Option<&Path> {
    url.strip_prefix("unix://")
        .map(Path::new)
        .filter(|path| path.is_absolute())
}

fn is_socket(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|metadata| metadata.file_type().is_socket())
}

/// Probe once before the HTTP client wraps errno in a transport error. This
/// keeps missing sockets, access denials and inactive services distinguishable.
pub(crate) async fn check_local_socket(url: &str) -> Result<(), SocketError> {
    let Some(path) = unix_socket_path(url) else {
        return Ok(());
    };

    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|source| SocketError::new(path, source))?;
    if !metadata.file_type().is_socket() {
        return Err(SocketError::new(
            path,
            io::Error::new(io::ErrorKind::InvalidInput, "not a Unix socket"),
        ));
    }

    tokio::net::UnixStream::connect(path)
        .await
        .map(drop)
        .map_err(|source| SocketError::new(path, source))
}

#[derive(Debug)]
pub(crate) struct SocketError {
    path: PathBuf,
    source: io::Error,
}

impl SocketError {
    fn new(path: &Path, source: io::Error) -> Self {
        Self {
            path: path.to_owned(),
            source,
        }
    }
}

impl fmt::Display for SocketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let help = match self.source.kind() {
            io::ErrorKind::PermissionDenied => gettext(
                "Access was denied. Ask an administrator to grant your user access to this socket. For a system Docker service, membership in the docker group grants root-level control of the device. Log out and back in after changing group membership. Keep Pods running as your regular user.",
            ),
            io::ErrorKind::NotFound => gettext(
                "The socket was not found. Start the container engine service or choose its actual socket path. Rootless Docker normally uses $XDG_RUNTIME_DIR/docker.sock.",
            ),
            io::ErrorKind::ConnectionRefused => gettext(
                "The socket exists, but the service is not accepting connections. Start or restart the container engine service.",
            ),
            io::ErrorKind::InvalidInput => gettext(
                "The selected path is not a Unix socket. Choose the socket created by the container engine.",
            ),
            _ => self.source.to_string(),
        };

        write!(
            f,
            "{}\n\n{help}",
            gettext_f(
                "Could not connect to the Unix socket “{}”.",
                [self.path.display()],
            ),
        )
    }
}

impl std::error::Error for SocketError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixListener;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use super::*;

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "pods-socket-{}-{}",
                std::process::id(),
                NEXT_DIR.fetch_add(1, Ordering::Relaxed),
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn discovers_rootless_socket_and_respects_explicit_local_host() {
        let dir = TestDir::new();
        let path = dir.0.join("docker.sock");
        let _listener = UnixListener::bind(&path).unwrap();

        assert_eq!(
            docker_socket_url_from(None, Some(&dir.0)),
            format!("unix://{}", path.display()),
        );
        assert_eq!(
            docker_socket_url_from(Some("unix:///custom/docker.sock"), Some(&dir.0)),
            "unix:///custom/docker.sock",
        );
        assert_eq!(
            docker_socket_url_from(Some("tcp://remote.example:2375"), Some(&dir.0)),
            format!("unix://{}", path.display()),
        );
    }

    #[test]
    fn ignores_files_and_relative_or_remote_hosts_for_local_choice() {
        let dir = TestDir::new();
        std::fs::write(dir.0.join("docker.sock"), "not a socket").unwrap();

        for host in [None, Some("ssh://example"), Some("unix://relative.sock")] {
            assert_eq!(
                docker_socket_url_from(host, Some(&dir.0)),
                SYSTEM_DOCKER_SOCKET,
            );
        }
        assert!(!socket_exists("http://example/docker.sock"));
        assert!(!socket_exists(&format!(
            "unix://{}",
            dir.0.join("docker.sock").display(),
        )));
    }

    #[test]
    fn preserves_socket_failure_kinds() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let dir = TestDir::new();
            let path = dir.0.join("docker.sock");
            let url = format!("unix://{}", path.display());

            let err = check_local_socket(&url).await.unwrap_err();
            assert_eq!(err.source.kind(), io::ErrorKind::NotFound);
            assert!(err.to_string().contains(path.to_str().unwrap()));

            std::fs::write(&path, "not a socket").unwrap();
            assert_eq!(
                check_local_socket(&url).await.unwrap_err().source.kind(),
                io::ErrorKind::InvalidInput,
            );
            std::fs::remove_file(&path).unwrap();

            let listener = UnixListener::bind(&path).unwrap();
            assert!(socket_exists(&url));
            check_local_socket(&url).await.unwrap();
            drop(listener);
            assert_eq!(
                check_local_socket(&url).await.unwrap_err().source.kind(),
                io::ErrorKind::ConnectionRefused,
            );
        });
    }

    #[test]
    fn permission_diagnostic_is_actionable_and_retains_io_source() {
        use std::error::Error;

        let err = SocketError::new(
            Path::new("/run/docker.sock"),
            io::Error::from(io::ErrorKind::PermissionDenied),
        );
        assert!(err.to_string().contains("/run/docker.sock"));
        assert!(err.to_string().contains("docker group"));
        assert_eq!(
            err.source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::PermissionDenied,
        );
    }

    #[test]
    #[ignore = "run as an unprivileged user to exercise Unix socket permissions"]
    fn denied_socket_retains_permission_denied_errno() {
        use std::os::unix::fs::PermissionsExt;

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let dir = TestDir::new();
            let path = dir.0.join("docker.sock");
            let _listener = UnixListener::bind(&path).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o0)).unwrap();

            let result = check_local_socket(&format!("unix://{}", path.display())).await;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert_eq!(
                result.unwrap_err().source.kind(),
                io::ErrorKind::PermissionDenied
            );
        });
    }
}
