//! Dev Tunnels as a network: `devtunnel://TUNNEL` (T-320).
//!
//! The network gives TCP to a port of the host of a Microsoft Dev Tunnels
//! tunnel, for a machine that reaches only the relay of the service on
//! TCP 443. Each [`Dial::dial`] call opens one `direct-tcpip` channel of the
//! SSH session that `pod-devtunnel` holds over the relay WebSocket. podnet
//! has no Dev Tunnels code of its own: the management read, the relay, SSH
//! and the reconnect are in the crate.
//!
//! * `devtunnel://NAME` or `devtunnel://ID.CLUSTER` names the tunnel.
//! * The port of the target is the port on the tunnel host. The host of the
//!   target is the local address of the tunnel host: `127.0.0.1`, `::1` or
//!   `localhost`.
//! * The credential is a setting, not a part of the spec (a spec goes into
//!   `argv`, logs and JSON). Give one form (T-354):
//!   * a user token: `PODNET_DEVTUNNEL_TOKEN_FILE` or
//!     `PODNET_DEVTUNNEL_TOKEN`, with `PODNET_DEVTUNNEL_SCHEME` (`github`,
//!     `aad` or `bearer`; default `github`);
//!   * a tunnel access token that the owner of the tunnel gives:
//!     `PODNET_DEVTUNNEL_ACCESS_TOKEN_FILE` or
//!     `PODNET_DEVTUNNEL_ACCESS_TOKEN`;
//!   * no token, for a tunnel with anonymous access:
//!     `PODNET_DEVTUNNEL_ANONYMOUS=1`.
//!
//!   More than one form is a usage error that names the variables. The
//!   crate sends each form to the management API, so no form needs
//!   another. The connect token comes from the connect information for
//!   each attempt, inside the crate. No text of this module holds a token.
//! * `PODNET_DEVTUNNEL_MANAGEMENT` replaces the management API (a base
//!   `https://HOST[:PORT]`, or `http://` to a loopback host): the default is
//!   the fact of `pod-services`.
//!
//! A reconnect ends each open stream with an error of the kind
//! `ConnectionAborted`; a new stream after it connects again.

use std::fmt;
use std::sync::Arc;

use pod_devtunnel::{
    AccessToken, Base, Credential, Locator, Relay, Scheme, Settings, UserToken, Way,
};
use tokio::time::Instant;

use super::usage;
use crate::network::Way as NetWay;
use crate::network::{BoxFuture, Check, Class, Connected, Dial, Failure, Link, Stage, Target};

/// The variable of the file that holds the management token (one line).
pub const TOKEN_FILE_VARIABLE: &str = "PODNET_DEVTUNNEL_TOKEN_FILE";
/// The variable of the management token. The file is the better choice.
pub const TOKEN_VARIABLE: &str = "PODNET_DEVTUNNEL_TOKEN";
/// The variable of the scheme of the token: `github`, `aad` or `bearer`.
pub const SCHEME_VARIABLE: &str = "PODNET_DEVTUNNEL_SCHEME";
/// The variable of the file that holds a tunnel access token (one line).
pub const ACCESS_FILE_VARIABLE: &str = "PODNET_DEVTUNNEL_ACCESS_TOKEN_FILE";
/// The variable of a tunnel access token. The file is the better choice.
pub const ACCESS_VARIABLE: &str = "PODNET_DEVTUNNEL_ACCESS_TOKEN";
/// The variable that selects no token (`1`): a tunnel with anonymous access.
pub const ANONYMOUS_VARIABLE: &str = "PODNET_DEVTUNNEL_ANONYMOUS";
/// The variable of the base of the management API.
pub const MANAGEMENT_VARIABLE: &str = "PODNET_DEVTUNNEL_MANAGEMENT";

/// A tunnel that a spec names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Spec {
    pub(super) locator: Locator,
}

impl Spec {
    /// The spec as it is shown: the scheme and the tunnel.
    pub(super) fn label(&self) -> String {
        format!("devtunnel://{}", self.locator)
    }
}

/// True when `spec` is a Dev Tunnels spec.
pub(super) fn is_devtunnel(spec: &str) -> bool {
    spec.starts_with("devtunnel://")
}

/// Parse `devtunnel://NAME` or `devtunnel://ID.CLUSTER`. The parse reads no
/// variable and opens no file. A fault never repeats text after an `@`.
pub(super) fn parse(spec: &str) -> Result<Spec, Failure> {
    let rest = spec.strip_prefix("devtunnel://").unwrap_or(spec);
    if rest.contains('@') {
        return Err(usage(format!(
            "devtunnel://***: a spec holds no token; set {TOKEN_FILE_VARIABLE}"
        )));
    }
    if rest.contains(['/', '?', '#', ':']) {
        return Err(usage(format!(
            "devtunnel://{}: the form is devtunnel://NAME or devtunnel://ID.CLUSTER; the token is a setting ({TOKEN_FILE_VARIABLE}), not a part of the spec",
            rest.split(['/', '?', '#', ':']).next().unwrap_or_default()
        )));
    }
    let locator =
        Locator::parse(rest).map_err(|e| usage(format!("devtunnel://{rest}: {}", e.chain())))?;
    Ok(Spec { locator })
}

/// The credential of the caller and the base of the management API.
#[derive(Clone)]
pub(super) struct Account {
    credential: Credential,
    management: Option<Base>,
}

impl Account {
    #[cfg(test)]
    pub(super) fn new(token: &str, management: Option<&str>) -> Account {
        let token = UserToken::new(Scheme::Github, token).unwrap();
        Account::with(token.into(), management)
    }

    #[cfg(test)]
    pub(super) fn with(credential: Credential, management: Option<&str>) -> Account {
        Account {
            credential,
            management: management.map(|m| Base::parse(m).unwrap()),
        }
    }

    /// The form of the credential, with no byte of a token.
    pub(super) fn form(&self) -> &'static str {
        self.credential.form()
    }
}

/// The form of the credential alone: the token never leaves the value as
/// text.
impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Account")
            .field("credential", &self.credential.form())
            .field("management", &self.management)
            .finish()
    }
}

/// The text of a secret from the file variable `file_name` (its first
/// line) or the variable `name`; `None` when neither is set. A fault names
/// the variables and never holds the text.
fn secret(
    set: &dyn Fn(&str) -> Option<String>,
    file_name: &str,
    name: &str,
) -> Result<Option<String>, Failure> {
    match (set(file_name), set(name)) {
        (Some(_), Some(_)) => Err(usage(format!(
            "set one of {file_name} and {name}, not both"
        ))),
        (Some(file), None) => {
            let text = std::fs::read_to_string(&file)
                .map_err(|e| usage(format!("{file_name}: cannot read {file}: {e}")))?;
            let line = text.lines().next().unwrap_or_default();
            Ok(Some(line.trim().to_string()))
        }
        (None, Some(token)) => Ok(Some(token)),
        (None, None) => Ok(None),
    }
}

/// Read the account with `get` (the variables of the environment). A fault
/// names the variables and never holds a value.
pub(super) fn account(get: &dyn Fn(&str) -> Option<String>) -> Result<Account, Failure> {
    let set = |name: &str| get(name).filter(|v| !v.is_empty());
    let user = secret(&set, TOKEN_FILE_VARIABLE, TOKEN_VARIABLE)?;
    let access = secret(&set, ACCESS_FILE_VARIABLE, ACCESS_VARIABLE)?;
    let anonymous = match set(ANONYMOUS_VARIABLE).as_deref() {
        None | Some("0") => false,
        Some("1") => true,
        Some(_) => return Err(usage(format!("{ANONYMOUS_VARIABLE}: use 1 or 0"))),
    };
    let forms = [
        user.is_some().then_some(TOKEN_FILE_VARIABLE),
        access.is_some().then_some(ACCESS_FILE_VARIABLE),
        anonymous.then_some(ANONYMOUS_VARIABLE),
    ];
    let given: Vec<&str> = forms.into_iter().flatten().collect();
    if given.len() > 1 {
        return Err(usage(format!(
            "Dev Tunnels takes one credential: a user token ({TOKEN_FILE_VARIABLE} or {TOKEN_VARIABLE}), a tunnel access token ({ACCESS_FILE_VARIABLE} or {ACCESS_VARIABLE}) or {ANONYMOUS_VARIABLE}=1; more than one is set"
        )));
    }
    if user.is_none() && set(SCHEME_VARIABLE).is_some() {
        return Err(usage(format!(
            "{SCHEME_VARIABLE} is the scheme of a user token; set {TOKEN_FILE_VARIABLE} or {TOKEN_VARIABLE} with it"
        )));
    }
    let credential = match (user, access, anonymous) {
        (Some(text), _, _) => {
            let scheme = match set(SCHEME_VARIABLE) {
                None => Scheme::Github,
                Some(word) => Scheme::parse(&word)
                    .ok_or_else(|| usage(format!("{SCHEME_VARIABLE}: use github, aad or bearer")))?,
            };
            // The crate names the rule and never the text.
            let token = UserToken::new(scheme, text).map_err(|e| {
                usage(format!(
                    "{TOKEN_FILE_VARIABLE} or {TOKEN_VARIABLE}: {}",
                    e.chain()
                ))
            })?;
            Credential::User(token)
        }
        (None, Some(text), _) => {
            let token = AccessToken::new(text).map_err(|e| {
                usage(format!(
                    "{ACCESS_FILE_VARIABLE} or {ACCESS_VARIABLE}: {}",
                    e.chain()
                ))
            })?;
            Credential::Access(token)
        }
        (None, None, true) => Credential::Anonymous,
        (None, None, false) => {
            return Err(usage(format!(
                "Dev Tunnels needs a management token: set {TOKEN_FILE_VARIABLE} (a file with one line) or {TOKEN_VARIABLE}; for a tunnel access token, {ACCESS_FILE_VARIABLE} or {ACCESS_VARIABLE}; for a tunnel with anonymous access, {ANONYMOUS_VARIABLE}=1"
            )))
        }
    };
    let management = match set(MANAGEMENT_VARIABLE) {
        None => None,
        Some(base) => Some(
            Base::parse(&base)
                .map_err(|e| usage(format!("{MANAGEMENT_VARIABLE}: {}", e.chain())))?,
        ),
    };
    Ok(Account {
        credential,
        management,
    })
}

/// True for the local address of the tunnel host.
fn local_host(target: &Target) -> bool {
    match target {
        Target::Addr(addr) => addr.ip().is_loopback(),
        Target::Name { host, .. } => host.eq_ignore_ascii_case("localhost"),
    }
}

/// The dialer of the network: one channel for each stream.
pub(super) struct DevtunnelDialer {
    label: String,
    relay: Relay,
}

impl Dial for DevtunnelDialer {
    fn label(&self) -> String {
        self.label.clone()
    }

    fn dial<'a>(
        &'a self,
        target: &'a Target,
        deadline: Instant,
    ) -> BoxFuture<'a, Result<Connected, Failure>> {
        Box::pin(async move {
            // The crate opens a channel to the local address of the host
            // (the fact `connect-host`): no other host is carried.
            if !local_host(target) {
                return Err(Failure::new(
                    Stage::Connect,
                    Class::Unsupported,
                    format!(
                        "{}: the host of the target is the local address of the tunnel host (127.0.0.1 or localhost), not {}",
                        self.label,
                        target.host()
                    ),
                ));
            }
            let open = self.relay.open(target.port());
            let stream = tokio::time::timeout_at(deadline, open)
                .await
                .map_err(|_| {
                    Failure::new(
                        Stage::Relay,
                        Class::Timeout,
                        format!(
                            "{}: no stream to port {} before the deadline",
                            self.label,
                            target.port()
                        ),
                    )
                })??;
            Ok(Connected::new(Box::new(stream)))
        })
    }
}

/// Bring the network up: the connect information, the relay WebSocket and
/// the SSH session, once. A fault names the step.
pub(super) async fn up(
    spec: &Spec,
    account: Account,
    below: Option<&Arc<dyn Dial>>,
) -> Result<Link, Failure> {
    let label = spec.label();
    let form = account.form();
    let dial = below.cloned().unwrap_or_else(crate::dialers::process);
    let agent = format!("podnet/{}", env!("CARGO_PKG_VERSION"));
    let tls = crate::trust::context().clone();
    let way = Way {
        http: pod_proxy::Client::new(tls.clone(), dial.clone()).with_user_agent(agent.clone()),
        ws: pod_ws::Way::new(tls, dial, agent),
        trust: crate::trust::ws_trust(),
    };
    let mut settings = Settings::default();
    if let Some(base) = account.management {
        settings.management = base;
    }
    settings.ssh_id = format!("SSH-2.0-podnet_{}", env!("CARGO_PKG_VERSION"));
    // The lines hold no token (the crate keeps them out).
    let report = pod_core::Report {
        events: Arc::new(|event: pod_core::Event| eprintln!("podnet: {event}")),
        ..Default::default()
    };
    let relay = Relay::connect(
        way,
        settings,
        spec.locator.clone(),
        account.credential,
        report,
    )
    .await?;
    // The status holds no token: the form of the credential alone.
    let status = serde_json::json!({
        "scheme": "devtunnel",
        "spec": label,
        "tunnel": relay.tunnel().shown(),
        "credential": form,
        "sessions": relay.sessions(),
    });
    let dialer = Arc::new(DevtunnelDialer { label, relay });
    Ok(Link::new(Some(NetWay::destination(dialer))).with_status(status))
}

/// The check of the network: the management API host (the fact of
/// `pod-services`, or the variable), with TLS.
pub(super) fn checks() -> Vec<Check> {
    let base = std::env::var(MANAGEMENT_VARIABLE)
        .ok()
        .and_then(|text| Base::parse(&text).ok())
        .unwrap_or_else(|| Settings::default().management);
    vec![Check {
        name: "devtunnel".into(),
        target: Target::new(&base.host, base.port),
        tls: base.tls,
    }]
}
