//! Origin connections: TCP, TLS, unix sockets, and TLS over unix.

use std::sync::Once;

use anyhow::{Context, Result};
use tokio::io::{AsyncRead, AsyncWrite};

pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncReadWrite for T {}

pub type BoxedIo = Box<dyn AsyncReadWrite>;

/// Make `path` free for a new unix listener (issues 202 and 115, and the
/// rule of podssh): a socket that answers belongs to another process and
/// is refused (exit 69), unless `replace`; a stale socket file is removed;
/// another kind of file is never removed (exit 64).
#[cfg(unix)]
pub fn claim_socket_path(path: &std::path::Path, replace: bool) -> Result<()> {
    use crate::exit::{fault, Fault};
    use std::os::unix::fs::FileTypeExt as _;
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_socket() => {
            if !replace && std::os::unix::net::UnixStream::connect(path).is_ok() {
                return Err(fault(
                    Fault::Unavailable,
                    anyhow::anyhow!(
                        "unix:{} is served by another process (podnet ps; podnet stop NAME, or --replace where the command has it)",
                        path.display()
                    ),
                ));
            }
            std::fs::remove_file(path)
                .with_context(|| format!("removing the old socket unix:{}", path.display()))?;
            Ok(())
        }
        Ok(_) => Err(fault(
            Fault::Usage,
            anyhow::anyhow!("unix:{} exists and is not a socket", path.display()),
        )),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("unix:{}", path.display())),
    }
}

/// Give a refused bind (`AddrInUse`, `PermissionDenied`, `AddrNotAvailable`
/// in the chain of `err`) the class `Fault::Unavailable`: exit 69.
pub fn bind_fault(err: anyhow::Error) -> anyhow::Error {
    use crate::exit::{fault, Fault};
    use std::io::ErrorKind::{AddrInUse, AddrNotAvailable, PermissionDenied};
    let refused = err.chain().any(|e| {
        e.downcast_ref::<std::io::Error>()
            .is_some_and(|io| matches!(io.kind(), AddrInUse | PermissionDenied | AddrNotAvailable))
    });
    if refused {
        fault(Fault::Unavailable, err)
    } else {
        err
    }
}

/// Bind a TCP listener. A refused bind exits 69.
pub async fn bind_tcp(addr: std::net::SocketAddr) -> Result<tokio::net::TcpListener> {
    tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding tcp://{addr}"))
        .map_err(bind_fault)
}

pub fn init_crypto() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_second_bind_on_one_port_exits_69() {
        pod_testkit::need_loopback!();
        let first = bind_tcp(([127, 0, 0, 1], 0).into()).await.unwrap();
        let err = bind_tcp(first.local_addr().unwrap()).await.unwrap_err();
        assert_eq!(crate::exit::code_of(&err), crate::exit::UNAVAILABLE);
    }

    #[test]
    fn a_claim_gives_69_for_a_live_socket_64_for_a_file_and_removes_a_stale_one() {
        use crate::exit::{code_of, UNAVAILABLE, USAGE};
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join("live.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&live).unwrap();
        let err = claim_socket_path(&live, false).unwrap_err();
        assert_eq!(code_of(&err), UNAVAILABLE);
        let file = dir.path().join("plain");
        std::fs::write(&file, b"x").unwrap();
        let err = claim_socket_path(&file, false).unwrap_err();
        assert_eq!(code_of(&err), USAGE);
        assert!(file.exists());
        let stale = dir.path().join("stale.sock");
        drop(std::os::unix::net::UnixListener::bind(&stale).unwrap());
        claim_socket_path(&stale, false).unwrap();
        assert!(!stale.exists());
    }
}
