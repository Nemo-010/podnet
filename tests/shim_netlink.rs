//! The vnet shim gives a program the interfaces of the virtual network
//! (T-392, GitHub #599): a NETLINK_ROUTE dump, getifaddrs, if_nameindex
//! and `ip` see `lo` and `vnet0` with the local address, not the host.
//!
//! Each test compiles the shim and a small C program, and runs the program
//! with the shim preloaded. With no C compiler, a test says so and passes.

mod shim_harness;

use std::path::Path;
use std::process::Command;

use podnet::vnet::shim;
use shim_harness::{build_program, have_compiler};

/// A dump in the way of libtorrent: socket, bind with port id 0,
/// getsockname, send, then recvfrom until NLMSG_DONE. Each answer must come
/// from the kernel's address and carry the port id of getsockname and the
/// sequence number of the request. Then getifaddrs, if_nameindex and
/// if_nametoindex.
const PROBE: &str = r#"
#define _GNU_SOURCE
#include <arpa/inet.h>
#include <ifaddrs.h>
#include <linux/netlink.h>
#include <linux/rtnetlink.h>
#include <net/if.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>
static int dump(int type, int family) {
  int s = socket(AF_NETLINK, SOCK_RAW | SOCK_CLOEXEC, NETLINK_ROUTE);
  if (s < 0) { perror("socket"); return 1; }
  struct sockaddr_nl me; memset(&me, 0, sizeof me); me.nl_family = AF_NETLINK;
  if (bind(s, (struct sockaddr *)&me, sizeof me) < 0) { perror("bind"); return 1; }
  socklen_t len = sizeof me;
  if (getsockname(s, (struct sockaddr *)&me, &len) < 0 || me.nl_family != AF_NETLINK) {
    perror("getsockname"); return 1;
  }
  struct { struct nlmsghdr h; struct rtgenmsg g; } req;
  memset(&req, 0, sizeof req);
  req.h.nlmsg_len = sizeof req; req.h.nlmsg_type = type;
  req.h.nlmsg_flags = NLM_F_REQUEST | NLM_F_DUMP; req.h.nlmsg_seq = 7;
  req.g.rtgen_family = family;
  if (send(s, &req, sizeof req, 0) < 0) { perror("send"); return 1; }
  char buf[8192]; int done = 0;
  while (!done) {
    struct sockaddr_nl from; socklen_t flen = sizeof from;
    int n = recvfrom(s, buf, sizeof buf, 0, (struct sockaddr *)&from, &flen);
    if (n <= 0) { perror("recvfrom"); return 1; }
    if (from.nl_family != AF_NETLINK || from.nl_pid != 0) { printf("not from the kernel\n"); return 1; }
    for (struct nlmsghdr *h = (struct nlmsghdr *)buf; NLMSG_OK(h, n); h = NLMSG_NEXT(h, n)) {
      if (h->nlmsg_pid != me.nl_pid || h->nlmsg_seq != 7) {
        printf("pid %u seq %u, want %u\n", h->nlmsg_pid, h->nlmsg_seq, me.nl_pid); return 1;
      }
      if (h->nlmsg_type == NLMSG_DONE) { done = 1; break; }
      if (h->nlmsg_type == RTM_NEWLINK) {
        struct ifinfomsg *i = NLMSG_DATA(h); int l = IFLA_PAYLOAD(h);
        for (struct rtattr *a = IFLA_RTA(i); RTA_OK(a, l); a = RTA_NEXT(a, l))
          if (a->rta_type == IFLA_IFNAME) printf("link %d %s\n", i->ifi_index, (char *)RTA_DATA(a));
      } else if (h->nlmsg_type == RTM_NEWADDR) {
        struct ifaddrmsg *i = NLMSG_DATA(h); int l = IFA_PAYLOAD(h);
        for (struct rtattr *a = IFA_RTA(i); RTA_OK(a, l); a = RTA_NEXT(a, l))
          if (a->rta_type == IFA_ADDRESS) {
            char t[64]; inet_ntop(i->ifa_family, RTA_DATA(a), t, sizeof t);
            printf("addr %d %s/%d\n", i->ifa_index, t, i->ifa_prefixlen);
          }
      } else {
        printf("message %d\n", h->nlmsg_type);
      }
    }
  }
  close(s); return 0;
}
int main(void) {
  int bad = dump(RTM_GETLINK, AF_UNSPEC) + dump(RTM_GETADDR, AF_UNSPEC) + dump(RTM_GETROUTE, AF_UNSPEC);
  struct ifaddrs *list;
  if (getifaddrs(&list) == 0) {
    for (struct ifaddrs *i = list; i; i = i->ifa_next) {
      char t[64] = "";
      if (!i->ifa_addr) continue;
      if (i->ifa_addr->sa_family == AF_INET)
        inet_ntop(AF_INET, &((struct sockaddr_in *)i->ifa_addr)->sin_addr, t, sizeof t);
      else if (i->ifa_addr->sa_family == AF_INET6)
        inet_ntop(AF_INET6, &((struct sockaddr_in6 *)i->ifa_addr)->sin6_addr, t, sizeof t);
      else continue;
      printf("ifaddr %s %s\n", i->ifa_name, t);
    }
    freeifaddrs(list);
  } else { perror("getifaddrs"); bad++; }
  struct if_nameindex *ix = if_nameindex();
  for (struct if_nameindex *p = ix; p && p->if_index; p++) printf("index %u %s\n", p->if_index, p->if_name);
  if (ix) if_freenameindex(ix);
  printf("nametoindex %u\n", if_nametoindex("vnet0"));
  return bad;
}
"#;

/// The lines that the shim gives with the local address 10.66.0.7.
/// No route, no other message, and nothing of the host.
const WANT: [&str; 11] = [
    "link 1 lo",
    "link 2 vnet0",
    "addr 1 127.0.0.1/8",
    "addr 2 10.66.0.7/24",
    "addr 1 ::1/128",
    "ifaddr lo 127.0.0.1",
    "ifaddr vnet0 10.66.0.7",
    "ifaddr lo ::1",
    "index 1 lo",
    "index 2 vnet0",
    "nametoindex 2",
];

/// Run the probe with the shim and ENV. The output, and true when the
/// program exits 0.
fn probe(dir: &Path, shim: &Path, env: &[(&str, &str)]) -> (String, bool) {
    let program = build_program(dir, "probe", PROBE);
    let mut command = Command::new(&program);
    command
        .env("LD_PRELOAD", shim)
        .env("PODNET_VNET_LOCAL_ADDR", "10.66.0.7");
    for (k, v) in env {
        command.env(k, v);
    }
    let out = command.output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        eprintln!(
            "probe: {}\n{text}{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    (text, out.status.success())
}

/// True when OUTPUT is exactly the view of the virtual network.
fn is_the_virtual_view(output: &str) -> bool {
    output.lines().eq(WANT)
}

#[test]
fn a_netlink_dump_and_getifaddrs_give_the_virtual_interfaces() {
    if !have_compiler() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let built = shim::build(dir.path()).unwrap();
    let (output, ok) = probe(dir.path(), &built.path, &[]);
    assert!(ok, "{output}");
    assert!(is_the_virtual_view(&output), "{output}");
    // The control: with PODNET_VNET_HOST_NETLINK the host's kernel answers,
    // so a shim that gives netlink to the host fails the check above.
    let (host, _) = probe(
        dir.path(),
        &built.path,
        &[("PODNET_VNET_HOST_NETLINK", "1")],
    );
    assert!(!is_the_virtual_view(&host), "{host}");
}

#[test]
fn ip_shows_the_virtual_interfaces() {
    if !have_compiler() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let Some(ip) = shim::which("ip") else {
        eprintln!("skipping: no ip on this host");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let built = shim::build(dir.path()).unwrap();
    let run = |args: &[&str]| {
        let out = Command::new(&ip)
            .args(args)
            .env("LD_PRELOAD", &built.path)
            .env("PODNET_VNET_LOCAL_ADDR", "10.66.0.7")
            .output()
            .unwrap();
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )
    };
    let (ok, addr) = run(&["-br", "addr"]);
    assert!(ok, "{addr}");
    let names: Vec<&str> = addr
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert_eq!(names, ["lo", "vnet0"], "{addr}");
    assert!(
        addr.contains("10.66.0.7/24") && addr.contains("127.0.0.1/8"),
        "{addr}"
    );
    // A change of the network is refused, as for a program with no
    // privilege.
    let (ok, _) = run(&["link", "set", "lo", "down"]);
    assert!(!ok, "ip link set must fail under the shim");
}
