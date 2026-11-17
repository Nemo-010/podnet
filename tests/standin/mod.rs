//! The stand-in of the quick-tunnel service and of the edge is
//! pod-cloudflare's (feature `standin`). This file runs the binary of podnet
//! against it: the stand-in API as `--service-url`, its edge in
//! `PODNET_EDGE`, its CA in `SSL_CERT_FILE`.

#![allow(dead_code, unused_imports)]

use std::io::{BufRead as _, Read as _, Write as _};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use pod_cloudflare::standin::{Answer, Reply, Standin};

/// The commands of podnet against a stand-in.
pub trait Cli {
    /// The run directory of the detached instances of a test.
    fn run_dir(&self) -> PathBuf;
    /// The binary of podnet with `args`, against the stand-in: no proxy
    /// variable, no UDP, the edge and the CA of the stand-in.
    fn podnet(&self, args: &[&str]) -> Command;
    /// `podnet tunnel` with `args`, on the stand-in API, HTTP/2, direct,
    /// with no check of the URL.
    fn tunnel(&self, args: &[&str]) -> Command;
}

impl Cli for Standin {
    fn run_dir(&self) -> PathBuf {
        self.dir.path().join("run")
    }

    fn podnet(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_podnet"));
        command.args(args);
        for name in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "no_proxy",
            "PODNET_PROXY",
            "RUST_LOG",
        ] {
            command.env_remove(name);
        }
        command
            .env("SSL_CERT_FILE", &self.ca_file)
            .env("PODNET_EDGE", self.edge_addr.to_string())
            .env("PODNET_RUN_DIR", self.run_dir())
            .env("HOME", self.dir.path())
            .env("PODNET_UDP", "off")
            .stdin(Stdio::null());
        command
    }

    fn tunnel(&self, args: &[&str]) -> Command {
        let mut all = vec![
            "tunnel",
            "--service-url",
            &self.api_url,
            "--protocol",
            "http2",
            "--transport",
            "direct",
            "--no-verify",
        ];
        all.extend_from_slice(args);
        self.podnet(&all)
    }
}

/// A podnet process whose standard output and error are read by threads.
pub struct Process {
    pub child: Child,
    lines: Receiver<String>,
    pub out: Arc<std::sync::Mutex<Vec<String>>>,
    pub err: Arc<std::sync::Mutex<String>>,
}

impl Process {
    pub fn spawn(mut command: Command) -> Process {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command.spawn().expect("spawn podnet");
        let (tx, lines) = std::sync::mpsc::channel();
        let out: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
        let kept = out.clone();
        let stdout = child.stdout.take().unwrap();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                kept.lock().unwrap().push(line.clone());
                let _ = tx.send(line);
            }
        });
        let err: Arc<std::sync::Mutex<String>> = Arc::default();
        let kept = err.clone();
        let stderr = child.stderr.take().unwrap();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                let mut text = kept.lock().unwrap();
                text.push_str(&line);
                text.push('\n');
            }
        });
        Process {
            child,
            lines,
            out,
            err,
        }
    }

    /// The next line of standard output that `matches`, within `within`.
    pub fn line_where(&self, within: Duration, matches: impl Fn(&str) -> bool) -> Option<String> {
        let end = Instant::now() + within;
        loop {
            let left = end.checked_duration_since(Instant::now())?;
            let line = self.lines.recv_timeout(left).ok()?;
            if matches(&line) {
                return Some(line);
            }
        }
    }

    /// The JSON line with `"event":"ready"` (`--json`).
    pub fn ready(&self) -> serde_json::Value {
        let line = self
            .line_where(Duration::from_secs(60), |l| {
                l.contains(r#""event":"ready""#)
            })
            .unwrap_or_else(|| panic!("no ready line; stderr:\n{}", self.stderr()));
        serde_json::from_str(&line).expect("the ready line is JSON")
    }

    pub fn stderr(&self) -> String {
        self.err.lock().unwrap().clone()
    }

    pub fn signal(&self, signal: libc::c_int) {
        // SAFETY: kill() to our own child.
        unsafe { libc::kill(self.child.id() as i32, signal) };
    }

    /// Wait for the end of the process.
    pub fn wait(&mut self, within: Duration) -> Option<ExitStatus> {
        let end = Instant::now() + within;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                // Let the thread of standard error finish.
                std::thread::sleep(Duration::from_millis(100));
                return Some(status);
            }
            if Instant::now() >= end {
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// An origin on the loopback: it answers each request with `body`, and it
/// keeps the head of each request. Gives the port.
pub fn origin(body: &'static str) -> (u16, Arc<std::sync::Mutex<Vec<String>>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    let kept = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let kept = kept.clone();
            std::thread::spawn(move || {
                let mut stream = stream;
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    match stream.read(&mut byte) {
                        Ok(1) => head.push(byte[0]),
                        _ => return,
                    }
                }
                kept.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&head).to_string());
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nX-Origin: standin\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    (port, seen)
}
