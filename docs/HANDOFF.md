# Handoff

The state at 2026-11-16 16:45 (UTC). Read this file first, then `CLAUDE.md`, `TODO/PROGRESS.md`, `docs/maintenance.md` and
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
* The record is `TODO/`. `cargo todo check` agrees: 404 entries, 16 open, 11 partial, 36 blocked, 341 done.

## Unfinished work (not on `main`)

None. The head is the commit of this handoff.

## CI

* A push to `main` starts `ci`, `deny`, `secrets`, `live` and `issue-sync`. The session starts and cancels runs with the
  MCP tool `actions_run_trigger` (`docs/maintenance.md`, Runs from the session). Push in batches; cancel the `ci` of a
  head that a newer push replaces. `live` has one concurrency group: a newer push replaces a pending `live`, so let a
  running `live` end before the next push when its result matters.
* Read each run when the watcher reports its end (`CLAUDE.md` step 1).
* On `c92438d`: `live` 43902951342 passed (T-397 holds); `gate` 43902956923 failed in `no-proc link-control`
  (T-401, fixed); `ci` 43902951966 failed in the job `features` (clippy with `--features full`, fixed: T-396
  Correction). The head after this handoff has the version `0.4.0-beta.14` and its notes (T-400). Read its `ci` and
  `live` first; then dispatch the dry run of `release.yml` (it runs the gate).
* `record-issues` (each day) no longer fails on #475: the owner removed the entry 305 (T-396).

## The work of 2026-11-16, third session (all on `main`)

* The owner's rulings (`docs/decisions.md`, 2026-11-16): the entry 305 is removed (#475 closed as not planned; the empty feature
  `portable` is gone, T-396); `--seal` stays opt-in (Q1); each triaged issue of Nemo-010 gets the label `confirmed`
  (172 issues labelled; the deferred #484 to #501 are not touched).
* T-392 (#599 closed): the vnet shim answers `NETLINK_ROUTE`, `getifaddrs` and `if_nameindex` with `lo` and `vnet0`.
* T-384: `pod --backend netns-kernel` binds fixed ports (61001 to 61003); it passes in `getsockname-denied`.
* T-397: the two WARP steps of the quick tunnel in `live.yml` share one device file (HTTP 429 of live 43894127680).
* T-358 step 4 is done: `src/cli/scenarios.txt` (written by `scripts/gate-scenarios.py`) gives the scenarios of each
  command; the help of a command with no scenario starts with "Experimental:", and `release.yml` lists them in the
  notes. T-398 gives scenarios to the experimental commands: 3 stay (`net tailscale`, `ssh-relay`, `warp
  license`).

## Next steps

0. Read the runs of "CI" above. Fix each failure first (a new lane case that fails on the GitHub runner is a defect of
   the case or of podnet).
1. T-400: cut `v0.4.0-beta.14` when `ci` and `live` pass on the head: the dry run of `release.yml` (with the gate),
   then `publish=v0.4.0-beta.14`, then `release-proofs.yml` (`docs/maintenance.md`, Release).
2. T-402 is done (the reactor watches the WARP session); its COORD issue to podcrates names the frozen files.
3. T-398: scenarios for the 3 commands left (see its Correction).
4. T-358, what remains: the row `way-out` (a quick tunnel in `podlab run --way-out proxy-only`) is a default row now; the lane rows with `podlab run --way-out PROFILE` and `--serve PORT`, WebSocket through the
   proxy, `ipv6-only` on a host with IPv6, the `icmp` rows; the relay and the tunnel as podssh and podrunner call them.
5. At each tag of podcrates: `sh scripts/agent/podcrates.sh`, and take the crates whose hand-off is at the tag (T-137).
   Write the take into COORD-096 (#597).
6. Go on with the work order of `TODO/PROGRESS.md`. Most open entries wait for crates of podcrates. The disk allowance
   holds about one debug build: build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
   After a change of a document, run `cargo test --bin podnet write_the_llms_files -- --ignored`. Issues #484 to #501
   are deferred by the owner: leave them open and unlabelled.

Run the lane here: `sh scripts/get-podlab.sh v0.63.0 .work/podlab-v0.63.0`; build podnet; copy the binary and the
podlab asset to a directory that uid 65534 can read (for example `/var/tmp/lane/`, not in `/tmp` or HOME); then as
root: `setpriv --reuid=65534 --regid=65534 --clear-groups env -i PATH=/usr/bin:/bin HOME=OTHERDIR LANE_DIR=DIR/out sh
scripts/lab-lane.sh PODLAB DIR/podnet [PROFILE...]`.

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
| #592 (COORD-095) | T-358 (the release gate; step 4 is done); T-380 and T-381 are done. |
| #587, #589 | podcrates closes #185 and #188. |
| #413, #425, #437, #439, #440, #448, #460 | Older proposals and asks; see `TODO/issues.md`. |

## Waiting on the owner

None. The owner ruled on 2026-11-16: the entry 305 is removed (#475 closed, T-396), `--seal` stays opt-in (Q1), and each
triaged issue of Nemo-010 gets the label `confirmed` (not the deferred #484 to #501).
