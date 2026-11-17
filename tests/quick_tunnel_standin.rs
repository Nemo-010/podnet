//! The binary of podnet against the stand-in of the service and of the edge
//! of pod-cloudflare: no call to Cloudflare. The tests of the quick tunnel
//! itself are in pod-cloudflare; these test the command line: its JSON
//! lines, its signals and its detached instances.

mod standin;
mod test_host;

use std::time::Duration;

use standin::{Cli as _, Process, Standin};

const LONG: Duration = Duration::from_secs(60);

fn hostname_of(ready: &serde_json::Value) -> String {
    let url = ready["url"].as_str().expect("a url");
    url.strip_prefix("https://").expect("an https url").into()
}

#[test]
fn quick_tunnel_standin_prints_json_lines() {
    test_host::need_loopback!();
    let standin = Standin::start();
    let mut tunnel = Process::spawn(standin.tunnel(&["--hello-world", "--json"]));
    let ready = tunnel.ready();
    let url = ready["url"].as_str().unwrap();
    assert!(
        url.starts_with("https://") && url.ends_with(".trycloudflare.com"),
        "{url}"
    );
    assert!(["quic", "http2", "quic+warp", "http2+relay"].contains(&ready["leg"].as_str().unwrap()));
    assert!(ready["tunnel_id"].is_string());
    assert!(ready["locations"].is_array(), "{ready}");
    assert_eq!(ready["locations"][0], "standin1");
    // Each line of standard output is one JSON object.
    tunnel.signal(libc::SIGTERM);
    tunnel.wait(LONG).expect("podnet stops");
    for line in tunnel.out.lock().unwrap().iter() {
        let value: serde_json::Value = serde_json::from_str(line).expect(line);
        assert!(value["event"].is_string(), "{line}");
    }
}

#[test]
fn quick_tunnel_standin_stops_clean_on_sigterm_after_ready() {
    test_host::need_loopback!();
    let standin = Standin::start();
    let mut tunnel =
        Process::spawn(standin.tunnel(&["--hello-world", "--run-for", "60", "--json"]));
    tunnel.ready();
    tunnel.signal(libc::SIGTERM);
    let status = tunnel.wait(LONG).expect("podnet stops");
    assert_eq!(status.code(), Some(0), "{}", tunnel.stderr());
    assert!(tunnel.stderr().contains("SIGTERM: stopping"));
}

#[test]
fn quick_tunnel_standin_detach_ps_logs_and_stop() {
    test_host::need_loopback!();
    let standin = Standin::start();
    let mut start = standin.tunnel(&["--hello-world", "--detach", "--name", "standin"]);
    let output = start.output().expect("podnet --detach");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let doc: serde_json::Value =
        serde_json::from_str(text.lines().last().expect("a line")).expect(&text);
    let host = hostname_of(&doc);
    let pid = doc["pid"].as_i64().expect("a pid") as i32;
    assert_eq!(doc["verified"], false);
    assert!(standin.block_on(standin.edge.wait_registered(&host, Duration::from_secs(20))));

    let (status, _, body) = standin.visit(&host, "/", &[]);
    assert_eq!(status, 200);
    assert!(body.contains("Hello, world"), "{body}");

    let ps = standin.podnet(&["ps", "--json"]).output().unwrap();
    let ps: serde_json::Value = serde_json::from_slice(&ps.stdout).unwrap();
    let state = ps["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "standin")
        .map(|i| i["state"].clone());
    assert_eq!(state, Some("ready".into()), "{ps}");
    // SAFETY: signal 0 only tests that the process exists.
    assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
    let logs = standin
        .podnet(&["logs", "standin", "-n", "5"])
        .output()
        .unwrap();
    assert!(logs.status.success());

    let stop = standin.podnet(&["stop", "standin"]).output().unwrap();
    assert!(
        stop.status.success(),
        "{}",
        String::from_utf8_lossy(&stop.stderr)
    );
    // SAFETY: signal 0 only tests that the process exists.
    assert_ne!(unsafe { libc::kill(pid, 0) }, 0, "the instance runs");
    let ps = standin.podnet(&["ps", "--json"]).output().unwrap();
    let ps: serde_json::Value = serde_json::from_slice(&ps.stdout).unwrap();
    assert_eq!(ps["instances"].as_array().unwrap().len(), 0, "{ps}");
}
