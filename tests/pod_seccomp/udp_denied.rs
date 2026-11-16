//! T-383: a name resolves where a seccomp filter of the host refuses each
//! UDP socket of the program (the lab `udp-denied` of pod-lab).

use super::*;
use std::os::unix::process::CommandExt;

/// Refuse `socket(AF_INET or AF_INET6, SOCK_DGRAM)` with `EPERM` for the
/// calling process and its children, as the lab `udp-denied` does. Call
/// it in the child, before the exec: podnet runs under the filter too.
fn refuse_udp_sockets() -> std::io::Result<()> {
    const RET_ALLOW: u32 = 0x7fff_0000;
    const RET_ERRNO: u32 = 0x0005_0000;
    let stmt = |code: u32, k: u32| libc::sock_filter {
        code: code as u16,
        jt: 0,
        jf: 0,
        k,
    };
    let jeq = |k: u32, jt: u8, jf: u8| libc::sock_filter {
        code: (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
        jt,
        jf,
        k,
    };
    let load = |offset: u32| stmt(libc::BPF_LD | libc::BPF_W | libc::BPF_ABS, offset);
    // seccomp_data: nr at 0, args[0] at 16, args[1] at 24 (the low 32
    // bits on a little-endian machine).
    let filter = [
        load(0),
        jeq(libc::SYS_socket as u32, 0, 6),
        load(16),
        jeq(libc::AF_INET as u32, 1, 0),
        jeq(libc::AF_INET6 as u32, 0, 3),
        load(24),
        stmt(libc::BPF_ALU | libc::BPF_AND | libc::BPF_K, 0xf),
        jeq(libc::SOCK_DGRAM as u32, 1, 0),
        stmt(libc::BPF_RET | libc::BPF_K, RET_ALLOW),
        stmt(libc::BPF_RET | libc::BPF_K, RET_ERRNO | libc::EPERM as u32),
    ];
    let prog = libc::sock_fprog {
        len: filter.len() as u16,
        filter: filter.as_ptr() as *mut libc::sock_filter,
    };
    // SAFETY: prctl with valid arguments; `prog` lives for the call.
    unsafe {
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0
            || libc::prctl(
                libc::PR_SET_SECCOMP,
                libc::SECCOMP_MODE_FILTER,
                &prog as *const libc::sock_fprog,
            ) != 0
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Make a UDP socket, then resolve a name (glibc), connect to it and read.
const CLIENT: &str = r#"
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <netdb.h>
#include <sys/socket.h>
int main(void) {
    alarm(20);
    int u = socket(AF_INET, SOCK_DGRAM, 0);
    printf("UDP:%s\n", u < 0 ? "refused" : "made");
    struct addrinfo hints, *res = NULL;
    memset(&hints, 0, sizeof hints);
    hints.ai_family = AF_UNSPEC; hints.ai_socktype = SOCK_STREAM;
    int r = getaddrinfo("example.com", "80", &hints, &res);
    printf("GAI:%d\n", r);
    if (r != 0) return 1;
    int s = socket(res->ai_family, SOCK_STREAM, 0);
    if (connect(s, res->ai_addr, res->ai_addrlen) < 0) { perror("connect"); return 1; }
    char b[64] = {0};
    int n = read(s, b, sizeof b - 1);
    printf("READ:%d:%s", n, b);
    return 0;
}
"#;

/// T-383: under a filter that refuses each UDP socket, a glibc program in
/// the pod resolves a name: the pod adds `use-vc` to `RES_OPTIONS`, and
/// its fake DNS answers the query over TCP. Each backend that runs here.
#[test]
fn a_name_resolves_where_udp_sockets_are_refused() {
    if !have_compiler() {
        eprintln!("skipping: no compiler");
        return;
    }
    let backends: Vec<&str> = [
        ("seccomp", seccomp_available()),
        ("netns", netns_available()),
    ]
    .into_iter()
    .filter_map(|(name, ok)| ok.then_some(name))
    .collect();
    if backends.is_empty() {
        eprintln!("skipping: no backend of the pod on this host");
        return;
    }
    let dir = std::env::temp_dir().join(format!("podnet-netns-udpdenied-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let sock = dir.join("socks.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || socks5_proxy(listener, tx, false));
    let program = build_program(&dir, "udp-denied", CLIENT, false);
    for backend in backends {
        let mut command = pod_command();
        command
            .args(["--backend", backend, "-x"])
            .arg(format!("unix:{}", sock.display()))
            .arg("--")
            .arg(&program)
            .env("LOCALDOMAIN", "")
            .env_remove("RES_OPTIONS")
            .env_remove("PODNET_NETNS_QUIET");
        // SAFETY: the closure calls only prctl, which is async-signal-safe.
        unsafe { command.pre_exec(refuse_udp_sockets) };
        let output = command.output().unwrap();
        let out = String::from_utf8_lossy(&output.stdout);
        let err = String::from_utf8_lossy(&output.stderr);
        assert!(out.contains("UDP:refused"), "{backend}: {out:?} {err:?}");
        assert!(out.contains("GAI:0"), "{backend}: {out:?} {err:?}");
        assert!(
            out.contains("HELLO-FROM-TARGET"),
            "{backend}: {out:?} {err:?}"
        );
        assert!(err.contains("RES_OPTIONS use-vc"), "{backend}: {err:?}");
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            "example.com:80",
            "{backend}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
