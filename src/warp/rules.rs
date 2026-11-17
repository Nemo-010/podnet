//! The rules of `podnet warp proxy` (issues 132 and 136): `pod-warp`'s
//! table, as one value of the process, with podnet's reload of the rules
//! file.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::Result;

pub use pod_warp::rules::*;

/// The rules of the proxies of this process.
pub fn rules() -> &'static Arc<Rules> {
    static RULES: std::sync::LazyLock<Arc<Rules>> =
        std::sync::LazyLock::new(|| Arc::new(Rules::new(Table::default())));
    &RULES
}

/// The rules of the flags, then the rules of `file`. A shadowed rule gives
/// a warning; a shadowed `reject` is a usage error unless `allow` (T-242,
/// the lint of `podnet pod -r` and of the router).
pub fn build(specs: &[String], file: Option<&Path>, default: Action, allow: bool) -> Result<Table> {
    let (table, warnings) =
        Table::build(specs, file, default, &lint, allow).map_err(super::failed)?;
    for warning in warnings {
        tracing::warn!("{warning}");
    }
    Ok(table)
}

/// podnet's lint of shadowed rules over the rules of `pod-warp`. A matcher
/// that podnet does not read is not compared.
fn lint(items: &[LintItem<'_>], allow: bool) -> std::result::Result<Vec<String>, String> {
    use podnet_netns::rules::shadow;
    let matchers: Vec<Option<RuleMatch>> = items
        .iter()
        .map(|item| RuleMatch::parse(item.matcher).ok())
        .collect();
    let items: Vec<shadow::LintItem<'_>> = items
        .iter()
        .zip(&matchers)
        .filter_map(|(item, matcher)| {
            Some(shadow::LintItem {
                matcher: matcher.as_ref()?,
                action: shadow::action_key(item.action),
                place: item.place.to_string(),
                spec: item.spec.to_string(),
            })
        })
        .collect();
    shadow::check(&items, allow).map_err(|err| format!("{err:#}"))
}

/// Set the rules of the proxies of this process.
pub fn set(table: Table) {
    rules().set(table);
}

/// The rules of now (no rule, default tunnel, when none were set).
pub fn current() -> Arc<Table> {
    rules().current()
}

/// How often the rules file is checked for a change.
pub const RELOAD_EVERY: Duration = Duration::from_secs(2);

/// Read `file` again when it changes, and use its rules with the rules of
/// the flags. A file with an error is reported, and the rules before stay.
pub fn watch(specs: Vec<String>, file: PathBuf, default: Action, allow: bool) {
    let stamp = |p: &Path| -> Option<(SystemTime, u64)> {
        let meta = std::fs::metadata(p).ok()?;
        Some((meta.modified().ok()?, meta.len()))
    };
    tokio::spawn(async move {
        let mut last = stamp(&file);
        let mut tick = tokio::time::interval(RELOAD_EVERY);
        tick.tick().await;
        loop {
            tick.tick().await;
            let now = stamp(&file);
            if now == last {
                continue;
            }
            last = now;
            match build(&specs, Some(&file), default, allow) {
                Ok(table) => {
                    tracing::info!(file = %file.display(), rules = table.rules.len(), "the rules of the WARP proxy are read again");
                    rules().counters.reloads.fetch_add(1, Relaxed);
                    set(table);
                }
                Err(err) => tracing::warn!(
                    error = %format!("{err:#}"),
                    "the rules file has an error; the rules before stay"
                ),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    // T-242: the lint of `podnet pod -r` and of the router also checks the
    // rules of `warp proxy`.
    #[test]
    fn warp_shadowed_reject_is_an_error() {
        let rules = specs(&[
            "domain:.example.com=tunnel",
            "domain:www.example.com=reject",
        ]);
        let err = build(&rules, None, Action::Tunnel, false).unwrap_err();
        let text = format!("{err:#}");
        assert!(text.contains("-r 2") && text.contains("-r 1"), "{text}");
        assert!(text.contains("--allow-shadowed-rules"), "{text}");
        let err = build(
            &specs(&["cidr:10.0.0.0/8=direct", "cidr:10.1.0.0/16=reject"]),
            None,
            Action::Tunnel,
            false,
        );
        assert!(err.is_err());
    }

    #[test]
    fn warp_shadowed_rules_with_the_flag_or_another_order_load() {
        let rules = specs(&[
            "domain:.example.com=tunnel",
            "domain:www.example.com=reject",
        ]);
        assert_eq!(
            build(&rules, None, Action::Tunnel, true)
                .unwrap()
                .rules
                .len(),
            2
        );
        let reversed = specs(&[
            "domain:www.example.com=reject",
            "domain:.example.com=tunnel",
        ]);
        assert_eq!(
            build(&reversed, None, Action::Tunnel, false)
                .unwrap()
                .rules
                .len(),
            2
        );
        // A shadowed rule of another action than reject loads (a warning).
        let direct = specs(&[
            "domain:.example.com=tunnel",
            "domain:www.example.com=direct",
        ]);
        assert!(build(&direct, None, Action::Tunnel, false).is_ok());
    }

    #[test]
    fn warp_shadowed_reject_in_the_file_names_its_line() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("rules");
        std::fs::write(
            &file,
            "# rules\ncidr:10.0.0.0/8=tunnel\ncidr:10.9.0.0/16=reject\n",
        )
        .unwrap();
        let err = build(&[], Some(&file), Action::Tunnel, false).unwrap_err();
        assert!(format!("{err:#}").contains(":3"), "{err:#}");
    }
}
