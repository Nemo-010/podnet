//! The service `exec` of a stream of a link (T-296).
//!
//! The listener runs one program for each stream. The stdin and the stdout
//! of the program are the stream. The stderr of the program goes to the
//! log, never to the stream. The peer is on the allow list before the
//! stream opens.
//!
//! Rules:
//!
//! * The program is an absolute path to a file. There is no shell: a
//!   program named `sh`, `bash` and the like is refused, and the arguments
//!   are an array (no word splitting, no expansion).
//! * The environment is empty except the names of the allow list
//!   (`--exec-env`). A name of the list takes its value from the listener,
//!   and is left out when the listener has none. The working directory is
//!   `/`.
//! * `user` (`--exec-user`) needs root. Without root the setup exits 64.
//!   The program runs with the uid and gid of the user and with no
//!   supplementary groups.
//! * At most [`MAX_RUNNING`] programs run at one time for the listener. A
//!   stream over the limit gets the answer 4 (resource limit).
//! * When the program ends its stdout, the stream ends. When the peer ends
//!   its side, the stdin of the program closes. A stream that is reset
//!   kills the program.

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::exit::{self, Fault};
use crate::link_service as service;

/// The prefix of the target in `--service NAME=exec:/ABS/PATH`.
pub const PREFIX: &str = "exec:";

/// The prefix of the framed form (T-403): `--service NAME=exec-status:/ABS/PATH`.
/// After the answer of the service, the listener sends records: a type
/// byte, a length of 4 bytes (big endian), the bytes. Type [`STDOUT`] and
/// [`STDERR`] carry the output; type [`STATUS`] (4 bytes, a signed exit
/// code; 128+N for the signal N) is the last record. The stdin of the
/// program is the bytes of the dialer, as for `exec:`.
pub const STATUS_PREFIX: &str = "exec-status:";

/// The record types of `exec-status:`.
pub const STDOUT: u8 = 1;
pub const STDERR: u8 = 2;
pub const STATUS: u8 = 3;

/// The most bytes of one record.
pub const RECORD_LIMIT: usize = 64 * 1024;

/// The most programs that run at one time.
pub const MAX_RUNNING: usize = 64;

/// The most arguments of a program.
pub const MAX_ARGS: usize = 64;

/// The most bytes of one argument.
pub const ARG_LIMIT: usize = 4096;

/// The most names of the environment allow list.
pub const MAX_ENV: usize = 32;

/// The time that a program has to end after it closed its stdout.
const END_LIMIT: Duration = Duration::from_secs(10);

/// The names that are a shell. A program with such a file name is refused.
const SHELLS: &[&str] = &[
    "sh", "bash", "dash", "zsh", "ash", "ksh", "mksh", "csh", "tcsh", "fish", "busybox",
];

/// A program of a service, checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    program: PathBuf,
    args: Vec<String>,
    env: Vec<String>,
    user: Option<Account>,
    /// `exec-status:`: the framed form.
    framed: bool,
}

/// A user, resolved when the listener starts.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Account {
    name: String,
    uid: u32,
    gid: u32,
}

fn usage(text: String) -> anyhow::Error {
    exit::fault(Fault::Usage, anyhow!(text))
}

impl Spec {
    /// Check a program, its arguments, the environment allow list and the
    /// user. A fault exits 64.
    pub fn new(
        program: &str,
        args: Vec<String>,
        env: Vec<String>,
        user: Option<&str>,
    ) -> Result<Spec> {
        let path = Path::new(program);
        if !path.is_absolute() {
            return Err(usage(format!(
                "exec {program:?}: the program must be an absolute path"
            )));
        }
        if path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(usage(format!("exec {program:?}: no `..` in the path")));
        }
        let file = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if SHELLS.contains(&file) {
            return Err(usage(format!(
                "exec {program:?}: a shell is no program of this service (no shell runs here)"
            )));
        }
        if !std::fs::metadata(path)
            .map(|m| m.is_file())
            .unwrap_or(false)
        {
            return Err(usage(format!("exec {program:?}: no such file")));
        }
        if args.len() > MAX_ARGS || args.iter().any(|a| a.len() > ARG_LIMIT) {
            return Err(usage(format!(
                "exec {program:?}: {MAX_ARGS} arguments of {ARG_LIMIT} bytes at most"
            )));
        }
        if env.len() > MAX_ENV {
            return Err(usage(format!(
                "exec {program:?}: {MAX_ENV} environment names at most"
            )));
        }
        for name in &env {
            let good = !name.is_empty()
                && !name.starts_with(|c: char| c.is_ascii_digit())
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !good {
                return Err(usage(format!(
                    "exec {program:?}: {name:?} is no environment name"
                )));
            }
        }
        let user = user.map(account).transpose()?;
        Ok(Spec {
            program: path.to_path_buf(),
            args,
            env,
            user,
            framed: false,
        })
    }

    /// The framed form `exec-status:` (T-403).
    pub fn framed(mut self) -> Spec {
        self.framed = true;
        self
    }

    /// The command: no inherited environment beyond the allow list.
    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args)
            .env_clear()
            .current_dir("/")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for name in &self.env {
            if let Some(value) = std::env::var_os(name) {
                cmd.env(name, value);
            }
        }
        if let Some(Account { uid, gid, .. }) = self.user {
            // SAFETY: setgroups, setgid and setuid are async-signal-safe,
            // and the closure allocates nothing. The order drops the
            // groups while the process is still root.
            unsafe {
                cmd.pre_exec(move || {
                    if libc::setgroups(0, std::ptr::null()) != 0
                        || libc::setgid(gid) != 0
                        || libc::setuid(uid) != 0
                    {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        cmd
    }
}

impl std::fmt::Display for Spec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{PREFIX}{}", self.program.display())
    }
}

/// The uid and gid of a user. Only root may name a user.
fn account(name: &str) -> Result<Account> {
    // SAFETY: geteuid has no precondition.
    if unsafe { libc::geteuid() } != 0 {
        return Err(usage(format!(
            "--exec-user {name}: a user needs root (this process has no privilege)"
        )));
    }
    let c = CString::new(name).map_err(|_| usage(format!("--exec-user {name:?}: bad name")))?;
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buf = vec![0u8; 4096];
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: the pointers stay valid for the call.
    let rc = unsafe {
        libc::getpwnam_r(
            c.as_ptr(),
            &mut pwd,
            buf.as_mut_ptr().cast(),
            buf.len(),
            &mut found,
        )
    };
    if rc != 0 || found.is_null() {
        return Err(usage(format!("--exec-user {name}: no such user")));
    }
    Ok(Account {
        name: name.to_string(),
        uid: pwd.pw_uid,
        gid: pwd.pw_gid,
    })
}

fn slots() -> &'static std::sync::Arc<Semaphore> {
    static SLOTS: OnceLock<std::sync::Arc<Semaphore>> = OnceLock::new();
    SLOTS.get_or_init(|| std::sync::Arc::new(Semaphore::new(MAX_RUNNING)))
}

/// A started program that has no stream yet.
pub struct Running {
    child: Child,
    service: String,
    framed: bool,
    _slot: OwnedSemaphorePermit,
}

/// Start the program for one stream. The error carries the answer for the
/// opener ([`service::RESOURCE_LIMIT`] or [`service::NO_ORIGIN`]).
pub fn start(name: &str, spec: &Spec) -> Result<Running, (u8, anyhow::Error)> {
    let slot = slots().clone().try_acquire_owned().map_err(|_| {
        (
            service::RESOURCE_LIMIT,
            anyhow!("{MAX_RUNNING} programs run already for the service {name:?}"),
        )
    })?;
    let child = spec.command().spawn().map_err(|e| {
        (
            service::NO_ORIGIN,
            anyhow!("starting {}: {e}", spec.program.display()),
        )
    })?;
    Ok(Running {
        child,
        service: name.to_string(),
        framed: spec.framed,
        _slot: slot,
    })
}

/// Log the stderr of the program, one record for each line.
async fn log_stderr<R: AsyncRead + Unpin>(mut read: R, name: String) {
    let mut buf = [0u8; 4096];
    while let Ok(n) = read.read(&mut buf).await {
        if n == 0 {
            break;
        }
        for line in String::from_utf8_lossy(&buf[..n]).lines() {
            tracing::info!(service = name, "exec stderr: {line}");
        }
    }
}

impl Running {
    /// Copy `stream` to the stdin of the program and its stdout to
    /// `stream`. It returns when the program closed its stdout and ended.
    pub async fn pump<S>(mut self, stream: S) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send,
    {
        if self.framed {
            return self.pump_framed(stream).await;
        }
        let mut stdin = self.child.stdin.take().context("no stdin")?;
        let mut stdout = self.child.stdout.take().context("no stdout")?;
        let stderr = self.child.stderr.take().context("no stderr")?;
        let log = tokio::spawn(log_stderr(stderr, self.service.clone()));
        let (mut read, mut write) = tokio::io::split(stream);
        let up = async {
            // A program that ended its stdin early ends the copy: not a fault.
            let _ = tokio::io::copy(&mut read, &mut stdin).await;
            drop(stdin);
            std::future::pending::<()>().await;
        };
        let down = async {
            let copied = tokio::io::copy(&mut stdout, &mut write).await;
            let _ = write.shutdown().await;
            copied
        };
        let copied = tokio::select! {
            copied = down => copied,
            () = up => unreachable!("the copy to the program never ends"),
        };
        let status = match tokio::time::timeout(END_LIMIT, self.child.wait()).await {
            Ok(status) => status?,
            Err(_) => {
                let _ = self.child.kill().await;
                self.child.wait().await?
            }
        };
        let _ = tokio::time::timeout(Duration::from_secs(1), log).await;
        copied?;
        if !status.success() {
            tracing::warn!(service = self.service, %status, "the program of a stream ended with a fault");
        }
        Ok(())
    }
}

impl Running {
    /// The framed form: stdout and stderr as records, then the status.
    async fn pump_framed<S>(mut self, stream: S) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send,
    {
        let mut stdin = self.child.stdin.take().context("no stdin")?;
        let mut stdout = self.child.stdout.take().context("no stdout")?;
        let mut stderr = self.child.stderr.take().context("no stderr")?;
        let (mut read, write) = tokio::io::split(stream);
        let write = tokio::sync::Mutex::new(write);
        let up = async {
            let _ = tokio::io::copy(&mut read, &mut stdin).await;
            drop(stdin);
            std::future::pending::<()>().await;
        };
        let out = async {
            tokio::try_join!(
                records(&mut stdout, STDOUT, &write),
                records(&mut stderr, STDERR, &write)
            )
        };
        let copied = tokio::select! {
            copied = out => copied,
            () = up => unreachable!("the copy to the program never ends"),
        };
        let status = match tokio::time::timeout(END_LIMIT, self.child.wait()).await {
            Ok(status) => status?,
            Err(_) => {
                let _ = self.child.kill().await;
                self.child.wait().await?
            }
        };
        copied?;
        let code = status_code(&status);
        let mut write = write.into_inner();
        write_record(&mut write, STATUS, &code.to_be_bytes()).await?;
        let _ = write.shutdown().await;
        if code != 0 {
            tracing::info!(
                service = self.service,
                code,
                "the program of a stream ended; the peer gets the status"
            );
        }
        Ok(())
    }
}

/// The exit code of a status: the code, or 128+N for the signal N.
pub fn status_code(status: &std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt as _;
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

/// One record: the type, the length (big endian), the bytes.
pub async fn write_record<W: AsyncWrite + Unpin>(w: &mut W, kind: u8, data: &[u8]) -> Result<()> {
    let len = u32::try_from(data.len()).context("a record is too large")?;
    let mut head = [0u8; 5];
    head[0] = kind;
    head[1..].copy_from_slice(&len.to_be_bytes());
    w.write_all(&head).await?;
    w.write_all(data).await?;
    w.flush().await?;
    Ok(())
}

/// Copy `from` as records of type `kind`.
async fn records<R, W>(from: &mut R, kind: u8, to: &tokio::sync::Mutex<W>) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = vec![0u8; RECORD_LIMIT];
    loop {
        let n = from.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        write_record(&mut *to.lock().await, kind, &buf[..n]).await?;
    }
}

/// Read the next record: its type and its bytes, or `None` at the end of
/// the stream. A record over [`RECORD_LIMIT`] is an error.
pub async fn read_record<R: AsyncRead + Unpin>(r: &mut R) -> Result<Option<(u8, Vec<u8>)>> {
    let mut head = [0u8; 5];
    match r.read_exact(&mut head).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize;
    if len > RECORD_LIMIT {
        anyhow::bail!("a record of {len} bytes: the most is {RECORD_LIMIT}");
    }
    let mut data = vec![0u8; len];
    r.read_exact(&mut data).await?;
    Ok(Some((head[0], data)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_a_shell_and_a_bad_name_exit_64() {
        for (program, env) in [
            ("cat", vec![]),
            ("/bin/sh", vec![]),
            ("/usr/bin/../bin/cat", vec![]),
            ("/nonexistent/prog", vec![]),
            ("/bin/cat", vec!["A=B".to_string()]),
            ("/bin/cat", vec!["1A".to_string()]),
        ] {
            let err = Spec::new(program, vec![], env, None).unwrap_err();
            assert_eq!(exit::code_of(&err), 64, "{program}: {err:#}");
        }
        assert!(Spec::new("/bin/cat", vec!["-n".into()], vec!["HOME".into()], None).is_ok());
    }

    /// T-403: a record goes and comes back; the end of the stream is None;
    /// a record over the limit is an error.
    #[tokio::test]
    async fn a_record_goes_and_comes_back() {
        let mut buf = Vec::new();
        write_record(&mut buf, STDERR, b"oops").await.unwrap();
        write_record(&mut buf, STATUS, &3i32.to_be_bytes())
            .await
            .unwrap();
        let mut r = &buf[..];
        assert_eq!(
            read_record(&mut r).await.unwrap(),
            Some((STDERR, b"oops".to_vec()))
        );
        assert_eq!(
            read_record(&mut r).await.unwrap(),
            Some((STATUS, vec![0, 0, 0, 3]))
        );
        assert_eq!(read_record(&mut r).await.unwrap(), None);
        let big = [STDOUT, 0xff, 0xff, 0xff, 0xff];
        assert!(read_record(&mut &big[..]).await.is_err());
    }

    /// T-403: `exec-status:` sends stdout and stderr as records, then the
    /// exit status of the program.
    #[tokio::test]
    async fn the_framed_form_sends_the_status() {
        let dir = tempfile::tempdir().unwrap();
        let job = dir.path().join("job");
        std::fs::write(
            &job,
            "#!/bin/sh\nread line\necho \"out: $line\"\necho oops >&2\nexit 3\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&job, std::fs::Permissions::from_mode(0o755)).unwrap();
        let spec = Spec::new(job.to_str().unwrap(), vec![], vec![], None)
            .unwrap()
            .framed();
        let running = start("job", &spec).map_err(|(_, e)| e).unwrap();
        let (mut ours, theirs) = tokio::io::duplex(1 << 16);
        let pump = tokio::spawn(running.pump(theirs));
        ours.write_all(b"hi\n").await.unwrap();
        let (mut out, mut err, mut code) = (Vec::new(), Vec::new(), None);
        while let Some((kind, data)) = read_record(&mut ours).await.unwrap() {
            match kind {
                STDOUT => out.extend(data),
                STDERR => err.extend(data),
                STATUS => code = Some(i32::from_be_bytes(data.try_into().unwrap())),
                other => panic!("type {other}"),
            }
        }
        pump.await.unwrap().unwrap();
        assert_eq!(out, b"out: hi\n");
        assert_eq!(err, b"oops\n");
        assert_eq!(code, Some(3));
    }
}
