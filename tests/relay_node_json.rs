//! `podnet relay node --json` (#631) against the local fake relay of the
//! tests of the reverse mode: the node prints its ready document in the
//! foreground when it is connected.

mod test_host;

use std::io::BufRead as _;
use std::path::Path;
use std::time::Duration;

const NAME: &str = "testnode";
/// The tokens that the fake relay takes: made-up values of a test.
const NODE_TOKEN: &str = "testonly-node";
const CONNECT_TOKEN: &str = "testonly-connect";

#[path = "relay_reverse/fake.rs"]
#[allow(dead_code)]
mod fake;

/// Write the pair `name` of the relay `url` to `dir` (`pair-NAME.json`, mode
/// 0600, the directory 0700), with one hour left.
fn write_pair(dir: &Path, url: &str) {
    use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .unwrap();
    let expires = (std::time::SystemTime::now() + Duration::from_secs(3600))
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let doc = serde_json::json!({
        "relay": url, "name": NAME, "node_token": NODE_TOKEN,
        "connect_token": CONNECT_TOKEN, "stop_token": "", "expires": expires,
    });
    let path = dir.join(format!("pair-{NAME}.json"));
    std::fs::write(&path, doc.to_string()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn relay_node_json_prints_the_ready_document() {
    test_host::need_loopback!();
    let (url, _state) = fake::fake_relay(None).await;
    let (home, run) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::set_permissions(
        run.path(),
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
    write_pair(&home.path().join("relay"), &url);
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_podnet"))
        .args(["relay", "node", NAME, "--to", "127.0.0.1:9", "--json"])
        .env("PODNET_STATE_DIR", home.path())
        .env("PODNET_RUN_DIR", run.path())
        .env_remove("PODNET_RELAY")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let line = tokio::task::spawn_blocking(move || {
        let mut line = String::new();
        let _ = std::io::BufReader::new(stdout).read_line(&mut line);
        line
    });
    let line = tokio::time::timeout(Duration::from_secs(20), line)
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    // The node runs on in the foreground after the document.
    let running = child.try_wait().unwrap().is_none();
    unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    let status = child.wait().unwrap();
    let doc: serde_json::Value = serde_json::from_str(&line).unwrap_or_else(|_| panic!("{line:?}"));
    assert_eq!(doc["event"], "ready", "{doc}");
    assert_eq!(doc["pair"], NAME, "{doc}");
    assert_eq!(doc["relay"], url.as_str(), "{doc}");
    assert_eq!(doc["target"], "127.0.0.1:9", "{doc}");
    assert_eq!(doc["schema"], 1, "{doc}");
    assert!(running, "the node ended after the document");
    assert_eq!(status.code(), Some(0), "{status:?}");
}
