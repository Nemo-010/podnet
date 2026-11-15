//! `devtunnel://TUNNEL` (T-320, T-354): the spec, the account, and the
//! streams against the stand-in of `pod-devtunnel` on the loopback (no
//! account and no live service).

use std::io::ErrorKind;
use std::time::Duration;

use pod_core::Class;
use pod_devtunnel::standin::{Plan, Standin};
use pod_devtunnel::{AccessToken, Credential};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::Instant;

use super::devtunnel::{self, Account};
use super::*;
use crate::network::Connected;

const TOKEN: &str = "made-up-user-token-of-the-tests";
const ACCESS: &str = "made-up-access-token-of-the-tests";
const WAIT: Duration = Duration::from_secs(10);

fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.to_string())
    }
}

async fn up(standin: &Standin) -> Result<Link, Failure> {
    let spec = devtunnel::parse("devtunnel://standin-box").unwrap();
    devtunnel::up(&spec, Account::new(TOKEN, Some(&standin.base())), None).await
}

fn dial_of(link: &Link) -> std::sync::Arc<dyn Dial> {
    link.way.as_ref().expect("a way").dial.clone()
}

async fn open(link: &Link, port: u16) -> Result<Connected, Failure> {
    let target = Target::new("127.0.0.1", port);
    dial_of(link).dial(&target, Instant::now() + WAIT).await
}

#[test]
fn devtunnel_spec_forms_and_the_row() {
    for (spec, label) in [
        ("devtunnel://lab", "devtunnel://lab"),
        ("devtunnel://Lab", "devtunnel://lab"),
        ("devtunnel://bcd123.usw2", "devtunnel://bcd123.usw2"),
    ] {
        let network = parse(spec).unwrap();
        assert_eq!(network.label(), label);
        assert_eq!(network.scheme(), "devtunnel");
        assert_eq!(network.row().scheme, "devtunnel");
        assert!(network.caps().contains(Caps::EGRESS_TCP));
        assert!(!network.caps().contains(Caps::EGRESS_UDP));
        assert_eq!(network.devtunnel().unwrap(), label["devtunnel://".len()..]);
    }
}

#[test]
fn devtunnel_spec_bad_forms_are_usage_and_hold_no_secret() {
    for spec in [
        "devtunnel://",
        "devtunnel://a.b.c",
        "devtunnel://-x",
        "devtunnel://lab/extra",
        "devtunnel://lab?token=hunter2",
        "devtunnel://lab:22",
        "devtunnel://user:hunter2@lab",
        "devtunnel://hunter2@lab",
    ] {
        let err = parse(spec).unwrap_err();
        assert_eq!(
            (err.stage, err.class),
            (Stage::Connect, Class::Usage),
            "{spec}"
        );
        assert!(!err.to_string().contains("hunter2"), "{spec}: {err}");
    }
}

#[test]
fn devtunnel_account_reads_the_file_or_the_variable_and_names_the_variables() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("token");
    std::fs::write(&file, format!("{TOKEN}\n")).unwrap();
    let file = file.to_str().unwrap();
    let ok = devtunnel::account(&env(&[("PODNET_DEVTUNNEL_TOKEN_FILE", file)])).unwrap();
    assert!(!format!("{ok:?}").contains(TOKEN), "{ok:?}");
    devtunnel::account(&env(&[
        ("PODNET_DEVTUNNEL_TOKEN", TOKEN),
        ("PODNET_DEVTUNNEL_SCHEME", "AAD"),
    ]))
    .unwrap();
    let none = devtunnel::account(&env(&[])).unwrap_err();
    assert!(
        none.to_string().contains("PODNET_DEVTUNNEL_TOKEN_FILE"),
        "{none}"
    );
    assert_eq!(none.class, Class::Usage);
    let both = devtunnel::account(&env(&[
        ("PODNET_DEVTUNNEL_TOKEN", TOKEN),
        ("PODNET_DEVTUNNEL_TOKEN_FILE", file),
    ]))
    .unwrap_err();
    assert!(both.to_string().contains("not both"), "{both}");
    let scheme = devtunnel::account(&env(&[
        ("PODNET_DEVTUNNEL_TOKEN", TOKEN),
        ("PODNET_DEVTUNNEL_SCHEME", "x"),
    ]))
    .unwrap_err();
    assert!(
        scheme.to_string().contains("PODNET_DEVTUNNEL_SCHEME"),
        "{scheme}"
    );
    // A bad token: the fault names the rule and never the text.
    let bad = devtunnel::account(&env(&[("PODNET_DEVTUNNEL_TOKEN", "has a space")])).unwrap_err();
    assert!(!bad.to_string().contains("has a space"), "{bad}");
    let base = devtunnel::account(&env(&[
        ("PODNET_DEVTUNNEL_TOKEN", TOKEN),
        ("PODNET_DEVTUNNEL_MANAGEMENT", "http://tunnels.example"),
    ]))
    .unwrap_err();
    assert!(
        base.to_string().contains("PODNET_DEVTUNNEL_MANAGEMENT"),
        "{base}"
    );
}

#[tokio::test]
async fn devtunnel_stream_echoes_and_status_holds_no_token() {
    crate::test_host::need_loopback!();
    let standin = Standin::start(Plan::echo("github", TOKEN)).await.unwrap();
    let link = up(&standin).await.unwrap_or_else(|e| panic!("{e}"));
    let status = link.status.to_string();
    assert!(status.contains("devtunnel://standin-box"), "{status}");
    assert!(
        !status.contains(TOKEN) && !status.contains("standin-connect-"),
        "{status}"
    );
    let mut io = open(&link, 22).await.unwrap_or_else(|e| panic!("{e}")).io;
    io.write_all(b"hello").await.unwrap();
    let mut back = [0u8; 5];
    io.read_exact(&mut back).await.unwrap();
    assert_eq!(&back, b"hello");
    // The stand-in saw the SSH user `none`-authenticated channel to port 22.
    assert_eq!(standin.seen().channels.len(), 1);
}

#[tokio::test]
async fn devtunnel_reconnect_ends_a_stream_with_a_named_error_and_a_new_stream_works() {
    crate::test_host::need_loopback!();
    let standin = Standin::start(Plan::echo("github", TOKEN)).await.unwrap();
    let link = up(&standin).await.unwrap_or_else(|e| panic!("{e}"));
    let mut old = open(&link, 22).await.unwrap().io;
    old.write_all(b"a").await.unwrap();
    old.read_exact(&mut [0u8; 1]).await.unwrap();
    standin.drop_relays();
    let err = tokio::time::timeout(WAIT, old.read(&mut [0u8; 8]))
        .await
        .expect("the read ends")
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::ConnectionAborted);
    assert_eq!(err.to_string(), pod_devtunnel::stream::SESSION_ENDED);
    let mut new = open(&link, 22).await.unwrap_or_else(|e| panic!("{e}")).io;
    new.write_all(b"again").await.unwrap();
    let mut back = [0u8; 5];
    new.read_exact(&mut back).await.unwrap();
    assert_eq!(&back, b"again");
    assert_eq!(standin.minted(), 2);
}

#[tokio::test]
async fn devtunnel_failures_name_their_step() {
    crate::test_host::need_loopback!();
    let standin = Standin::start(Plan::echo("github", TOKEN)).await.unwrap();
    // The management API refuses the token: the step is the management API.
    let spec = devtunnel::parse("devtunnel://standin-box").unwrap();
    let wrong = Account::new("another-made-up-token", Some(&standin.base()));
    let err = devtunnel::up(&spec, wrong, None).await.unwrap_err();
    assert_eq!(err.class, Class::Refused, "{err}");
    assert!(
        err.to_string().contains("Dev Tunnels management API"),
        "{err}"
    );
    assert!(!err.to_string().contains("another-made-up-token"), "{err}");
    // A tunnel that the service does not know.
    let spec = devtunnel::parse("devtunnel://nobody").unwrap();
    let err = devtunnel::up(&spec, Account::new(TOKEN, Some(&standin.base())), None)
        .await
        .unwrap_err();
    assert_eq!(err.class, Class::NotFound, "{err}");
    // A port that the host does not serve: the step is the channel.
    let link = up(&standin).await.unwrap();
    let err = open(&link, 9).await.unwrap_err();
    assert!(
        err.to_string().contains("channel of the Dev Tunnels relay"),
        "{err}"
    );
}

#[tokio::test]
async fn devtunnel_target_host_is_the_local_address_of_the_tunnel_host() {
    crate::test_host::need_loopback!();
    let standin = Standin::start(Plan::echo("github", TOKEN)).await.unwrap();
    let link = up(&standin).await.unwrap();
    let target = Target::new("db.example", 22);
    let err = dial_of(&link)
        .dial(&target, Instant::now() + WAIT)
        .await
        .unwrap_err();
    assert_eq!(err.class, Class::Unsupported);
    assert!(err.to_string().contains("local address"), "{err}");
    let local = Target::new("localhost", 22);
    dial_of(&link)
        .dial(&local, Instant::now() + WAIT)
        .await
        .unwrap();
}

#[test]
fn devtunnel_account_reads_each_form_and_its_debug_and_errors_hold_no_token() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("access");
    std::fs::write(&file, format!("{ACCESS}\n")).unwrap();
    let file = file.to_str().unwrap();
    for (pairs, form) in [
        (
            vec![("PODNET_DEVTUNNEL_TOKEN", TOKEN)],
            "a GitHub user token",
        ),
        (
            vec![("PODNET_DEVTUNNEL_ACCESS_TOKEN_FILE", file)],
            "a tunnel access token",
        ),
        (
            vec![("PODNET_DEVTUNNEL_ACCESS_TOKEN", ACCESS)],
            "a tunnel access token",
        ),
        (vec![("PODNET_DEVTUNNEL_ANONYMOUS", "1")], "no token"),
        (
            vec![
                ("PODNET_DEVTUNNEL_ANONYMOUS", "0"),
                ("PODNET_DEVTUNNEL_ACCESS_TOKEN", ACCESS),
            ],
            "a tunnel access token",
        ),
    ] {
        let account = devtunnel::account(&env(&pairs)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(account.form(), form, "{pairs:?}");
        let shown = format!("{account:?}");
        assert!(!shown.contains(TOKEN) && !shown.contains(ACCESS), "{shown}");
        assert!(shown.contains(form), "{shown}");
    }
}

#[test]
fn devtunnel_account_refuses_two_forms_and_names_the_variables_not_the_values() {
    for pairs in [
        vec![
            ("PODNET_DEVTUNNEL_TOKEN", TOKEN),
            ("PODNET_DEVTUNNEL_ACCESS_TOKEN", ACCESS),
        ],
        vec![
            ("PODNET_DEVTUNNEL_TOKEN", TOKEN),
            ("PODNET_DEVTUNNEL_ANONYMOUS", "1"),
        ],
        vec![
            ("PODNET_DEVTUNNEL_ACCESS_TOKEN", ACCESS),
            ("PODNET_DEVTUNNEL_ANONYMOUS", "1"),
        ],
        vec![
            ("PODNET_DEVTUNNEL_ACCESS_TOKEN", ACCESS),
            ("PODNET_DEVTUNNEL_ACCESS_TOKEN_FILE", "/nonexistent"),
        ],
    ] {
        let err = devtunnel::account(&env(&pairs)).unwrap_err();
        assert_eq!(err.class, Class::Usage, "{pairs:?}: {err}");
        let text = format!("{err} {err:?}");
        assert!(!text.contains(TOKEN) && !text.contains(ACCESS), "{text}");
        assert!(text.contains("PODNET_DEVTUNNEL_ACCESS_TOKEN"), "{text}");
    }
    // No form: the message names each setting.
    let none = devtunnel::account(&env(&[])).unwrap_err();
    for name in [
        "PODNET_DEVTUNNEL_TOKEN_FILE",
        "PODNET_DEVTUNNEL_ACCESS_TOKEN_FILE",
        "PODNET_DEVTUNNEL_ANONYMOUS=1",
    ] {
        assert!(none.to_string().contains(name), "{none}");
    }
    // A scheme with no user token, a bad switch, a bad access token.
    for (pairs, name) in [
        (
            vec![
                ("PODNET_DEVTUNNEL_SCHEME", "aad"),
                ("PODNET_DEVTUNNEL_ANONYMOUS", "1"),
            ],
            "PODNET_DEVTUNNEL_SCHEME",
        ),
        (
            vec![("PODNET_DEVTUNNEL_ANONYMOUS", "yes")],
            "PODNET_DEVTUNNEL_ANONYMOUS",
        ),
        (
            vec![("PODNET_DEVTUNNEL_ACCESS_TOKEN", "has a space")],
            "PODNET_DEVTUNNEL_ACCESS_TOKEN",
        ),
    ] {
        let err = devtunnel::account(&env(&pairs)).unwrap_err();
        assert_eq!(err.class, Class::Usage, "{pairs:?}: {err}");
        assert!(err.to_string().contains(name), "{err}");
        assert!(!err.to_string().contains("has a space"), "{err}");
    }
}

/// The stand-in tunnel that takes the tunnel access token `ACCESS` and,
/// with `anonymous`, a caller with no token.
fn open_plan(anonymous: bool, mints_for_all: bool) -> Plan {
    Plan {
        access: Some(ACCESS.into()),
        anonymous,
        mints_for_all,
        ..Plan::echo("github", TOKEN)
    }
}

async fn up_with(standin: &Standin, credential: Credential) -> Result<Link, Failure> {
    let spec = devtunnel::parse("devtunnel://standin-box").unwrap();
    let account = Account::with(credential, Some(&standin.base()));
    devtunnel::up(&spec, account, None).await
}

async fn echo(link: &Link) {
    let mut io = open(link, 22).await.unwrap_or_else(|e| panic!("{e}")).io;
    io.write_all(b"hello").await.unwrap();
    let mut back = [0u8; 5];
    io.read_exact(&mut back).await.unwrap();
    assert_eq!(&back, b"hello");
}

#[tokio::test]
async fn devtunnel_a_tunnel_access_token_dials_and_the_status_names_the_form() {
    crate::test_host::need_loopback!();
    for mints in [false, true] {
        let standin = Standin::start(open_plan(false, mints)).await.unwrap();
        let access = AccessToken::new(ACCESS).unwrap();
        let link = up_with(&standin, access.into())
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        let status = link.status.to_string();
        assert!(status.contains("a tunnel access token"), "{status}");
        assert!(!status.contains(ACCESS), "{status}");
        echo(&link).await;
        let seen = standin.seen();
        assert_eq!(seen.forms, ["access"], "{seen:?}");
        let relay = if mints { "fresh" } else { "access" };
        assert_eq!(seen.relay_tokens, [relay], "{seen:?}");
    }
}

#[tokio::test]
async fn devtunnel_no_token_dials_a_tunnel_with_anonymous_access() {
    crate::test_host::need_loopback!();
    let standin = Standin::start(open_plan(true, false)).await.unwrap();
    let link = up_with(&standin, Credential::Anonymous)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(link.status.to_string().contains("no token"));
    echo(&link).await;
    let seen = standin.seen();
    assert_eq!(seen.forms, ["none"], "{seen:?}");
    assert_eq!(seen.relay_tokens, ["none"], "{seen:?}");
}

#[tokio::test]
async fn devtunnel_a_refused_access_token_or_no_token_is_refused_with_its_form() {
    crate::test_host::need_loopback!();
    // The tunnel takes ACCESS and no anonymous caller.
    let standin = Standin::start(open_plan(false, false)).await.unwrap();
    let other = "another-made-up-access-token";
    let err = up_with(&standin, AccessToken::new(other).unwrap().into())
        .await
        .unwrap_err();
    // The class of a refused user token (`devtunnel_failures_name_their_step`).
    assert_eq!(err.class, Class::Refused, "{err}");
    let text = format!("{err} {err:?}");
    assert!(text.contains("Dev Tunnels management API"), "{text}");
    assert!(!text.contains(other) && !text.contains(ACCESS), "{text}");
    let err = up_with(&standin, Credential::Anonymous).await.unwrap_err();
    assert_eq!(err.class, Class::Refused, "{err}");
    assert!(!err.to_string().contains(ACCESS), "{err}");
}
