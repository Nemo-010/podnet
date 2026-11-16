# Handoff

The state at 2026-11-16 05:50 (UTC). Read this file first, then `CLAUDE.md`, `TODO/PROGRESS.md`, `docs/maintenance.md` and
`docs/decisions.md`. Remove this file when the lead session has no unfinished step.

## The goal

The owner (2026-11-13, `docs/decisions.md`, The complete beta): the beta test round is cancelled. Close each GitHub
issue except the deferred ones, then cut a final complete beta. The takes of PodUtils/podcrates follow its plan
(PodUtils/podcrates#133): the next tags bring `pod-cloudflare`, `pod-egress`, `pod-derp`, `pod-ts` and `pod-acme`.

## Where the code is

* All finished work is on `main` of `PodUtils/podnet`. The toolchain is Rust 1.99.1 (`rustup default 1.99.1`).
* podnet takes the crates of `PodUtils/podcrates` by the git tag `v0.59.0` (T-137), with `pod-warp`: `src/warp/` is glue
  over the crate and takes fixes only (each fix goes to podcrates as a COORD issue).
* The newest release is `v0.4.0-beta.13` (T-355, 0a0f3e5).
* The record is `TODO/`. `cargo todo check` agrees: 386 entries, 12 open, 10 partial, 37 blocked, 327 done.

## Unfinished work (not on `main`)

None. Each change of the session of 2026-11-15/16 is on `main` (head `d440d36`). No subagent runs, no scheduled
heartbeat is left.

## Open fault: CI of `1852fcc` (GitHub #598) — fix this first

#598 asked for ci, deny and a dry run of `release` on `1852fcc`. deny 43796829153 and release 43796832964 passed (the
release copy of the installer is stamped and installs). ci 43796824990 failed in the jobs `restricted-host` and
`no-loopback` (and `all`, which only collects them). The cause is T-360: `tests/install_script/core.rs`
(`the_install_test_passes_in_each_shell` and `an_installer_with_no_pin_check_fails_the_install_test`) runs
`scripts/install-test.sh`, which starts an HTTP server on `127.0.0.1`. `restricted-host` refuses `bind(AF_INET)`
(`PermissionError: [Errno 13]`, "the server did not start"); `no-loopback` has no loopback (`curl: (7)`). The fix: skip
these two tests on such hosts, as the other tests of the job do (`restricted-host: 143 tests skipped (bind(AF_INET)
refused)`; find the helper they use), or serve the files with no TCP (a `file://` mirror). Then record #598 (its row in
`TODO/issues.md` and `TODO/github.txt`: deny and release passed, ci failed on the two tests, the fix commit) and ask for
ci, deny and secrets on the new head in one request. The head `d440d36` (T-383) is not in a CI run yet.

## The work of 2026-11-16 (all on `main`, see the Done sections)

* T-358 step 3 (partial): consumers as tests. `scripts/consumer-dotnet.sh`, `scripts/consumer-python.sh` (urllib and
  requests under the shim; in `ci.yml` job `static` with a plant), `scripts/consumer-pod.sh` (`podnet pod` around
  python3 and curl in each lab; local only, CI has no podlab).
* T-382 done: `pod` refuses the netns backend where a TCP connect is refused (Landlock).
* T-383 done (55908f6, d440d36): the fake DNS answers DNS over TCP; `use-vc` is set where UDP is refused. Checked by the
  lead before the push: check.sh with 2023 tests, clippy `--features full`, the i686 check, all exit 0.
* T-384 open (P3): `--backend netns-kernel` in `getsockname-denied`.
* T-385 done: podcrates v0.58.0, the glue of pod-manual removed (`--help` byte-identical).
* T-360 done: `scripts/install.sh` on the installer core of podcrates (vendored in `scripts/install/`,
  `scripts/check-install.py`, `scripts/install-test.sh`); `release.yml` stamps the pins.
* T-358 step 1: the row proxy-only of the lane (`podlab run --allow-connect`, `tests/consumers/connect-proxy.py`);
  `scripts/get-podlab.sh VERSION DIR` gets podlab from the release of podcrates (SHA-256 checked; podcrates signs no
  asset yet).
* T-386 done: podcrates v0.59.0; T-380 and T-381 done. The doctor probe tries an AF_INET stream socket, then datagram.

## CI

A push of the cloud session starts no workflow run. Ask the owner for runs with an issue titled `[gh action] requested`
(a block of `gh workflow run` and `gh run watch` pinned to one commit; see GitHub #528 to #530). Map each request in
`TODO/github.txt` and `TODO/issues.md`, and close it when its results are in. Do not change the SHA of a request
after it is sent: the owner can run it at once (GitHub #531). Put later commits into the next request.
A release request (`docs/maintenance.md`, "Release") chains: the checks, the dry run of `release.yml`, the publish
(`-f publish=vX`), the release on the tag, and `release-proofs.yml -f tag=vX`. The release on the tag is a run of the event
`workflow_dispatch` on the branch `vX` (not `push`): find it with `gh run list --workflow release.yml --branch vX`.
For `v0.4.0-beta.11` and `beta.12` that lookup gave the run of the publish itself; for `beta.13` (GitHub #579) it gave a run of its own (43714213695). Both forms pass.
`release.yml` tags the head of `main` at the publish: push nothing to `main` from the request to the tag.

Each workflow passes on 0a0f3e5, the tag `v0.4.0-beta.13` (GitHub #579): ci 43711402086, deny 43711405265, secrets
43711407779, live 43711410272; fuzz 43654449404 passed on 6536905 (GitHub #569). ci 43718529045 and deny 43718532371 pass on f6aaeca,
the take of podcrates `v0.52.0` (GitHub #581). ci 43728239022, deny 43728242664 and secrets 43728245947 pass on 78a3911, T-356 and T-357 (GitHub #583). Read the runs (the watcher, `CLAUDE.md` step 1), not only the comment of a request: the runs of #583 ended an hour before any comment.

The watcher is `watch.py` of PodUtils/podcrates (`CLAUDE.md`, "Talk with the other sessions", step 1): issues, pull
requests, comments and Actions runs in one monitor. When a run of a request ends, record it and go on: do not wait for the
comment of the request.

## Next steps

0. The release gate (T-358) comes before the next beta (`docs/decisions.md`, 2026-11-15). The help and the manual are on `pod-manual` (T-359, done). The lane of step 1 runs with `scripts/lab-lane.sh` (`docs/development.md`, The lane in the
   profiles of pod-lab):

   1. `sh scripts/get-podlab.sh v0.59.0 .work/podlab-v0.59.0` (the release asset of podcrates, SHA-256 checked).
   2. Build the release binary of podnet. Copy it to a directory that uid 65534 can read and write, not in `/tmp`
      (a lab hides `/tmp`), for example `/var/tmp/lane/` with mode 1777.
   3. As root, run the lane as uid 65534: `setpriv --reuid=65534 --regid=65534 --clear-groups env HOME=DIR
      LANE_DIR=DIR/out sh scripts/lab-lane.sh .work/podlab-v0.59.0 DIR/podnet [PROFILE...]`.

   The first run (2026-11-16, T-358): 25 labs hold as uid 65534; the open failures are T-380 (`link-unix` in the
   Landlock labs) and T-381 (`pod-netns` in `udp-denied`), both fixes of podcrates. The lead writes the COORD issues
   `(to podcrates)` for them. podcrates v0.59.0 fixes both (T-386): the lane of 2026-11-16 with `podlab` v0.59.0
   gives 187 rows pass and no failure. `docs/lab-profiles/` is the hand-off of the six network profiles to podcrates.

   Step 3, the first consumer (2026-11-16): `sh scripts/consumer-dotnet.sh PODNET [PODLAB [PROFILE]]` runs a .NET 8
   `HttpClient` (`tests/consumers/dotnet-http`) under the shim with `--forward 127.0.0.1:PORT=unix:PATH`. It passes
   with no lab and in `bind-denied` as uid 65534; with no shim the lab refuses the connect (`Permission denied`). The
   job `static` of `ci.yml` runs it with no lab, with a plant (T-356 removed). It needs `apt-get install -y -qq
   dotnet-sdk-8.0`. Python and `pod` consumers are in too (see above).

   Left in T-358: a `gate.yml` CI job that runs the lane as non-root (CI must get podlab with `scripts/get-podlab.sh`);
   the network labs (their entry 716 in podcrates, on their `main` for the tag after v0.59.0: proxy-only, proxy-tls-reterminate,
   proxy-per-name, sni-allowlist, no-proxy-no-dns, ws-upgrade-refused, ipv6-only); the `icmp` rows; the relay and the
   tunnel as podssh and podrunner call them; T-384. No beta before the gate.
1. Ask for runs of each new head with a `[gh action] requested` issue after a batch of changes.
2. At each tag of podcrates: `sh scripts/agent/podcrates.sh`, and take the crates whose hand-off is at the tag (T-137).
3. Go on with the work order of `TODO/PROGRESS.md`. The disk holds one build: run one agent at a time in this checkout,
   with `CARGO_INCREMENTAL=0`. Close each issue whose entries are done through the record (its row and its line
   `closed`, with the commits and the CI runs); `issue-sync.yml` sets the state on GitHub (`docs/maintenance.md`, "The issue loop"). Issues #484 to #501 are deferred by the owner: leave them open.
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
| #597 (COORD-096, the takes of v0.58.0 and v0.59.0) | podcrates reads it; close it when podcrates closes #200 and #204. |
| #592 (COORD-095) | T-358 (the release gate); T-380 and T-381 are done. |
| #587, #589 | podcrates closes #185 and #188. |
| #413, #425, #437, #439, #440, #448, #460 | Older proposals and asks; see `TODO/issues.md`. |

## Waiting on the owner

* Q2 in `docs/decisions.md`: remove T-305 by ruling, so that #475 stays closed.
* The ~76 issues of Nemo-010 (author_association NONE): add the label `confirmed`, or add the author to the team.
* The relay-tls-proxy bundle of the owner's local agent was not reviewed: the review stopped at a safety block of the
  model. The owner can ask a narrower question.
