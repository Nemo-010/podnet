//! The vnet shim under a stock daemon (GitHub #599): a record lock with
//! fcntl (T-390), and a listener bound to the wildcard address (T-391).
//!
//! Each test compiles the shim and a small C program, and runs the program
//! with the shim preloaded. With no C compiler, a test says so and passes.

mod shim_harness;

use std::process::Command;

use podnet::vnet::shim;
use shim_harness::{build_program, have_compiler, run_pair};

/// A server: socket(FAMILY), bind(ADDRESS, PORT), listen, accept one
/// connection, echo what it reads.
const SERVER: &str = r#"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <arpa/inet.h>
#include <unistd.h>
int main(int argc, char **argv){
  int fam = strcmp(argv[1], "6") == 0 ? AF_INET6 : AF_INET;
  int port = atoi(argv[3]);
  int s = socket(fam, SOCK_STREAM, 0);
  struct sockaddr_storage ss; memset(&ss, 0, sizeof ss); socklen_t len;
  if (fam == AF_INET6) {
    struct sockaddr_in6 *a = (struct sockaddr_in6 *)&ss;
    a->sin6_family = AF_INET6; a->sin6_port = htons(port);
    inet_pton(AF_INET6, argv[2], &a->sin6_addr); len = sizeof *a;
  } else {
    struct sockaddr_in *a = (struct sockaddr_in *)&ss;
    a->sin_family = AF_INET; a->sin_port = htons(port);
    inet_pton(AF_INET, argv[2], &a->sin_addr); len = sizeof *a;
  }
  if (bind(s, (struct sockaddr *)&ss, len) < 0) { perror("bind"); return 1; }
  listen(s, 4);
  printf("ready\n"); fflush(stdout);
  int c = accept(s, 0, 0);
  if (c < 0) { perror("accept"); return 2; }
  char buf[32] = {0}; int n = read(c, buf, sizeof buf);
  write(c, buf, n); close(c); close(s); return 0;
}
"#;

/// A client: connect to ADDRESS:PORT over AF_INET, send a line, print the echo.
const CLIENT: &str = r#"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <arpa/inet.h>
#include <unistd.h>
int main(int argc, char **argv){
  int s = socket(AF_INET, SOCK_STREAM, 0);
  struct sockaddr_in a; memset(&a, 0, sizeof a);
  a.sin_family = AF_INET; a.sin_port = htons(atoi(argv[2]));
  inet_pton(AF_INET, argv[1], &a.sin_addr);
  if (connect(s, (struct sockaddr *)&a, sizeof a) < 0) { perror("connect"); return 1; }
  const char *q = "wildcard"; write(s, q, strlen(q));
  char buf[32] = {0}; int n = read(s, buf, sizeof buf);
  printf("client: got %dB: %s\n", n, buf);
  close(s); return 0;
}
"#;

/// One case: the family and the address of the bind, the address that the
/// client dials, the port, and PODNET_VNET_MAP_LOOPBACK.
struct Case {
    family: &'static str,
    bind: &'static str,
    dial: &'static str,
    port: u16,
    map_loopback: bool,
}

/// Run SERVER and CLIENT of CASE, both under the shim with the prefix
/// PREFIX. True when the client gets its echo.
fn wildcard_pair(dir: &std::path::Path, shim: &std::path::Path, prefix: &str, case: &Case) -> bool {
    let Case {
        family,
        bind,
        dial,
        port,
        map_loopback,
    } = *case;
    let server = build_program(dir, "server", SERVER);
    let client = build_program(dir, "client", CLIENT);
    let mut s = Command::new(&server);
    s.args([family, bind, &port.to_string()])
        .env("LD_PRELOAD", shim)
        .env("PODNET_VNET_PREFIX", prefix);
    let mut c = Command::new(&client);
    c.args([dial, &port.to_string()])
        .env("LD_PRELOAD", shim)
        .env("PODNET_VNET_PREFIX", prefix);
    if map_loopback {
        s.env("PODNET_VNET_MAP_LOOPBACK", "1");
        c.env("PODNET_VNET_MAP_LOOPBACK", "1");
    }
    let (server_out, client_out) = run_pair(&mut s, &mut c);
    let got = String::from_utf8_lossy(&client_out.stdout).contains("got 8B: wildcard");
    if !got {
        eprintln!(
            "{bind} <- {dial}: client {}: {}; server {}: {}",
            client_out.status,
            String::from_utf8_lossy(&client_out.stderr),
            server_out.status,
            String::from_utf8_lossy(&server_out.stderr)
        );
    }
    got && server_out.status.success()
}

// T-391: a server bound to 0.0.0.0 or :: answers a client that dials the
// local address of the shim, and 127.0.0.1 where loopback maps to it.
#[test]
fn a_wildcard_bind_takes_the_local_address() {
    if !have_compiler() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let built = shim::build(dir.path()).unwrap();
    let prefix = format!("podnet-wild-{}", std::process::id());
    let cases = [
        Case {
            family: "4",
            bind: "0.0.0.0",
            dial: "10.66.0.2",
            port: 18191,
            map_loopback: false,
        },
        Case {
            family: "6",
            bind: "::",
            dial: "10.66.0.2",
            port: 18192,
            map_loopback: false,
        },
        Case {
            family: "4",
            bind: "0.0.0.0",
            dial: "127.0.0.1",
            port: 18193,
            map_loopback: true,
        },
        // The control: a bind to the local address itself.
        Case {
            family: "4",
            bind: "10.66.0.2",
            dial: "10.66.0.2",
            port: 18194,
            map_loopback: false,
        },
    ];
    for case in &cases {
        assert!(
            wildcard_pair(dir.path(), &built.path, &prefix, case),
            "family {}: bind {}, dial {}: no echo",
            case.family,
            case.bind,
            case.dial
        );
    }
}

// T-390: a record lock and its query on a file under the shim, with the
// struct flock of the classic and the open file description forms.
#[test]
fn a_record_lock_passes_its_struct_flock() {
    if !have_compiler() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let built = shim::build(dir.path()).unwrap();
    let src = r#"
#define _GNU_SOURCE
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <errno.h>
#include <unistd.h>
static int lock(int fd, int cmd, const char *name) {
  struct flock fl; memset(&fl, 0, sizeof fl);
  fl.l_type = F_WRLCK; fl.l_whence = SEEK_SET;
  int rc = fcntl(fd, cmd, &fl);
  printf("%s: %d %s type=%d\n", name, rc, rc ? strerror(errno) : "ok", fl.l_type);
  return rc != 0;
}
int main(int argc, char **argv){
  int fd = open(argv[1], O_RDWR | O_CREAT, 0600);
  if (fd < 0) { perror("open"); return 2; }
  int bad = lock(fd, F_SETLK, "F_SETLK") + lock(fd, F_GETLK, "F_GETLK")
          + lock(fd, F_SETLKW, "F_SETLKW");
#ifdef F_OFD_SETLK
  int fd2 = open(argv[1], O_RDWR);
  bad += lock(fd2, F_OFD_GETLK, "F_OFD_GETLK");
#endif
  struct f_owner_ex own; memset(&own, 0, sizeof own);
  int rc = fcntl(fd, F_GETOWN_EX, &own);
  printf("F_GETOWN_EX: %d %s\n", rc, rc ? strerror(errno) : "ok");
  return bad + (rc != 0);
}
"#;
    let prog = build_program(dir.path(), "lock", src);
    let file = dir.path().join("lockfile");
    let bare = Command::new(&prog).arg(&file).output().unwrap();
    assert!(
        bare.status.success(),
        "with no shim: {}",
        String::from_utf8_lossy(&bare.stdout)
    );
    let out = Command::new(&prog)
        .arg(&file)
        .env("LD_PRELOAD", &built.path)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "under the shim: {text}");
    assert!(!text.contains("Bad address"), "{text}");
}
