# Handoff

The state at 2026-11-16 15:15 (UTC). Read this file first, then `CLAUDE.md`, `TODO/PROGRESS.md`, `docs/maintenance.md` and
`docs/decisions.md`. Remove this file when the lead session has no unfinished step.

## The goal

The owner (2026-11-13, `docs/decisions.md`, The complete beta): the beta test round is cancelled. Close each GitHub
issue except the deferred ones, then cut a final complete beta. The takes of PodUtils/podcrates follow its plan
(PodUtils/podcrates#133): the next tags bring `pod-cloudflare`, `pod-egress`, `pod-derp`, `pod-ts` and `pod-acme`.

## Where the code is

* All finished work is on `main` of `PodUtils/podnet`. The toolchain is Rust 1.99.1 (`rustup default 1.99.1`; add
  `rustfmt`, `clippy` and the target `x86_64-unknown-linux-musl` in a new container).
* podnet takes the crates of `PodUtils/podcrates` by the git tag `v0.63.0` (T-394), with `pod-warp`: `src/warp/` is glue
  over the crate and takes fixes only (each fix goes to podcrates as a COORD issue). The installer core in
  `scripts/install/` is that of `v0.63.0` (`core.sh`, `core-net.sh`, `pins.py`; `scripts/install/SOURCES`).
* The newest release is `v0.4.0-beta.13` (T-355, 0a0f3e5).
* The record is `TODO/`. `cargo todo check` agrees: 395 entries, 13 open, 10 partial, 37 blocked, 335 done.

## Unfinished work (not on `main`)

None. No subagent runs, no scheduled task. The head is the commit of this handoff (after `3f203cc`).

## CI

* A push to `main` of the session starts `ci`, `deny`, `secrets`, `live` and `issue-sync`. The session can start a
  workflow and cancel a run with the MCP tool `actions_run_trigger` (`docs/maintenance.md`, Runs from the session). No
  `[gh action] requested` issue is needed. Push in batches: `ci` has about 12 jobs, and the runners are shared. Cancel
  the runs of a head that a newer push replaces.
* The last full set: ci 43881449004, deny 43881449196, secrets 43881449226 and issue-sync 43881449821 passed on
  `7954152`; live 43881448907 failed there on a race of its check (T-395, fixed in `3f203cc`); on `3f203cc` deny
  43888061649, secrets 43888061611, issue-sync 43888063034 and live 43888061947 passed, and ci 43888061562 was running at
  this handoff. Read its result first.
* `gate.yml` (the release gate) passed on 01487d8, 89a6e1e, and as the job `gate / lane` of the release dry runs
  43839899756 (5fbb2e3) and 43849173659 (81d2e73). The second dry run writes the section "The release gate" in the body
  (226 pass, 0 fail, 24 n.a.).
* `record-issues` (each day) fails on #475 alone until Q2 is ruled (see "Waiting on the owner").
* Read each run when the watcher reports its end (`CLAUDE.md` step 1). Do not wait for a comment.

## The work of 2026-11-16, second session (all on `main`)

* #598 closed: T-360 Correction (`c24e0cf`), the two install-test cases skip on a host with no loopback.
* Takes of podcrates: T-387 v0.60.0 (`ad8abd4`, the manual gives clap's `requires`), T-388 v0.61.0 (`45a683a`), T-393
  v0.62.0 (`e8fb00a`), T-394 v0.63.0 (`62816a8`). COORD-096 (#597) holds each take.
* The installer: T-389 took the core of v0.61.0 (the core checks the cosign signature, `PODNET_COSIGN`); podnet's finding
  (a copy of the tree from a file stopped on a host with cosign) is their entry 773, fixed in v0.63.0; T-394 removed
  podnet's own `verify_signature`. `scripts/install-test.sh`: 12 cases.
* T-358 (the release gate): the lane runs in the network labs (`f32483a`; the old row `proxy-only` is `allow-connect`);
  `gate.yml` runs it on a GitHub runner as a user that is not root (`01487d8`); `release.yml` calls it as the job `gate`,
  which `publish` needs (`5fbb2e3`); `scripts/gate-notes.sh` writes each row of the lane in the notes of a release
  (`81d2e73`).
* GitHub #599 (Nemo-010, outside the team): verified and labelled `confirmed`; T-390 (fcntl lock commands) and T-391 (a
  wildcard bind takes the local address) are done in `c3616be` (`tests/shim_daemon.rs`); T-392 (netlink) is open.
* #467, #468, #470 and #473 labelled `confirmed`, so that `issue-sync` closed them (`7954152`).
* T-395 (`3f203cc`): the check of `agent up` in `live.yml` retries a name that the resolver of the runner does not know
  yet.

## Next steps

0. The release gate (T-358) comes before the next beta (`docs/decisions.md`, 2026-11-15). Left in T-358:
   * Step 4: a feature with no passing scenario is marked experimental in its help and in the notes.
   * Rows of the lane with `podlab run --way-out PROFILE` (a live row through the stand-in proxy; podssh found that the
     roots of the host in such a lab are their entry 792) and `podlab run --serve PORT` (the relay of podnet in a
     network lab; podssh's lane passed this way in five labs, PodUtils/podcrates#211).
   * A row with WebSocket through the proxy (`ws-upgrade-refused`); `ipv6-only` on a host with IPv6 (it holds on the
     GitHub runner); the `icmp` rows; the relay and the tunnel as podssh and podrunner call them; T-384.

   Run the lane here:

   1. `sh scripts/get-podlab.sh v0.63.0 .work/podlab-v0.63.0` (the release asset of podcrates, SHA-256 checked).
   2. Build the release musl binary of podnet. Copy it to a directory that uid 65534 can read and write, not in `/tmp`
      and not in HOME, for example `/var/tmp/lane/` with mode 1777.
   3. As root: `setpriv --reuid=65534 --regid=65534 --clear-groups env -i PATH=/usr/bin:/bin HOME=OTHERDIR
      LANE_DIR=DIR/out sh scripts/lab-lane.sh .work/podlab-v0.63.0 DIR/podnet [PROFILE...]`.
1. T-392 (P2): the vnet shim answers `AF_NETLINK` route dumps with the virtual interfaces (GitHub #599 has the design).
   Close #599 through the record when T-392 is done.
2. At each tag of podcrates: `sh scripts/agent/podcrates.sh`, and take the crates whose hand-off is at the tag (T-137).
   Write the take into COORD-096 (#597): amend its body with a line and a `state:` line.
3. Go on with the work order of `TODO/PROGRESS.md`. The disk allowance holds about one debug build: run one build at a
   time, with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`. Remove `target/debug` when a
   write fails with "no space". After a change of a document, run `cargo test --bin podnet write_the_llms_files --
   --ignored` (the llms files hold the documents). Issues #484 to #501 are deferred by the owner: leave them open.
4. When each non-deferred issue is closed and `live` is green: cut the final complete beta (the steps of T-331).

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
| #597 (COORD-096, the takes of v0.58.0 to v0.63.0) | podcrates reads it; close it when podcrates closes #200, #204, #206, #209, #210 and #211. |
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
