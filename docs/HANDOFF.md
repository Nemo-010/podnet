# Handoff

The state at 2026-11-14. Read this file first, then `CLAUDE.md`, `TODO/PROGRESS.md`, `docs/maintenance.md` and
`docs/decisions.md`. Remove this file when the lead session has no unfinished step.

## The goal

The owner (2026-11-13, `docs/decisions.md`, The complete beta): the beta test round is cancelled. Close each GitHub
issue except the deferred ones, then cut a final complete beta. The takes of PodUtils/podcrates follow its plan
(PodUtils/podcrates#133): the next tags bring `pod-cloudflare`, `pod-egress`, `pod-derp`, `pod-ts` and `pod-acme`.

## Where the code is

* All finished work is on `main` of `PodUtils/podnet`. The toolchain is Rust 1.99.1 (`rustup default 1.99.1`).
* podnet takes the crates of `PodUtils/podcrates` by the git tag `v0.56.0` (T-137), with `pod-warp`: `src/warp/` is glue
  over the crate and takes fixes only (each fix goes to podcrates as a COORD issue).
* The newest release is `v0.4.0-beta.13` (T-355, 0a0f3e5).
* The record is `TODO/`. `cargo todo check` agrees: 378 entries, 12 open, 11 partial, 37 blocked, 318 done.

## Unfinished work (not on `main`)

None. The patches of the earlier session are merged: T-153 (085f5c8) and the WARP parts of the router, T-097, T-180, T-099
and T-061 (3855573).

T-287 and T-301 are done.

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

   1. `git clone --depth 1 --branch v0.56.0 https://github.com/PodUtils/podcrates .work/podcrates-v0.56.0`, then in it
      `CC_x86_64_unknown_linux_musl=clang cargo build --release -p pod-lab --features cli --target
      x86_64-unknown-linux-musl` (the toolchain of podcrates needs `rustup target add x86_64-unknown-linux-musl` in
      that directory).
   2. Build the release binary of podnet. Copy both binaries to a directory that uid 65534 can read (`.work/lane/`).
   3. As root, run the lane as uid 65534: `setpriv --reuid=65534 --regid=65534 --clear-groups env HOME=DIR
      LANE_DIR=DIR2 sh scripts/lab-lane.sh .work/lane/podlab .work/lane/podnet [PROFILE...]` (DIR and DIR2 of uid
      65534, not in `/tmp`).

   The first run (2026-11-16, T-358): 25 labs hold as uid 65534; the open failures are T-380 (`link-unix` in the
   Landlock labs) and T-381 (`pod-netns` in `udp-denied`), both fixes of podcrates. The lead writes the COORD issues
   `(to podcrates)` for them. `docs/lab-profiles/` is the hand-off of the six network profiles to podcrates.

   Step 3, the first consumer (2026-11-16): `sh scripts/consumer-dotnet.sh PODNET [PODLAB [PROFILE]]` runs a .NET 8
   `HttpClient` (`tests/consumers/dotnet-http`) under the shim with `--forward 127.0.0.1:PORT=unix:PATH`. It passes
   with no lab and in `bind-denied` as uid 65534; with no shim the lab refuses the connect (`Permission denied`). The
   job `static` of `ci.yml` runs it with no lab, with a plant (T-356 removed). It needs `apt-get install -y -qq
   dotnet-sdk-8.0`. Next in step 3: a Python `requests` client, `podnet pod` around a dynamic program.
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
| #413, #425, #437, #439, #440, #448, #460 | Older proposals and asks; see `TODO/issues.md`. |
