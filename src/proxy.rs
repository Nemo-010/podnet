//! The services of podnet behind the tunnel of pod-cloudflare: the axum
//! routers (the static files, the file server, git, the shares) and the
//! share proxy with its PIN gate. pod-cloudflare parses the edge framing,
//! checks the credential in front of the tunnel, and serves the other
//! services; it hands a request for a service of podnet to [`Services`].

use std::sync::Arc;

use crate::cloudflare::http2::{connection_named, is_hop_by_hop};
use crate::cloudflare::stream as cqstream;
#[cfg(test)]
use crate::cloudflare::stream::HTTP_HOST_KEY;
use crate::cloudflare::stream::{HTTP_HEADER_KEY, HTTP_METHOD_KEY};
use anyhow::{anyhow, Result};
use axum::body::Body;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Request, Response};
use futures::future::BoxFuture;
use pod_core::{Class, Failure, Stage};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tower::ServiceExt;

use crate::gate::UNLOCK_PATH;
use crate::ingress::{OriginOptions, Service};
use crate::metrics::Metrics;
use crate::service::{Built, ServiceRuntime};
use crate::share::ShareControl;
use crate::util::path_and_query;

pub use pod_cloudflare::proxy::{
    content_length, forward_http, forward_tcp, header_value, is_websocket, serve_stream,
    write_response_meta, write_simple_response, write_simple_response_with, EdgeReader, EdgeWriter,
    Runtime, RPC_STREAM_REFUSED,
};

/// The services of podnet for the rules of one tunnel.
pub struct Services(pub Arc<ServiceRuntime>);

impl pod_cloudflare::ServiceDispatch for Services {
    fn prepare(&self, index: usize, service: &Service) -> Result<(), Failure> {
        self.0.prepare(index, service).map_err(|e| {
            let text = format!("{}: {e:#}", service.describe());
            Failure::caused(Stage::Host, Class::Config, text, e)
        })
    }

    fn serve<'a>(
        &'a self,
        index: usize,
        request: &'a cqstream::ConnectRequest,
        host: &'a str,
        reader: EdgeReader,
        writer: EdgeWriter,
    ) -> BoxFuture<'a, Result<(), Failure>> {
        Box::pin(async move {
            serve(&self.0, index, request, host, reader, writer)
                .await
                .map_err(|e| {
                    Failure::caused(Stage::Origin, Class::Unavailable, format!("{e:#}"), e)
                })
        })
    }
}

/// Answer one request of the edge with the service of podnet of rule
/// `index`.
async fn serve(
    runtime: &ServiceRuntime,
    index: usize,
    request: &cqstream::ConnectRequest,
    host: &str,
    reader: EdgeReader,
    writer: EdgeWriter,
) -> Result<()> {
    let built = runtime
        .get(index)
        .ok_or_else(|| anyhow!("no service of podnet for rule {index}"))?;
    match built.as_ref() {
        Built::Static(router) => {
            dispatch_axum(
                router.clone(),
                request,
                host,
                reader,
                writer,
                runtime.metrics(),
            )
            .await
        }
        Built::Share(app) => {
            dispatch_axum(
                app.router(),
                request,
                host,
                reader,
                writer,
                runtime.metrics(),
            )
            .await
        }
        Built::ShareProxy {
            target,
            gate,
            control,
            stop_after,
        } => {
            share_proxy(
                target,
                gate,
                control,
                *stop_after,
                request,
                host,
                reader,
                writer,
                runtime.metrics(),
            )
            .await
        }
    }
}

// ── Built-in (axum) dispatch ────────────────────────────────────────────────

async fn dispatch_axum(
    router: axum::Router,
    request: &cqstream::ConnectRequest,
    host: &str,
    reader: EdgeReader,
    mut writer: EdgeWriter,
    metrics: &Metrics,
) -> Result<()> {
    let uri = path_and_query(&request.dest);
    let method = request.meta(HTTP_METHOD_KEY).unwrap_or("GET");
    let builder = Request::builder().method(method).uri(uri);

    let mut headers = HeaderMap::new();
    let named = connection_named(
        request
            .metadata
            .iter()
            .filter(|(key, _)| {
                key.strip_prefix(&format!("{HTTP_HEADER_KEY}:"))
                    .is_some_and(|name| name.eq_ignore_ascii_case("connection"))
            })
            .map(|(_, value)| value.as_str()),
    );
    for (key, value) in &request.metadata {
        let Some(name) = key.strip_prefix(&format!("{HTTP_HEADER_KEY}:")) else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        if is_hop_by_hop(&lower) || named.contains(&lower) {
            continue;
        }
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            headers.append(name, value);
        }
    }
    if !headers.contains_key(axum::http::header::HOST) && !host.is_empty() {
        if let Ok(value) = HeaderValue::from_str(host) {
            headers.insert(axum::http::header::HOST, value);
        }
    }

    // The body goes through a pump (issue 92): when the service answers
    // before the end of the body (an early 413), the pump reads the rest and
    // drops it for a time, so that the stream is not reset before the edge
    // has the answer (a reset gave 502 to the visitor in place of the 413).
    let (tx, rx) = tokio::sync::mpsc::channel::<std::io::Result<bytes::Bytes>>(4);
    tokio::spawn(pump_body(reader, tx));
    let stream = futures::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    });
    let body = Body::from_stream(stream);
    let mut axum_request = builder.body(body)?;
    *axum_request.headers_mut() = headers;

    let response: Response<Body> = router
        .oneshot(axum_request)
        .await
        .map_err(|e| anyhow!("service error: {e}"))?;

    write_axum_response(&mut writer, response, metrics).await
}

/// The longest time that the rest of a body is read after an early answer.
const DRAIN_LIMIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Move the request body from the edge to `tx`. When the receiver is gone
/// (the service does not read more), read the rest and drop it, for at most
/// [`DRAIN_LIMIT`].
async fn pump_body(
    reader: EdgeReader,
    tx: tokio::sync::mpsc::Sender<std::io::Result<bytes::Bytes>>,
) {
    use tokio::io::AsyncReadExt as _;
    let mut reader = reader.compat();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => return,
            Ok(n) => {
                if tx
                    .send(Ok(bytes::Bytes::copy_from_slice(&buf[..n])))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Err(err) => {
                let _ = tx.send(Err(err)).await;
                return;
            }
        }
    }
    let drained = tokio::time::timeout(DRAIN_LIMIT, async {
        let mut total = 0u64;
        while let Ok(n) = reader.read(&mut buf).await {
            if n == 0 {
                break;
            }
            total += n as u64;
        }
        total
    })
    .await;
    tracing::debug!(?drained, "the rest of a request body after an early answer");
}

async fn write_axum_response(
    writer: &mut EdgeWriter,
    response: Response<Body>,
    metrics: &Metrics,
) -> Result<()> {
    let status = response.status().as_u16();
    let mut headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_string(),
                // A value with bytes above 127 is kept (as UTF-8, lossy):
                // to_str() refuses it, and the header was lost.
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect();

    let has_len = headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("content-length"));
    let has_chunked = headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("transfer-encoding")
            && value.to_ascii_lowercase().contains("chunked")
    });

    let body = response.into_body();
    if !has_len && !has_chunked {
        // A body of a known size (the small built-in pages) gets a
        // Content-Length. A body of no known size is not buffered (issue
        // 243): it is streamed, and the edge frames it.
        if let Some(len) = axum::body::HttpBody::size_hint(&body).exact() {
            headers.push(("Content-Length".to_string(), len.to_string()));
        }
    }
    write_response_meta(writer, status, &headers).await?;
    use futures::StreamExt;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| anyhow!("response body: {e}"))?;
        futures::io::AsyncWriteExt::write_all(writer, &chunk).await?;
        futures::io::AsyncWriteExt::flush(writer).await?;
        metrics.add_out(chunk.len() as u64);
    }
    futures::io::AsyncWriteExt::close(writer).await?;
    Ok(())
}

// ── quickbridge proxy mode ──────────────────────────────────────────────────

/// The origin of a proxy share: `unix:/PATH` (issue 204) or `host:port`.
pub fn share_origin(target: &str) -> Service {
    match target.strip_prefix("unix:") {
        Some(path) => Service::Unix(path.into()),
        None => Service::Http(format!("http://{target}")),
    }
}

#[allow(clippy::too_many_arguments)]
async fn share_proxy(
    target: &str,
    gate: &crate::gate::Gate,
    control: &ShareControl,
    stop_after: bool,
    request: &cqstream::ConnectRequest,
    host: &str,
    mut reader: EdgeReader,
    mut writer: EdgeWriter,
    metrics: &Metrics,
) -> Result<()> {
    let path = path_and_query(&request.dest);
    let method = request.meta(HTTP_METHOD_KEY).unwrap_or("GET");
    let headers = headers_from_request(request);

    if path == UNLOCK_PATH && method.eq_ignore_ascii_case("POST") {
        let len = content_length(request).unwrap_or(0);
        if len > crate::gate::MAX_UNLOCK_BODY {
            return reply(
                &mut writer,
                413,
                "text/plain; charset=utf-8",
                b"the unlock request is too large",
            )
            .await;
        }
        let body = read_body(&mut reader, len).await?;
        // Issue 229: an unlock from another site does not use a try of the
        // PIN.
        if !crate::gate::same_site(&headers, Some(host)) {
            return reply(
                &mut writer,
                403,
                "text/plain; charset=utf-8",
                b"an unlock from another site is refused",
            )
            .await;
        }
        let fields = parse_form(&body);
        let password = fields
            .iter()
            .find(|(k, _)| k == "password")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        let next = fields
            .iter()
            .find(|(k, _)| k == "next")
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| "/".to_string());
        return match gate.unlock(&password) {
            Some(cookie) => {
                reply_with(
                    &mut writer,
                    303,
                    "text/plain; charset=utf-8",
                    b"",
                    &[
                        (
                            "Location".to_string(),
                            crate::gate::safe_next(&next).to_string(),
                        ),
                        ("Set-Cookie".to_string(), cookie),
                    ],
                )
                .await
            }
            None => {
                let html = gate.page_html(&next, true);
                reply(
                    &mut writer,
                    403,
                    "text/html; charset=utf-8",
                    html.as_bytes(),
                )
                .await
            }
        };
    }

    if !gate.is_open(&headers) {
        let html = gate.page_html(&path, false);
        return reply(
            &mut writer,
            crate::gate::locked_status(method),
            "text/html; charset=utf-8",
            html.as_bytes(),
        )
        .await;
    }

    control.touch();
    let service = share_origin(target);
    let result = forward_http(
        crate::origin::dial::origins(),
        &service,
        &OriginOptions::default(),
        request,
        reader,
        writer,
        metrics,
    )
    .await
    .map_err(anyhow::Error::from);
    // --stop-after: the first request that the origin answered in full
    // ends the session. A HEAD request does not count.
    if stop_after && result.is_ok() && !method.eq_ignore_ascii_case("HEAD") {
        control.request_stop();
    }
    result
}

/// [`write_simple_response`] with the error of podnet.
async fn reply(
    writer: &mut EdgeWriter,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    Ok(write_simple_response(writer, status, content_type, body).await?)
}

/// [`write_simple_response_with`] with the error of podnet.
async fn reply_with(
    writer: &mut EdgeWriter,
    status: u16,
    content_type: &str,
    body: &[u8],
    extra: &[(String, String)],
) -> Result<()> {
    Ok(write_simple_response_with(writer, status, content_type, body, extra).await?)
}

fn headers_from_request(request: &cqstream::ConnectRequest) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (key, value) in &request.metadata {
        let Some(name) = key.strip_prefix(&format!("{HTTP_HEADER_KEY}:")) else {
            continue;
        };
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            headers.append(name, value);
        }
    }
    headers
}

async fn read_body(reader: &mut EdgeReader, len: u64) -> Result<Vec<u8>> {
    let mut body = vec![0u8; len as usize];
    if len > 0 {
        futures::io::AsyncReadExt::read_exact(reader, &mut body).await?;
    }
    Ok(body)
}

fn parse_form(body: &[u8]) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(body);
    text.split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            Some((percent_decode(key), percent_decode(value)))
        })
        .collect()
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    index += 3;
                } else {
                    out.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

// `connect.rs` is the HTTP CONNECT client (a dial through a proxy). This
// module serves the edge streams. These re-exports keep the old paths
// (`crate::proxy::connect_tunnel` and others).
pub use crate::connect::{
    base64_encode, base64_encode_pub, connect_tunnel, connect_tunnel_with_auth, strip_scheme,
    TunnelStream,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue 92: on one QUIC connection in memory, three share uploads of
    /// 60 MB each get 200 and are on disk; a Content-Length over
    /// --max-request gets 413 before the body; the connection stays open.
    #[tokio::test(flavor = "multi_thread")]
    async fn large_share_uploads_on_one_quic_connection() {
        crate::test_host::need_loopback!();
        use crate::cloudflare::schema::ConnectionType;
        use crate::cloudflare::stream::HTTP_STATUS_KEY;
        use futures::io::AsyncWriteExt as _;
        use std::sync::Arc;
        use std::time::Duration;
        use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
        let pair = pod_cloudflare::standin::quic::local_pair().await;
        let dir = tempfile::tempdir().unwrap();
        let config = crate::share::ShareConfig::upload(dir.path());
        let token = config.token.clone();
        let ingress =
            Arc::new(crate::ingress::Ingress::new().catch_all(crate::ingress::share(config)));
        let hosted = crate::tunnel::hosted().unwrap();
        let runtime = Arc::new(Runtime::build(hosted.host, &ingress, hosted.metrics).unwrap());
        let client = pair.client.clone();
        let server = tokio::spawn(async move {
            while let Ok((send, recv)) = client.accept_bi().await {
                tokio::spawn(serve_stream(runtime.clone(), ingress.clone(), send, recv));
            }
        });
        const SIZE: usize = 60 * 1024 * 1024;
        let boundary = "XBOUNDARYX";
        let post = |name: &str, length: usize| cqstream::ConnectRequest {
            dest: format!("http://x.trycloudflare.com/s/{token}/upload?{name}"),
            conn_type: ConnectionType::Http,
            metadata: vec![
                (HTTP_METHOD_KEY.into(), "POST".into()),
                (HTTP_HOST_KEY.into(), "x.trycloudflare.com".into()),
                (
                    format!("{HTTP_HEADER_KEY}:Content-Type"),
                    format!("multipart/form-data; boundary={boundary}"),
                ),
                (
                    format!("{HTTP_HEADER_KEY}:Content-Length"),
                    length.to_string(),
                ),
            ],
        };
        for round in 0..3 {
            let head = format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"files\"; \
                 filename=\"f{round}.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n"
            );
            let tail = format!("\r\n--{boundary}--\r\n");
            let total = head.len() + SIZE + tail.len();
            let (send, recv) = pair.edge.open_bi().await.unwrap();
            let mut writer = send.compat_write();
            let mut reader = recv.compat();
            cqstream::write_connect_request(&mut writer, &post("a", total))
                .await
                .unwrap();
            let body = tokio::spawn(async move {
                writer.write_all(head.as_bytes()).await.unwrap();
                let chunk = vec![7u8; 64 * 1024];
                let mut left = SIZE;
                while left > 0 {
                    let n = left.min(chunk.len());
                    writer.write_all(&chunk[..n]).await.unwrap();
                    left -= n;
                }
                writer.write_all(tail.as_bytes()).await.unwrap();
                writer.close().await.unwrap();
            });
            let (error, meta) = tokio::time::timeout(
                Duration::from_secs(120),
                cqstream::read_connect_response(&mut reader),
            )
            .await
            .expect("an answer to the upload")
            .unwrap();
            assert!(error.is_empty(), "round {round}: {error}");
            assert!(
                meta.contains(&(HTTP_STATUS_KEY.to_string(), "200".to_string())),
                "round {round}: {meta:?}"
            );
            body.await.unwrap();
            let len = std::fs::metadata(dir.path().join(format!("f{round}.bin")))
                .unwrap()
                .len();
            assert_eq!(len, SIZE as u64);
        }
        // Over the limit: 413 at once, with no body sent.
        let (send, recv) = pair.edge.open_bi().await.unwrap();
        let mut writer = send.compat_write();
        let mut reader = recv.compat();
        cqstream::write_connect_request(&mut writer, &post("b", 150_000_000))
            .await
            .unwrap();
        let (_, meta) = tokio::time::timeout(
            Duration::from_secs(10),
            cqstream::read_connect_response(&mut reader),
        )
        .await
        .expect("a 413 before the body")
        .unwrap();
        assert!(
            meta.contains(&(HTTP_STATUS_KEY.to_string(), "413".to_string())),
            "{meta:?}"
        );
        // The rest of the body is read after the answer (it is not reset
        // at once, which gave 502 at the edge): 2 MiB more go through.
        let chunk = vec![1u8; 64 * 1024];
        for _ in 0..32 {
            writer
                .write_all(&chunk)
                .await
                .expect("the stream takes the rest of the body");
        }
        assert!(pair.client.close_reason().is_none());
        server.abort();
    }

    /// Issue 243: a built-in body of no known size is streamed, not
    /// buffered to its end. A body of a known size gets a Content-Length.
    #[tokio::test]
    async fn a_built_in_body_of_no_known_size_is_streamed() {
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(4);
        let body = Body::from_stream(tokio_stream_from(rx));
        let response = Response::builder().status(200).body(body).unwrap();
        let (near, far) = tokio::io::duplex(1 << 16);
        let mut writer: EdgeWriter = Box::pin(
            tokio_util::compat::TokioAsyncWriteCompatExt::compat_write(near),
        );
        let task = tokio::spawn(async move {
            write_axum_response(&mut writer, response, &Metrics::new()).await
        });
        tx.send(Ok(bytes::Bytes::from_static(b"first")))
            .await
            .unwrap();
        let mut far = tokio_util::compat::TokioAsyncReadCompatExt::compat(far);
        let (_, meta) = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            cqstream::read_connect_response(&mut far),
        )
        .await
        .expect("the head comes before the end of the body")
        .unwrap();
        assert!(
            !meta
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("HttpHeader:Content-Length")),
            "{meta:?}"
        );
        let mut got = [0u8; 5];
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            futures::io::AsyncReadExt::read_exact(&mut far, &mut got),
        )
        .await
        .expect("the first chunk comes before the end of the body")
        .unwrap();
        assert_eq!(&got, b"first");
        drop(tx);
        task.await.unwrap().unwrap();

        // A body of a known size.
        let response = Response::builder()
            .status(200)
            .body(Body::from("hello"))
            .unwrap();
        let (near, far) = tokio::io::duplex(1 << 16);
        let mut writer: EdgeWriter = Box::pin(
            tokio_util::compat::TokioAsyncWriteCompatExt::compat_write(near),
        );
        write_axum_response(&mut writer, response, &Metrics::new())
            .await
            .unwrap();
        let mut far = tokio_util::compat::TokioAsyncReadCompatExt::compat(far);
        let (_, meta) = cqstream::read_connect_response(&mut far).await.unwrap();
        assert!(
            meta.contains(&("HttpHeader:Content-Length".to_string(), "5".to_string())),
            "{meta:?}"
        );
    }

    /// A stream of the items of `rx`.
    fn tokio_stream_from<T: Send + 'static>(
        rx: tokio::sync::mpsc::Receiver<T>,
    ) -> impl futures::Stream<Item = T> + Send + 'static {
        futures::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|v| (v, rx)) })
    }

    // Issue 204: the origin of a proxy share.
    #[test]
    fn the_origin_of_a_proxy_share() {
        assert!(
            matches!(share_origin("unix:/run/a.sock"), Service::Unix(p) if p == std::path::Path::new("/run/a.sock"))
        );
        assert!(
            matches!(share_origin("127.0.0.1:3000"), Service::Http(u) if u == "http://127.0.0.1:3000")
        );
    }

    #[test]
    fn form_parsing() {
        let fields = parse_form(b"password=123456&next=%2Fs%2Fabc%2F&x=a+b");
        assert!(fields.contains(&("password".to_string(), "123456".to_string())));
        assert!(fields.contains(&("next".to_string(), "/s/abc/".to_string())));
        assert!(fields.contains(&("x".to_string(), "a b".to_string())));
    }
}
