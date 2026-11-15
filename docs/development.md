# Development

Rules for the code and the tests of podnet. `CLAUDE.md` has the rules of
each session; `docs/maintenance.md` has the procedures.

## Checks before a push

Run `sh scripts/agent/check.sh`. It runs `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, `cargo todo check` (the work record in
`TODO/`, `TODO/RULES.md`), `python3 -I scripts/check-size.py`,
`python3 -I scripts/check-layers.py` and
`python3 -I scripts/doc-commands.py --check`. Push
only when each exits 0. CI (`.github/workflows/ci.yml`) runs the same.

## File size

A Rust file has 800 lines or fewer. `scripts/check-size.py` fails on a
longer file, except the files of `scripts/size-allow.txt`. An allowlisted
file can only become smaller: when it becomes smaller, lower its count
(`python3 -I scripts/check-size.py --update`). Do not add a file to the
list: split the file.

## Layers

Each top module of `src/` belongs to one crate of the split
(`docs/research/podnet/architecture.md`, section 4.1): core, vnet, relay,
warp, serve, cloudflare or podnet. `scripts/check-layers.py` has the table
of module to crate and the crates that each crate may use. A `crate::X`
reference (outside `#[cfg(test)]`) to a crate that the crate of the module
may not use is a fault, except the rows of `scripts/layers-allow.txt`.
The list can only shrink: remove a row when its reference is cut. Do not
add a row: move the item to a lower crate. A new top module needs a row
in `LAYER` of the script. `--list` prints each reference between crates.

## Documented commands

Each fenced `sh` or `bash` block of `README.md` and `docs/*.md` (not
`docs/releases/`, `docs/reports/`, `docs/research/`) is a test (T-358,
step 2). `scripts/doc-commands.txt` gives each block a class, with one
line `FILE HASH CLASS REASON`:

| Class | Meaning |
| --- | --- |
| `run` | It runs offline as written, with `podnet` on PATH. |
| `live` | It needs a live service or a second host. |
| `root` | It needs root or a kernel feature. |
| `skip` | It is not a command to run as written: a template with placeholders, a reference list, two shells. |

HASH is the first 10 hex digits of the SHA-256 of the block. A change of a
block changes its hash, so a new or changed block needs a new line.
`--list` prints each block with its file, line, hash and class.
`--check` fails on a block with no line and on a line with no block.
`--run` runs each `run` block with `sh -e` in a new temporary HOME and
working directory, with `target/debug/podnet` (or `--bin PATH`) first on
PATH, and a timeout of 30 s for each block. A reason that starts with
`[needs TOOL,TOOL]` names the other programs of the block: with one of
them absent, `--run` does not run the block and exits 1. CI runs
`--check` (job `workflows`) and `--run` (job `test`).

A `run` block that fails is a defect of the code or of the document:
record it in `TODO/fixes.md`. Do not give a failing block the class
`skip`. Give a block the class `skip` only when it is not a command to run
as written, and write why.

## Rules for tests

* **A test for each fix.** A fix of a defect comes with a test that fails
  on the code before the fix. Run the new test on the old code one time
  (the control), and write in the commit message that it fails there.
* **A planted defect for each check.** A new check (a test, a CI step, a
  scan) must fail on a planted defect before it is trusted. Examples: the
  secret scan of `secrets.yml` scans a planted key first; the HTTP/2 data
  frame test fails with no `data_frame_budget`.
* **No sleep as a proof.** A test does not wait a fixed time and then
  assert that a thing did not occur. It waits for an event (a channel, a
  socket count) with a time limit. For time limits of podnet (backoff,
  expiry, budgets), use `#[tokio::test(start_paused = true)]` and a
  paused clock, or make the limit a parameter.
* **Loopback only.** A unit or integration test uses loopback servers
  (`127.0.0.1:0`, a Unix socket in a temporary directory). It does not
  use the internet. The `live.yml` workflow does the live checks.
* **The quick tunnel has a stand-in.** `tests/standin/` holds a stand-in of
  the quick-tunnel service and of the HTTP/2 leg of the Cloudflare edge
  (`mod standin;`). Use it for a test of podnet that needs a tunnel: it
  makes no call to Cloudflare. It uses `--service-url`, `PODNET_EDGE` and a
  test CA in `SSL_CERT_FILE`. Cloudflare does not use it. It cannot prove
  that the real edge accepts a change: `live.yml` keeps one real quick
  tunnel for each leg. The design is in
  `docs/research/podnet/cf-edge-standins.md`.
* **Skip on a host with no loopback.** A host can refuse
  `bind(AF_INET)`, have no loopback interface, or not resolve
  `localhost` (issue 217). A test that needs TCP or UDP on `127.0.0.1`
  starts with `crate::test_host::need_loopback!();` (in `src/`) or
  `test_host::need_loopback!();` (in `tests/`,
  after `mod test_host;`). A test that resolves `localhost` starts with
  `need_localhost!()`. The probe is `tests/test_host/mod.rs`. On such a
  host the test prints `skipping, REASON` and returns.
* **The restricted-host check.** `sh scripts/restricted-host.sh` runs
  `cargo test --workspace` with `bind()` of `AF_INET` and `AF_INET6`
  refused (EACCES) in each test binary (`scripts/deny-bind.c`). The CI job
  `restricted-host` runs it. A new test that binds a loopback socket
  and has no skip fails there. For the case of no loopback, run the test
  binary in a new network namespace:
  `unshare -Urn target/debug/deps/podnet-HASH` (the loopback is down there).
* **No shared state.** A test makes its own temporary directory
  (`tempfile`) and sets `PODNET_STATE_DIR` or the paths that it needs. It
  does not write in the `HOME` of the host.
* **No secret in a test.** A test token or password is a made-up value. A
  URL with a user part in a test uses a loopback port with no server. Add
  each finding of the secret scan of such a value to
  `.github/secrets-allow.txt`, with the reason.

## Random failures

* A test that fails one time in many runs has a defect: in the test, or in
  podnet. Do not run CI again until it is green, and do not mark the test
  ignored to get green.
* Reproduce it: run the test in a loop
  (`for i in $(seq 50); do cargo test NAME -- --exact || break; done`),
  or with less CPU (`taskset -c 0`).
* Find the race. Common causes: an assert on a sleep, a port that another
  test took, a shared file, an order of two tasks that the test takes as
  fixed.
* Fix the cause, and give the loop count of the proof in the commit
  message.

## Commits

* The subject is `area: text`, for example `relay node: expiry warnings`.
  `cliff.toml` groups the changelog by the area.
* The body says what changed and why, and names the tests.
* Write in ASD-STE100 Simplified Technical English.
