//! The vnet shim and the IPv4-mapped IPv6 address (T-356), and the forward
//! of a TCP destination to a unix socket path (T-357, `PODNET_VNET_FORWARD`).
//!
//! The source is PodUtils/podrunner#170: a dual-mode .NET client connects
//! an `AF_INET6` socket to `::ffff:127.0.0.1`, and must reach a broker that
//! listens on a unix socket path.

mod shim_harness;

use std::process::Command;

use podnet::vnet::shim;
use shim_harness::{build_program, have_compiler, run_pair};

/// A server and a client in one program: `peer server FAMILY ADDR PORT` or
/// `peer client FAMILY ADDR PORT`. Each prints the addresses that the shim
/// gives back as `TAG FAMILY ADDR PORT LEN`.
const PEER: &str = r#"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <arpa/inet.h>
#include <unistd.h>
static socklen_t make(int fam, const char *ip, int port, struct sockaddr_storage *ss){
  memset(ss,0,sizeof *ss);
  if(fam==6){struct sockaddr_in6*a=(void*)ss;a->sin6_family=AF_INET6;a->sin6_port=htons(port);
    if(inet_pton(AF_INET6,ip,&a->sin6_addr)!=1)exit(9);return sizeof *a;}
  struct sockaddr_in*a=(void*)ss;a->sin_family=AF_INET;a->sin_port=htons(port);
  if(inet_pton(AF_INET,ip,&a->sin_addr)!=1)exit(9);return sizeof *a;
}
static void show(const char *tag, struct sockaddr_storage *ss, socklen_t len){
  char ip[64]="?";int port=-1,fam=0;
  if(ss->ss_family==AF_INET){struct sockaddr_in*a=(void*)ss;fam=4;
    inet_ntop(AF_INET,&a->sin_addr,ip,sizeof ip);port=ntohs(a->sin_port);}
  else if(ss->ss_family==AF_INET6){struct sockaddr_in6*a=(void*)ss;fam=6;
    inet_ntop(AF_INET6,&a->sin6_addr,ip,sizeof ip);port=ntohs(a->sin6_port);}
  printf("%s %d %s %d %u\n",tag,fam,ip,port,(unsigned)len);fflush(stdout);
}
int main(int argc,char**argv){
  if(argc<5)return 8;
  int fam=atoi(argv[2]),port=atoi(argv[4]);
  struct sockaddr_storage ss,got;socklen_t len=make(fam,argv[3],port,&ss),gl;
  int s=socket(fam==6?AF_INET6:AF_INET,SOCK_STREAM,0);
  if(s<0){perror("socket");return 1;}
  if(strcmp(argv[1],"server")==0){
    if(bind(s,(void*)&ss,len)<0){perror("bind");return 2;}
    listen(s,4);printf("ready\n");fflush(stdout);
    gl=sizeof got;int c=accept(s,(void*)&got,&gl);
    if(c<0){perror("accept");return 3;}
    show("accept",&got,gl);
    gl=sizeof got;if(getpeername(c,(void*)&got,&gl)==0)show("server-peer",&got,gl);
    char buf[16]={0};int n=read(c,buf,sizeof buf);if(n>0)write(c,buf,n);
    close(c);close(s);return 0;
  }
  if(connect(s,(void*)&ss,len)<0){perror("connect");return 4;}
  gl=sizeof got;if(getpeername(s,(void*)&got,&gl)<0){perror("getpeername");return 5;}
  show("peer",&got,gl);
  gl=sizeof got;if(getsockname(s,(void*)&got,&gl)<0){perror("getsockname");return 6;}
  show("local",&got,gl);
  write(s,"mapped",6);char buf[16]={0};int n=read(s,buf,sizeof buf-1);
  printf("echo %d %s\n",n,buf);close(s);return 0;
}
"#;

/// Run the server and the client of [`PEER`] with the shim, and give the
/// stdout of each.
fn pair(
    shim: &std::path::Path,
    prefix: &str,
    server: &[&str],
    client: &[&str],
) -> (String, String) {
    let dir = shim.parent().unwrap();
    let program = dir.join("peer");
    if !program.exists() {
        build_program(dir, "peer", PEER);
    }
    let mut server_cmd = Command::new(&program);
    server_cmd
        .arg("server")
        .args(server)
        .env("LD_PRELOAD", shim)
        .env("PODNET_VNET_PREFIX", prefix)
        .env("PODNET_VNET_LOG", "1");
    let mut client_cmd = Command::new(&program);
    client_cmd
        .arg("client")
        .args(client)
        .env("LD_PRELOAD", shim)
        .env("PODNET_VNET_PREFIX", prefix)
        .env("PODNET_VNET_LOG", "1");
    let (s, c) = run_pair(&mut server_cmd, &mut client_cmd);
    let text = |o: &std::process::Output| {
        format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        )
    };
    assert!(s.status.success(), "server: {}", text(&s));
    assert!(c.status.success(), "client: {}", text(&c));
    (text(&s), text(&c))
}

/// T-356: a listener on `127.0.0.1:P` and a client that connects an
/// `AF_INET6` socket to `::ffff:127.0.0.1:P` meet, on the name
/// `podnet-vnet/4/127.0.0.1/P`. Each side gets addresses of the family of
/// its own descriptor. The other way, a listener of `AF_INET6` on
/// `::ffff:127.0.0.1` takes an `AF_INET` client.
#[test]
fn a_mapped_ipv6_connect_meets_an_ipv4_listener() {
    if !have_compiler() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let shim = shim::build(dir.path()).unwrap();
    let prefix = format!("podnet-vnet-mapped-{}", std::process::id());

    let (server, client) = pair(
        &shim.path,
        &prefix,
        &["4", "127.0.0.1", "17001"],
        &["6", "::ffff:127.0.0.1", "17001"],
    );
    assert!(
        client.contains(&format!("connect -> {prefix}/4/127.0.0.1/17001")),
        "{client}"
    );
    assert!(
        client.contains("peer 6 ::ffff:127.0.0.1 17001 28"),
        "{client}"
    );
    assert!(client.contains("local 6 ::ffff:10.66.0.2 "), "{client}");
    assert!(client.contains("echo 6 mapped"), "{client}");
    // The IPv4 listener sees an IPv4 peer: the autobind address of the
    // client in the IPv4 form.
    assert!(server.contains("accept 4 10.66.0.2 "), "{server}");
    assert!(server.contains("server-peer 4 10.66.0.2 "), "{server}");

    let (server, client) = pair(
        &shim.path,
        &prefix,
        &["6", "::ffff:127.0.0.1", "17002"],
        &["4", "127.0.0.1", "17002"],
    );
    assert!(
        server.contains(&format!("bind -> {prefix}/4/127.0.0.1/17002")),
        "{server}"
    );
    assert!(client.contains("peer 4 127.0.0.1 17002 16"), "{client}");
    assert!(client.contains("echo 6 mapped"), "{client}");
    // The IPv6 listener sees the IPv4 client in the mapped form.
    assert!(server.contains("accept 6 ::ffff:10.66.0.2 "), "{server}");
    assert!(server.contains(" 28\n"), "{server}");
}
