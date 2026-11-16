# Handoff

The state at 2026-11-16 14:15 (UTC). Read this file first, then `CLAUDE.md`, `TODO/PROGRESS.md`, `docs/maintenance.md` and
`docs/decisions.md`. Remove this file when the lead session has no unfinished step.

## The goal

The owner (2026-11-13, `docs/decisions.md`, The complete beta): the beta test round is cancelled. Close each GitHub
issue except the deferred ones, then cut a final complete beta. The takes of PodUtils/podcrates follow its plan
(PodUtils/podcrates#133): the next tags bring `pod-cloudflare`, `pod-egress`, `pod-derp`, `pod-ts` and `pod-acme`.

## Where the code is

* All finished work is on `main` of `PodUtils/podnet`. The toolchain is Rust 1.99.1 (`rustup default 1.99.1`; add
  `rustfmt`, `clippy` and the target `x86_64-unknown-linux-musl` in a new container).
* podnet takes the crates of `PodUtils/podcrates` by the git tag `v0.63.0` (T-394), with `pod-warp`: `src/warp/` is glue
  over the crate and takes fixes only (each fix goes to podcrates as a COORD issue).
* The newest release is `v0.4.0-beta.13` (T-355, 0a0f3e5).
* The record is `TODO/`. `cargo todo check` agrees: 394 entries, 13 open, 10 partial, 37 blocked, 334 done.

## Unfinished work (not on `main`)

None. No subagent runs.

## CI

* A push to `main` of the session starts `ci`, `deny`, `secrets`, `live` and `issue-sync` (measured 2026-11-16). The
  session can dispatch a workflow and cancel a run with the MCP tool `actions_run_trigger`. No `[gh action] requested`
  issue is needed (`docs/maintenance.md`, Runs from the session). Push in batches: `ci` has about 12 jobs, and the
  runners are shared; cancel the runs of a head that a newer push replaces.
* Each workflow passed on `01487d8`: ci 43820922609, deny 43820922608, secrets 43820922620, live 43820922672, gate
  43820928202 (the first run of `gate.yml`). GitHub #598 is closed in the record.
* Each workflow passed on `89a6e1e` (podcrates v0.61.0 and its installer core): ci 43831877376, deny 43831877606,
  secrets 43831877485, live 43831877467, gate 43831896685.
* The dry run of `release.yml` on `5fbb2e3` passed with its new job `gate / lane` before `publish` (run 43839899756).
  On `81d2e73` (run 43849173659) the body of the release has the section "The release gate" (226 pass, 0 fail, 24 n.a.).
* Each workflow passed on `b6c4ef7` (podcrates v0.62.0): ci 43854114725, deny 43854114730, secrets 43854114663, live
  43854114684 and the daily live 43863047917.
* Each workflow passed on `62816a8` (v0.63.0): ci 43867293485, live 43867293410, deny 43867293283, secrets 43867293551.
* T-394 (podcrates v0.63.0): the installer core checks the signature for the copy of the tree too; podnet's own
  `verify_signature` is removed (one cosign check for each run).
* GitHub #599 (Nemo-010, outside the team): verified and labelled `confirmed`. T-390 and T-391 are done (`c3616be`,
  `tests/shim_daemon.rs`); T-392 (the netlink answer of the vnet shim, P2) is open.
* Read each run when the watcher reports its end (`CLAUDE.md` step 1). Do not wait for a comment.

## The work of 2026-11-16, second session (all on `main`)

* T-360 Correction (`c24e0cf`): the two install-test cases skip on a host with no loopback.
* T-387 done (`ad8abd4`): podcrates v0.60.0. The manual now gives clap's `requires` rules; `llms-full.txt` is written
  again. COORD-096 (#597) has the take.
* T-388 and T-389 done (`45a683a`): podcrates v0.61.0 and its installer core (`core.sh` and `core-net.sh`). The copy of
  a release checks the cosign signature in the core; the copy of the tree keeps its own check. `PODNET_COSIGN`.
* T-358 step 1 (`f32483a`): the lane runs in the network labs of podlab v0.60.0. The old row `proxy-only` is now
  `allow-connect`. The full lane: 223 pass, 22 n.a., 0 fail (as uid 65534; HOME away from the binary).

## Next steps

0. The release gate (T-358) comes before the next beta (`docs/decisions.md`, 2026-11-15). Run the lane:

   1. `sh scripts/get-podlab.sh v0.61.0 .work/podlab-v0.61.0` (the release asset of podcrates, SHA-256 checked).
   2. Build the release musl binary of podnet. Copy it to a directory that uid 65534 can read and write, not in `/tmp`
      and not in HOME, for example `/var/tmp/lane/` with mode 1777.
   3. As root: `setpriv --reuid=65534 --regid=65534 --clear-groups env -i PATH=/usr/bin:/bin HOME=OTHERDIR
      LANE_DIR=DIR/out sh scripts/lab-lane.sh .work/podlab-v0.61.0 DIR/podnet [PROFILE...]`.

   `gate.yml` runs the same lane on a GitHub runner (by hand, and as the job `gate` of `release.yml`: `publish` needs
   it). podlab v0.61.0 has `podlab run --way-out PROFILE` (a lab with a way out to the real network through its
   stand-in proxy): use it for a live row. Left in T-358: the notes of a beta list each scenario and its result (step 4); a row with WebSocket through the proxy (`ws-upgrade-refused`: the relay and the tunnel);
   `ipv6-only` on a host with IPv6; the `icmp` rows; the relay and the tunnel as podssh and podrunner call them; T-384.
   No beta before the gate.
1. At each tag of podcrates: `sh scripts/agent/podcrates.sh`, and take the crates whose hand-off is at the tag (T-137).
2. Go on with the work order of `TODO/PROGRESS.md`. The disk allowance holds about one debug build: run one build at a
   time, with `CARGO_INCREMENTAL=0`; `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` keeps `target/` small. Remove
   `target/debug` when a write fails with "no space". Close each issue whose entries are done through the record.
   Issues #484 to #501 are deferred by the owner: leave them open.
3. When each non-deferred issue is closed and `live` is green: cut the final complete beta (the steps of T-331).

## Known flakes under load

`quick_tunnel_standin_serves_a_visitor_over_http2`, `service_serve_ingress::the_service_ends_with_the_serve_process`,
`shim_direct::a_perl_accept_loop_serves_each_connection` and `ws_relay::...::a_demoted_relay_host_recovers_on_success`
`autostart::sigterm_is_a_clean_stop`, `doctor_ssh_relay_row_dials_through_the_proxy` and
`guest_wire_unix_socket_serves_a_guest_and_the_next` (keep the default `TMPDIR`) fail at times in a full run and pass alone. Run a failed test alone three times before a push.

## Open COORD issues

| Issue | Waits for |
| --- | --- |
| #522, #526, #532 (takes of v0.37.0, v0.39.0 and v0.40.0) | podcrates records the take. |
| #560 (COORD-081, the take of v0.45.0) | The answer of podcrates on the ws listener (a refused pin: `Refused` or no peer). |
| #555 (COORD-079) | The fuzz crash of `pod-proxy` is fixed in v0.45.0 (T-349); the crash of `pod-cloudflare` waits for its take (their entry 029). |
| #597 (COORD-096, the takes of v0.58.0 to v0.62.0) | podcrates reads it; close it when podcrates closes #200, #204, #206, #209 and #210. |
| #592 (COORD-095) | T-358 (the release gate); T-380 and T-381 are done. |
| #587, #589 | podcrates closes #185 and #188. |
| #413, #425, #437, #439, #440, #448, #460 | Older proposals and asks; see `TODO/issues.md`. |

## Waiting on the owner

* Q2 in `docs/decisions.md`: remove T-305 by ruling, so that #475 stays closed.
* The ~76 issues of Nemo-010 (author_association NONE): add the label `confirmed`, or add the author to the team.
  `issue-sync` does not change them. On 2026-11-16 the session labelled `confirmed` #467, #468, #470 and #473 (each
  entry done; the record closes them) because `record-issues` failed on them. `record-issues` still fails on #475 until
  Q2 is ruled: GitHub has it closed, and the record keeps it open for T-305.
* The relay-tls-proxy bundle of the owner's local agent was not reviewed: the review stopped at a safety block of the
  model. The owner can ask a narrower question.
