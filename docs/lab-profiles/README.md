# Network profiles of podnet for pod-lab

This directory is the hand-off of podnet for the network classes of
`pod-lab` (PodUtils/podcrates, their entry T-716; podnet T-358 step 1). Each
file gives one profile:

* the data of the profile in the form of `crates/pod-lab/src/table/` of
  podcrates at the tag `v0.56.0`, commit `93bf133a`
  (`crates/pod-lab/src/table.rs`, `crates/pod-lab/src/table/parts.rs`,
  `crates/pod-lab/src/profile.rs`);
* for each part: what observes it, and how a lab can hold it;
* what podnet does in the class, with the files of its code;
* the commands of podnet in the class, and their expected result.

| File | Profile | Parts |
| --- | --- | --- |
| [proxy-tls-reterminate.md](proxy-tls-reterminate.md) | `proxy-tls-reterminate` | `tls-reterminated`, `ssh-port-silent` |
| [proxy-per-name.md](proxy-per-name.md) | `proxy-per-name` | `answer-per-name` |
| [sni-allowlist.md](sni-allowlist.md) | `sni-allowlist` | `sni-reset` |
| [no-proxy-no-dns.md](no-proxy-no-dns.md) | `no-proxy-no-dns` | `no-resolver`, `literal-reaches` |
| [ws-upgrade-refused.md](ws-upgrade-refused.md) | `ws-upgrade-refused` | `upgrade-refused` |
| [ipv6-only.md](ipv6-only.md) | `ipv6-only` | `no-ipv4-route`, `ipv6-reaches` |

## The form

The data uses the types of `profile.rs` at `93bf133a` only: `Part`,
`Observe::Nothing { needs, entry }` and `Hold::Elsewhere(NETWORK)` through
the helpers `net(...)` and `needs(...)` of `table/parts.rs`. The parts and
their names are the ones of the table at `v0.56.0`. The `needs` texts give
the exact probe that a network lab runs. A new `Hold` variant for a network
lab is the work of podcrates: each file gives the mechanism in words only.

## The ways to hold a part

| Way | Meaning |
| --- | --- |
| no root | A process of the caller: a stand-in proxy or server on a unix socket or a loopback port, Landlock, seccomp. |
| user namespace | `unshare(CLONE_NEWUSER \| CLONE_NEWNET \| CLONE_NEWNS)` as the caller: the namespace root sets `lo` up, adds addresses and routes, binds files. A stand-in outside the namespace is reached through a unix socket. |
| root only | A change of the host: `nftables` of the host namespace, a route of the host, NAT66. |
| a stand-in proxy process | A small HTTP `CONNECT` proxy of the lab with a table of answers for each name and port. It can be a part of each way above. |

## The marks

* **measured**: a run of podnet on a real host gave the fact
  (`docs/target-environment.md`, with its date).
* **inferred**: the code or a test of podnet gives the fact; no real host
  was measured.
* **assumed**: neither. The fact is the form of the class that podssh or
  podnet named, and a real network can differ. A lab that holds an assumed
  part tests podnet against the assumption only.

No file gives a fact of a real network that was not measured: each such
part is marked assumed.
