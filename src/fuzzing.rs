//! Entry points for the fuzz targets of `fuzz/` (cargo-fuzz). Each function
//! takes untrusted bytes, runs one parser of podnet on them, and must not
//! panic, hang or allocate much more than the input. The parsers of the
//! edge of Cloudflare have their targets in pod-cloudflare. The functions give no
//! result: a parse error is a correct answer to bad bytes.
//!
//! [`TARGETS`] is the list of the targets, and [`seeds`] gives the seed
//! inputs of each. A unit test runs each target on its seeds and on
//! deterministic mutations of them, so that the stable test suite also
//! catches a panic.

mod peers;
pub use peers::{
    mesh_announce, mesh_table, network_spec, router_config, transfer_message, transfer_zip,
    transit_record, wg_quick,
};

/// The targets of the seccomp handlers of the pod (`podnet_netns::fuzzing`).
pub use podnet_netns::fuzzing::{seccomp_sendmsg, seccomp_sendto, seccomp_sockopt};

/// One fuzz target: it takes untrusted bytes.
pub type Target = fn(&[u8]);

/// The name and the function of each target.
pub const TARGETS: &[(&str, Target)] = &[
    ("capsule", capsule),
    ("proxy_url", proxy_url),
    ("no_proxy", no_proxy),
    ("connect_response_head", connect_response_head),
    ("acl_rule", acl_rule),
    ("local_spec", local_spec),
    ("vnet_frame", vnet_frame),
    ("seccomp_sendto", seccomp_sendto),
    ("seccomp_sendmsg", seccomp_sendmsg),
    ("seccomp_sockopt", seccomp_sockopt),
    ("wg_quick", wg_quick),
    ("network_spec", network_spec),
    ("router_config", router_config),
    ("transfer_message", transfer_message),
    ("transfer_zip", transfer_zip),
    ("transit_record", transit_record),
    ("mesh_announce", mesh_announce),
    ("mesh_table", mesh_table),
];

/// MASQUE capsules, in two chunks.
pub fn capsule(data: &[u8]) {
    let mut decoder = crate::warp::capsule::CapsuleDecoder::new();
    let half = data.len() / 2;
    decoder.push(&data[..half]);
    while decoder.next_packet().is_some() {}
    decoder.push(&data[half..]);
    let mut total = 0;
    while let Some(packet) = decoder.next_packet() {
        total += packet.len();
    }
    assert!(total <= data.len());
}

fn text(data: &[u8]) -> Option<&str> {
    std::str::from_utf8(data).ok()
}

/// A proxy URL (`--proxy`, `HTTPS_PROXY`) by podnet's parse (T-349: with the
/// check of a Unix path). A parsed proxy prints with no password.
pub fn proxy_url(data: &[u8]) {
    let Some(spec) = text(data) else { return };
    if let Ok(proxy) = crate::egress::parse(spec) {
        let shown = proxy.label();
        assert!(!shown.contains('@'), "{shown}");
    }
    // The same text as a network spec (T-059): the parse and the label
    // never panic, and an error is one line. A `wg:` spec reads a file: the
    // target opens no file of the input.
    if spec.contains("wg:") {
        return;
    }
    match crate::networks::parse(spec) {
        Ok(network) => {
            let _ = network.label();
            let _ = crate::network::Network::checks(&network);
        }
        Err(e) => assert!(!e.text().contains('\n') || spec.contains('\n'), "{e}"),
    }
}

/// A `NO_PROXY` list.
pub fn no_proxy(data: &[u8]) {
    let Some(list) = text(data) else { return };
    let rules = crate::egress::NoProxy::parse(list);
    let _ = rules.matches("api.trycloudflare.com", 443);
    let _ = rules.matches("198.41.192.167", 7844);
    let _ = rules.matches("::1", 80);
}

/// The status line of a proxy answer to `CONNECT`.
pub fn connect_response_head(data: &[u8]) {
    let _ = crate::egress::parse_status(data);
}

/// An ACL rule of `podnet net`.
pub fn acl_rule(data: &[u8]) {
    if let Some(line) = text(data) {
        let _ = crate::vnet::policy::AclRule::parse(line);
    }
}

/// A local listener of `podnet connect`.
pub fn local_spec(data: &[u8]) {
    if let Some(value) = text(data) {
        let _ = crate::ws::LocalSpec::parse(value);
    }
}

/// A frame of the virtual network.
pub fn vnet_frame(data: &[u8]) {
    let mut reader = data;
    let mut packet = Vec::new();
    for _ in 0..64 {
        match futures::executor::block_on(crate::vnet::framing::read_frame(
            &mut reader,
            &mut packet,
        )) {
            Ok(Some(n)) => assert!(n <= data.len()),
            _ => break,
        }
    }
}

/// The seed inputs of `target`.
pub fn seeds(target: &str) -> Vec<Vec<u8>> {
    let s = |items: &[&str]| items.iter().map(|i| i.as_bytes().to_vec()).collect();
    match target {
        "capsule" => vec![
            vec![0x00, 0x04, 0x45, 0x00, 0x00, 0x04],
            vec![0x01, 0x01, 0x00],
        ],
        "proxy_url" => s(&[
            "http://127.0.0.1:3128",
            "http://user:pass@proxy:8080",
            "socks5h://[::1]:1080",
            "https://p",
        ]),
        "no_proxy" => s(&["localhost,.example.com,10.0.0.0/8", "*", "[::1]:80,api.x"]),
        "connect_response_head" => s(&[
            "HTTP/1.1 200 Connection established\r\n\r\n",
            "HTTP/1.0 407 Proxy Authentication Required\r\n",
        ]),
        "acl_rule" => s(&["allow 10.66.0.0/24:80-90", "deny *", "deny [fd00::1]:22"]),
        "local_spec" => s(&[
            "tcp://8080",
            "tcp://0.0.0.0:2222",
            "unix:///run/a.sock",
            "unix:",
        ]),
        "vnet_frame" => vec![vec![0x00, 0x03, 1, 2, 3], vec![0x80, 0x01, 9]],
        "seccomp_sendto" | "seccomp_sendmsg" | "seccomp_sockopt" => {
            podnet_netns::fuzzing::seeds(target)
        }
        _ => peers::seeds(target),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-349: the crash input of `proxy_url` of the fuzz run 43590953876: the
    /// unix path held the user info, and the label showed it.
    #[test]
    fn fuzz_a_unix_proxy_label_holds_no_user_info() {
        proxy_url(b"socks5+unix://ss@proxy:2]:/");
        proxy_url(b"socks5h+unix://user:hunter2@x");
    }

    /// Each target on its seeds and on 2000 mutations of them (100 for
    /// mesh_announce; a fixed xorshift sequence): no panic. This is the stable part of `fuzz/`.
    #[test]
    fn each_target_survives_mutations() {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for (name, target) in TARGETS {
            let seeds = seeds(name);
            assert!(!seeds.is_empty(), "{name} has no seed");
            for seed in &seeds {
                target(seed);
            }
            // Each input of mesh_announce that parses signs and verifies
            // three times: a debug build does that slowly.
            let rounds = if *name == "mesh_announce" { 100 } else { 2000 };
            for _ in 0..rounds {
                let mut input = seeds[(next() % seeds.len() as u64) as usize].clone();
                for _ in 0..(1 + next() % 4) {
                    let r = next();
                    match r % 4 {
                        0 if !input.is_empty() => {
                            let i = (r >> 8) as usize % input.len();
                            input[i] ^= (r >> 16) as u8;
                        }
                        1 if !input.is_empty() => {
                            let i = (r >> 8) as usize % input.len();
                            input[i] = [0, 0xff, 0x7f, 0x80][(r >> 16) as usize % 4];
                        }
                        2 => {
                            let i = (r >> 8) as usize % (input.len() + 1);
                            input.insert(i, (r >> 16) as u8);
                        }
                        _ => {
                            let keep = (r >> 8) as usize % (input.len() + 1);
                            input.truncate(keep);
                        }
                    }
                }
                target(&input);
            }
        }
    }
}
