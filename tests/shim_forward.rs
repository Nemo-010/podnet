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

/// A client of each `FAMILY ADDR PORT` triple of its arguments: one line
/// `ADDR PORT ok FAMILY PEER PEERPORT domain D reply TEXT` or
/// `ADDR PORT fail ERRNO`. `D` is the real domain of the descriptor
/// (`SO_DOMAIN`, 1 is `AF_UNIX`).
const FORWARD_CLIENT: &str = r#"
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <arpa/inet.h>
#include <unistd.h>
int main(int argc,char**argv){
  for(int i=1;i+2<argc;i+=3){
    int fam=atoi(argv[i]),port=atoi(argv[i+2]);
    struct sockaddr_storage ss;socklen_t len;memset(&ss,0,sizeof ss);
    if(fam==6){struct sockaddr_in6*a=(void*)&ss;a->sin6_family=AF_INET6;a->sin6_port=htons(port);
      inet_pton(AF_INET6,argv[i+1],&a->sin6_addr);len=sizeof *a;}
    else{struct sockaddr_in*a=(void*)&ss;a->sin_family=AF_INET;a->sin_port=htons(port);
      inet_pton(AF_INET,argv[i+1],&a->sin_addr);len=sizeof *a;}
    int s=socket(fam==6?AF_INET6:AF_INET,SOCK_STREAM,0);
    if(connect(s,(void*)&ss,len)<0){
      printf("%s %d fail %s\n",argv[i+1],port,errno==ECONNREFUSED?"ECONNREFUSED":strerror(errno));
      close(s);continue;}
    struct sockaddr_storage got;socklen_t gl=sizeof got;char ip[64]="?";int pp=-1,pf=0;
    if(getpeername(s,(void*)&got,&gl)==0){
      if(got.ss_family==AF_INET){struct sockaddr_in*a=(void*)&got;pf=4;
        inet_ntop(AF_INET,&a->sin_addr,ip,sizeof ip);pp=ntohs(a->sin_port);}
      else if(got.ss_family==AF_INET6){struct sockaddr_in6*a=(void*)&got;pf=6;
        inet_ntop(AF_INET6,&a->sin6_addr,ip,sizeof ip);pp=ntohs(a->sin6_port);}}
    int dom=0;socklen_t dl=sizeof dom;getsockopt(s,SOL_SOCKET,SO_DOMAIN,&dom,&dl);
    write(s,"hi",2);char buf[64]={0};int n=read(s,buf,sizeof buf-1);
    printf("%s %d ok %d %s %d domain %d reply %s\n",argv[i+1],port,pf,ip,pp,dom,n>0?buf:"-");
    close(s);
  }
  return 0;
}
"#;

/// T-357: `PODNET_VNET_FORWARD` sends a stream connect to a unix socket
/// path that a plain process serves (here a `UnixListener` of the test).
/// The descriptor is `AF_UNIX`; `getpeername` gives the TCP address that the
/// client asked for. A mapped IPv6 address matches its IPv4 entry. A
/// forward comes before a SOCKS route. A bad entry is ignored and logged. A
/// destination with no forward goes to its abstract name, as before.
#[test]
fn a_forward_connects_to_a_unix_socket_path() {
    if !have_compiler() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let shim = shim::build(dir.path()).unwrap();
    let client = build_program(dir.path(), "fwd", FORWARD_CLIENT);
    let broker = dir.path().join("broker.sock");
    let listener = std::os::unix::net::UnixListener::bind(&broker).unwrap();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        for mut conn in listener.incoming().map_while(Result::ok) {
            let mut buf = [0u8; 64];
            let n = conn.read(&mut buf).unwrap_or(0);
            let mut reply = b"broker-".to_vec();
            reply.extend_from_slice(&buf[..n]);
            let _ = conn.write_all(&reply);
        }
    });
    let path = broker.to_string_lossy().into_owned();
    let forward = format!(
        "127.0.0.1:7001=unix:{path}, [::1]:3128=unix:{path},127.0.0.1:7003=unix:{}/none.sock,bad,127.0.0.1:7004=unix:relative",
        dir.path().display()
    );
    let output = Command::new(&client)
        .args([
            "4",
            "127.0.0.1",
            "7001",
            "6",
            "::ffff:127.0.0.1",
            "7001",
            "6",
            "::1",
            "3128",
            "4",
            "127.0.0.1",
            "7002",
            "4",
            "127.0.0.1",
            "7003",
            "4",
            "127.0.0.1",
            "7004",
        ])
        .env("LD_PRELOAD", &shim.path)
        .env(
            "PODNET_VNET_PREFIX",
            format!("podnet-vnet-fwd-{}", std::process::id()),
        )
        .env("PODNET_VNET_LOG", "1")
        .env("PODNET_VNET_FORWARD", &forward)
        // The SOCKS route takes 127.0.0.0/8 too: the forward comes first.
        .env("PODNET_VNET_SOCKS", dir.path().join("door.sock"))
        .env("PODNET_VNET_SOCKS_ROUTES", "127.0.0.0/8")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}{stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        [
            "127.0.0.1 7001 ok 4 127.0.0.1 7001 domain 1 reply broker-hi",
            "::ffff:127.0.0.1 7001 ok 6 ::ffff:127.0.0.1 7001 domain 1 reply broker-hi",
            "::1 3128 ok 6 ::1 3128 domain 1 reply broker-hi",
            "127.0.0.1 7002 fail No such file or directory",
            "127.0.0.1 7003 fail ECONNREFUSED",
            "127.0.0.1 7004 fail No such file or directory",
        ],
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!("connect -> forward {path}")),
        "{stderr}"
    );
    // 7002 and 7004 have no forward: the SOCKS route takes them, and the
    // door path does not exist (ENOENT, as the door path gives before).
    assert!(stderr.contains("connect -> door "), "{stderr}");
    assert!(stderr.contains("forward bad: not IPV4:PORT"), "{stderr}");
    assert!(
        stderr.contains("forward 127.0.0.1:7004=unix:relative: not IPV4:PORT"),
        "{stderr}"
    );
}

/// T-357: with no SOCKS route, a destination with no forward goes to its
/// abstract name: a listener of the shim on 127.0.0.1:7005 takes it.
#[test]
fn a_destination_with_no_forward_keeps_its_abstract_name() {
    if !have_compiler() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let shim = shim::build(dir.path()).unwrap();
    let prefix = format!("podnet-vnet-nofwd-{}", std::process::id());
    let program = build_program(dir.path(), "peer", PEER);
    let forward = format!("127.0.0.1:7001=unix:{}/none.sock", dir.path().display());
    let mut server_cmd = Command::new(&program);
    server_cmd
        .args(["server", "4", "127.0.0.1", "7005"])
        .env("LD_PRELOAD", &shim.path)
        .env("PODNET_VNET_PREFIX", &prefix);
    let mut client_cmd = Command::new(&program);
    client_cmd
        .args(["client", "6", "::ffff:127.0.0.1", "7005"])
        .env("LD_PRELOAD", &shim.path)
        .env("PODNET_VNET_PREFIX", &prefix)
        .env("PODNET_VNET_FORWARD", &forward);
    let (s, c) = run_pair(&mut server_cmd, &mut client_cmd);
    assert!(s.status.success() && c.status.success());
    assert!(String::from_utf8_lossy(&c.stdout).contains("echo 6 mapped"));
}
