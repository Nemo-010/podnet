//! `podnet exit` (T-329): one SOCKS5 session on stdin and stdout.

use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub(crate) struct ExitArgs {
    /// Serve one SOCKS5 session on stdin and stdout, with no listener. The way
    /// out is the way of the process (--proxy, --no-proxy, --egress, the
    /// variables). CONNECT: the stream is the session. UDP: a pipe has no UDP
    /// socket. So the client sends the command 0x83 (UDP in the stream), and
    /// each datagram in each direction is a frame on the stream: 2 bytes of
    /// length (big endian), then a SOCKS5 UDP datagram. UDP ASSOCIATE (command
    /// 3) is refused with the reply 0x07. UDP leaves from this host when the
    /// process has no proxy; with a proxy it is refused (reply 0x07). The run
    /// ends when the client closes stdin. This is the only mode and the
    /// default: the flag is optional.
    #[arg(long)]
    stdio: bool,
}

/// Run the exit.
pub(crate) async fn exit(args: ExitArgs) -> Result<()> {
    // Stdio is the only mode (#623): the flag is optional.
    let _ = args.stdio;
    let io = super::connect_stdio::StdioIo::new()?;
    let served = podnet::exit_stdio::serve(
        io,
        std::sync::Arc::new(podnet::exit_stdio::ExitDial::process()),
    )
    .await;
    match served {
        Err(e) if closed_before_first_byte(&e) => Ok(()),
        other => other,
    }
}

/// Whether the client closed stdin before it sent a byte: an empty session
/// is a clean end, not a fault (#623).
fn closed_before_first_byte(e: &anyhow::Error) -> bool {
    e.to_string() == "the first byte"
        && e.root_cause()
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::UnexpectedEof)
}
