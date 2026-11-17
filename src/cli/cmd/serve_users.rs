//! `podnet serve --add-user USER:ROLE --users FILE` (#633): add a user to
//! the users file of the file server. The password comes from the first
//! line of stdin. A new file gets mode 0600; an old file must be private.

use std::path::Path;

use anyhow::{Context, Result};
use podnet::exit::{Fault, ResultExt};

/// Read the password of `flag` from the first line of stdin.
pub(super) fn read_password(flag: &str) -> Result<String> {
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("reading the password from stdin")?;
    let password = line.trim_end_matches(['\r', '\n']);
    if password.is_empty() {
        return Err(podnet::exit::fault(
            Fault::Usage,
            anyhow::anyhow!("{flag}: give the password on the first line of stdin"),
        ));
    }
    Ok(password.to_string())
}

/// Add the user of `spec` (USER:ROLE) to the file `path`, or replace the
/// line of that user with `replace`.
pub(super) fn add_user(spec: &str, path: &Path, replace: bool) -> Result<()> {
    let (name, role) = parse_spec(spec).fault(Fault::Usage)?;
    let current = pod_core::files::read_own(path, pod_core::files::Others::NoAccess)
        .map_err(anyhow::Error::msg)
        .with_context(|| path.display().to_string())
        .fault(Fault::Config)?
        .unwrap_or_default();
    // The check of a duplicate comes before the password: a refused add
    // reads nothing.
    let line = |hash: &str| format!("{name}:{role}:{hash}");
    with_user(&current, name, "", replace)
        .with_context(|| path.display().to_string())
        .fault(Fault::Usage)?;
    let password = read_password("--add-user")?;
    let next = with_user(
        &current,
        name,
        &line(&podnet::files::hash_password(&password)),
        replace,
    )?;
    pod_core::files::replace_private(path, next.as_bytes(), 0o600, false)
        .with_context(|| format!("writing {}", path.display()))?;
    println!("podnet: the user {name} ({role}) is in {}", path.display());
    Ok(())
}

/// The name and the role of USER:ROLE.
fn parse_spec(spec: &str) -> Result<(&str, &str)> {
    let (name, role) = spec
        .split_once(':')
        .context("--add-user: give USER:ROLE (ROLE is read or write)")?;
    if name.is_empty() || name.len() > 255 || name.chars().any(|c| c == ':' || c.is_control()) {
        anyhow::bail!("--add-user: a user has 1 to 255 bytes, with no colon");
    }
    if !matches!(role, "read" | "write") {
        anyhow::bail!("--add-user: the role {role:?} is not read or write");
    }
    Ok((name, role))
}

/// `text` with the line `line` for the user `name`: in the place of the old
/// line of `name` with `replace`, else at the end. An old line of `name`
/// with no `replace` is an error.
fn with_user(text: &str, name: &str, line: &str, replace: bool) -> Result<String> {
    let of_name = |l: &str| {
        let l = l.trim_end_matches('\r');
        !l.trim_start().starts_with('#') && l.split(':').next() == Some(name)
    };
    if text.lines().any(of_name) {
        if !replace {
            anyhow::bail!("the user {name} is in the file: give --replace-user to replace it");
        }
        let mut out: String = text
            .lines()
            .map(|l| if of_name(l) { line } else { l })
            .collect::<Vec<_>>()
            .join("\n");
        out.push('\n');
        return Ok(out);
    }
    let mut out = text.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(line);
    out.push('\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_user_goes_to_the_end_or_replaces_its_line() {
        let text = "# users\nbob:read:h1\n";
        let added = with_user(text, "alice", "alice:write:h2", false).unwrap();
        assert_eq!(added, "# users\nbob:read:h1\nalice:write:h2\n");
        assert!(with_user(&added, "bob", "bob:write:h3", false).is_err());
        let replaced = with_user(&added, "bob", "bob:write:h3", true).unwrap();
        assert_eq!(replaced, "# users\nbob:write:h3\nalice:write:h2\n");
        assert_eq!(with_user("", "a", "a:read:h", false).unwrap(), "a:read:h\n");
        assert_eq!(
            with_user("x:read:h", "a", "a:read:h", false).unwrap(),
            "x:read:h\na:read:h\n"
        );
    }

    #[test]
    fn the_spec_is_user_and_role() {
        assert_eq!(parse_spec("alice:read").unwrap(), ("alice", "read"));
        for bad in ["alice", ":read", "alice:admin", "a:b:read"] {
            assert!(parse_spec(bad).is_err(), "{bad}");
        }
    }
}
