//! Ingress: which local service answers which public hostname/path. The
//! rules are pod-cloudflare's; the services of podnet (the file server,
//! git, the shares) are custom services of a rule.

use std::sync::Arc;

pub use pod_cloudflare::ingress::*;

use crate::share::ShareConfig;

/// A service of podnet in a rule ([`Service::Custom`]).
#[derive(Clone, Debug)]
pub enum Podnet {
    /// The file server: listings, uploads, users.
    Files(crate::files::FilesConfig),
    /// A git repository over smart HTTP.
    Git(crate::git::http::HttpOptions),
    /// A share: a file, a directory, a drop box or a proxy behind a PIN.
    Share(ShareConfig),
}

impl Podnet {
    /// The text of the service in a log line and in an error.
    pub fn describe(&self) -> String {
        match self {
            Podnet::Files(files) => format!(
                "{}:{}",
                if files.spa { "spa" } else { "static" },
                files.root.display()
            ),
            Podnet::Git(git) => format!("git:{}", git.root.display()),
            Podnet::Share(config) => config.describe(),
        }
    }

    /// The rule service of this service of podnet.
    pub fn service(self) -> Service {
        Service::Custom(Custom {
            name: self.describe(),
            value: Arc::new(self),
        })
    }
}

/// The file server `files` as a rule service.
pub fn files(files: crate::files::FilesConfig) -> Service {
    Podnet::Files(files).service()
}

/// The git repository of `git` as a rule service.
pub fn git(git: crate::git::http::HttpOptions) -> Service {
    Podnet::Git(git).service()
}

/// The share `config` as a rule service.
pub fn share(config: ShareConfig) -> Service {
    Podnet::Share(config).service()
}

/// The service of podnet in `service`, when it is one.
pub fn podnet(service: &Service) -> Option<&Podnet> {
    match service {
        Service::Custom(custom) => custom.value.downcast_ref::<Podnet>(),
        _ => None,
    }
}
