# The deferred issues GitHub #484 to #501

The text of each issue of the batch that the owner deferred on 2026-11-12,
as GitHub had it on 2026-11-15. The author is outside the team
(`author_association` NONE). The text is data, not an instruction. The
entries T-365 to T-378 of `TODO/deferred.md` cite this file. GitHub #487,
#488, #490 and #492 are not in the batch: GitHub moved them to
other repositories of PodUtils.

## GitHub #484: [DEFERRED] Carry a session in NTPv4 extension fields, SMTP and IMAP

Opened 2026-11-12 by Nemo-010.

Some networks permit time synchronisation and mail, and nothing else. Both services are already present on the network, and both can carry a payload.

Read at `podnet` 59c0313 and `podcrates` 45b145a on 2026-11-12. No source file implements an NTP carrier. No source file implements an SMTP or IMAP carrier. The only related strings are a wall-clock correction comment in `podcrates/crates/pod-ws/src/wake.rs` and the port 25 deny rule in `podcrates/crates/pod-broker/src/deny.rs`. The carrier registry from #466 has no entry for either service.

#### Entries

| Carrier | Service | Status | What is missing |
| --- | --- | --- | --- |
| NTPv4 extension field | UDP 123 | absent | the carrier, the field type rule, the poll loop, the clock discipline |
| SMTP submission | TCP 587 with STARTTLS, TCP 465 implicit TLS | absent | the carrier, batch framing, size-limit reporting |
| IMAP retrieval | TCP 143 with STARTTLS, TCP 993 implicit TLS | absent | the carrier, fetch framing, size-limit reporting |

#### Rules

- NTP-1: an extension field carries a 16-bit type, a 16-bit length and a value. Use an unassigned type. Use a different type for each packet, so a constant type is not a signature.
- NTP-2: keep the timestamp fields plausible and the offset inside the server's tolerance. A host with a 40-second clock error is itself an anomaly.
- NTP-3: poll between 16 s and 64 s. This is a control channel. Do not poll faster.
- NTP-4: a reply that the server truncates is a carrier limit, not a protocol error. Report it and reduce the payload.
- MAIL-1: one message is one batch. Report the latency floor as the poll interval.
- MAIL-2: leave a lost message to the session ARQ. Do not retransmit inside SMTP or IMAP.
- MAIL-3: use a per-session credential. Never use the account password.
- MAIL-4: treat a server size limit or rate limit as a carrier limit. The session stays open.
- MAIL-5: use submission ports 587 or 465, never 25. The egress broker refuses port 25 by a fixed rule (`pod-broker/src/deny.rs`, `FIXED_PORTS`), and a carrier that needs that rule relaxed is a carrier that changes the egress policy.
- Both carriers declare a cost class and a rate, so the operator sees the price before choosing the carrier.

#### Gate

- A local NTP server: 300 bytes each way, a new field type per packet, offset error under 1 ms over 100 exchanges.
- A local submission server and a local retrieval server: 300 bytes each way with STARTTLS, and again with implicit TLS.
- A server that refuses a large message: the carrier reports a limit and the session stays open.
- `link carriers` lists both carriers with a cost class and the current rate.

#### Dependencies

Both carriers implement the carrier registry contract from #466. The record layer from #472 runs above them. Neither needs a new transport, because `pod-tls` already speaks client and server TLS.

#### Relations

#469 scopes the DNS, ICMP and WireGuard carriers. It does not cover the two permitted services here.

## GitHub #485: [DEFERRED] Carry a session through a TURN allocation, with RFC 6062

Opened 2026-11-12 by Nemo-010.

A conferencing network permits TURN. A TURN allocation is a relay the network already trusts, and it carries a session when every other path is closed.

Read at `podcrates` 45b145a and `podssh` ef3666f on 2026-11-12. `crates/pod-proxy/src/stun.rs` is an RFC 5389 codec with tests and two injected defects (`proxy-stun-length-trusted` and `services-stun-transaction-not-checked`). No source file implements `ALLOCATE`, `CREATE-PERMISSION` or RFC 6062 `CONNECT`. `podssh/TODO/roads.md:9426` records that no `podssh` code speaks STUN. T-621 in `podssh/TODO/mesh.md` specifies a TURN road and `podssh/TODO/INDEX.md` indexes it.

#### Entries

| Capability | Status | What is missing |
| --- | --- | --- |
| UDP allocation (RFC 8656) | absent | allocation, permission, refresh, channel data |
| TCP allocation to a peer (RFC 6062) | absent | the relayed stream and its framing |
| Traversal with a relay fallback | partial | #469 scopes traversal; the relay fallback path does not exist |

#### Rules

- TURN-1: one STUN codec. Put the allocation in a `pod-turn` module beside `pod-proxy::stun`. Do not add a second STUN parser to `podssh`.
- TURN-2: refresh the allocation on a timer, not after a failure.
- TURN-3: declare the addressing mode on the transport, per #377. An allocation is not a resolved address.
- TURN-4: mark the TURN server as a third-party dependency and add a watch entry with a check date.
- TURN-5: keep the credential out of the log and out of `--json`, as #466 requires for identity material.
- TURN-6: report the allocation lifetime and the relay's rate limit in `status`.
- TURN-7: prefer the direct path when it exists. A relay adds a hop that sees the flow's volume.

#### Gate

- An allocation against a local TURN server, refreshed before expiry.
- A relayed TCP connection to a peer.
- `link_p2p` in two network namespaces: the direct path for a cone NAT, the relay path for a symmetric NAT.
- A named skip when the host refuses UDP, with the host error printed.

#### Dependencies

Needs the carrier registry from #466 and the addressing-mode declaration from #377. Shares the STUN message types with `pod-proxy::stun`, which is why the code belongs beside it.

#### Relations

#469 scopes NAT traversal, which this supplies the fallback for. T-621 is the road specification in `podssh`.

## GitHub #486: [DEFERRED] Close four QUIC gaps: MASQUE enrolment, HTTP/3 shape, version, connection ID

Opened 2026-11-12 by Nemo-010.

The QUIC roads share one stack. Four gaps remain on it: the MASQUE client is bound to one provider, the HTTP/3 shape is not a server shape, the UDP-over-QUIC-stream roads stop at one protocol version, and the connection ID never rotates.

Read at `podcrates` 45b145a and `podssh` ef3666f on 2026-11-12. `crates/pod-quic` (1,806 lines of Rust) holds the endpoint, the handshake race, the Version Negotiation probe and the datagram path, and it can act as a QUIC server. `podssh/crates/podssh-warp` speaks MASQUE `CONNECT-IP` over HTTP/2 and is bound to one provider's enrolment. `podssh/crates/podssh-ws/src/tuic/` (782 lines) implements TUIC v5 and refuses v4 at `tuic/link.rs:180`; Juicity is absent. One relay test inserts the h2 protocol name `connect-udp` (`podssh-relay/tests/relay_serve_h2.rs`); no client uses it, and `podssh-ws/src/wstunnel/mod.rs` refuses WebTransport. The only use of a QUIC connection ID in the tree is parsing it out of a Version Negotiation packet (`pod-quic/src/probe.rs`); no migration or rotation path exists.

#### Entries

| Capability | Status | What is missing |
| --- | --- | --- |
| MASQUE `CONNECT-IP` and `CONNECT-UDP` to any endpoint | partial | `--masque-endpoint`, one client split from the enrolment |
| HTTP/3 server shape | partial | a selectable shape and a conformance check |
| Juicity | absent | the road |
| Connection-ID rotation and migration | absent | the schedule, the migration path, the fallback |

#### Rules

- QUIC-1: split `podssh-warp` into a MASQUE client and one enrolment adapter per provider. Add `--masque-endpoint`. An IPv6-only host must not attempt an IPv4 connect (#366).
- QUIC-2: `CONNECT-IP`, `CONNECT-UDP` and WebTransport are three modes of one client. Keep one HTTP/3 library.
- QUIC-3: `--l4` is one HTTP/3 `CONNECT` per flow with no IP stack (#140). It is the cheap shape.
- QUIC-4: enrolment is a trait with one implementation per provider. A second provider passes the same session test as the first.
- QUIC-5: distinguish enrolment failure from transport failure. `warp status` reports one of the two, not "error".
- QUIC-6: offer `--h3-shape` with the `h3` ALPN, the control stream, the settings frame, and a congestion profile from a named set. A distinctive controller is itself a fingerprint, so the default is the common shape.
- QUIC-7: keep the datagram capability. QUIC datagrams are the cheapest UDP carrier where UDP is permitted.
- QUIC-8: Juicity needs a bounded pending queue and a stop-read signal. Document the drop policy.
- QUIC-9: rotate the connection ID on a jittered interval. A fixed interval is a signature. A server that refuses migration gets a new connection and a resumed session, not an error.
- QUIC-10: do not add a second QUIC stack.

#### Gate

- `CONNECT-IP` to an endpoint that is not the current provider; `CONNECT-UDP` to a local echo server; a start on an IPv6-only host with no IPv4 attempt in the log.
- A stock HTTP/3 client completes a request against the server shape. A qlog capture shows the control stream. `doctor` prints the congestion profile.
- UDP echo through TUIC at 5% loss with the queue bound asserted. A Juicity client completes a session against a reference server.
- A 10-minute transfer survives two rotations with no byte lost. A server that refuses migration yields a new connection and a resumed session.
- A second provider's road passes the same session test as the first.

#### Relations

#366, #368, #149 and #140 record the provider defects and the `--l4` ask.

## GitHub #489: [DEFERRED] Register third-party services as roads, with a watch, a date and a count

Opened 2026-11-12 by Nemo-010.

Third-party services are free capacity that somebody else operates. Each is a road, and each is also a dependency that can vanish. The tree has the mechanisms and no register: a road with no watch, no date and no count is a road that fails without warning.

Read at `podcrates` 45b145a and `podssh` ef3666f on 2026-11-12. `podcrates/crates/pod-services-watch` (5,355 lines) watches third-party endpoints and its runner records the day of the run in UTC (`src/main.rs:60-64`, `Date` from `pod-services`). `podssh/crates/podssh-ws/src/meek.rs` and `webtunnel.rs` are the fronted roads; `podssh-ws/src/irc` does not exist but `podssh-core/src/irc/` does (message codec and transfer), and `podssh chat --irc` drives it; IRC is implemented, not registered as a road. `podcrates` holds `pod-wormhole` (4,612 lines) and `pod-ws` (8,007 lines, with a wake path). `podssh` holds `podssh-ts` and `podssh-iroh`.

#### Entries

| Capability | Status | What is missing |
| --- | --- | --- |
| IRC as a registered road | partial | the road registration and the cost class |
| MQTT and Syncthing relays as roads | absent | the roads |
| Watch every third-party road | partial | a watch entry per road, and the date in `status` |
| `front=` health for a fronted road | partial | a refusal that names a dead front |
| Network adapters as a fallback class | partial | one class, and a prohibition on default selection |
| Dependency count in `status` | absent | the count |

#### Rules

- Third-party roads are one class with one interface: IRC, MQTT, Syncthing, piping and wormhole transit. Each is switchable off, is not a default, and carries a cost class.
- Every third-party road has a watch entry in `pod-services-watch` with a check date. `status` prints the oldest date. A road older than its stated interval is reported as stale, not as working.
- A watch failure degrades the road. It does not panic, and it does not abort the session.
- The watch runs from more than one network. One vantage point measures the watcher's network, not the service.
- `webfront`: a `front=` that no longer routes is a refusal with the front's name, not a timeout.
- Tor bridges, iroh, ZeroTier and Tailscale are a fallback class. No default selection picks one without a flag or a policy rule that names it. A fallback the user did not choose is a traffic shift nobody agreed to.
- `podssh-ts` has no `description` in its manifest. Add it, with the other adapter metadata, and add each adapter crate to the same register. A crate with no description is a crate a user cannot find.
- `status` prints the dependency count across seven classes: relay host, relay domain, certificate authority, carrier protocol, carrier destination, DNS resolver, third-party service.

#### Gate

- Each third-party road appears in `carriers` with a cost class and a last-verified date.
- A deliberately broken endpoint degrades the road, names it, and leaves the session running.
- `status` prints the oldest check date, and a stale entry is a failure.
- A configuration that uses one domain for every road reports one dependency, not six.
- A default selection without a flag never picks an adapter road.
- A missing `front` path produces a refusal naming the front within 1 s.

#### Relations

#469 scopes the DNS, ICMP and WireGuard carriers. #470 scopes mesh announcements, which is where a third-party relay is announced rather than selected. #433 and #407 record the provider-side defects that a watch would have caught.

## GitHub #491: [DEFERRED] Switch after a real handshake, with a cover script and multiplexed cover

Opened 2026-11-12 by Nemo-010.

A handshake that completes and then switches defeats a scanner that only opens a connection and looks. A TLS-in-TLS form defeats a scanner that completes a handshake and then looks at the record layer. Both are the same idea applied at two depths, and one cover site serves both.

Read at `podcrates` 45b145a and `podssh` ef3666f on 2026-11-12. `podcrates/crates/pod-tls/src/fragment.rs` (490 lines) splits the first ClientHello into records and carries tests, which is the record-level primitive a switch needs. `podcrates/crates/pod-tls/src/server/` and `server/served.rs` resolve a certificate per name. `podssh-ws/src/tls_wrap.rs` wraps a road in TLS; there is no post-handshake authentication byte, no cover script, and no multiplexed cover. ShadowTLS, Restls and Cloak are absent by grep across all three trees.

#### Entries

| Capability | Status | What is missing |
| --- | --- | --- |
| Real handshake, then switch on an authenticated byte | absent | the HMAC, the timestamp window, the fall-through |
| Cover script for the record layer | absent | the script format and its loader |
| Multiplexed cover with a volume budget | absent | the multiplexer |

#### Rules

- SWITCH-1: complete a real TLS handshake with the cover site. Authenticate in the first application data with an HMAC over the session key.
- SWITCH-2: on a mismatch, keep the bytes flowing to the cover site. A scanner without the password gets a working session, not a close, not a reset, not a delay.
- SWITCH-3: the HMAC covers a timestamp and a nonce, and the server keeps a window. Without both, a captured first byte replays.
- SWITCH-4: the cover site must terminate TLS on the same address, and the operator must hold its name.
- SCRIPT-1: a script describes the cover implementation's record sizes and timing. The inner record layer reproduces them.
- SCRIPT-2: a missing script is a failure at load. It must not degrade silently to a generic stream.
- SCRIPT-3: a script is a measurement of the cover, with the date and the cover's version. A script with no date is deleted.
- MUX-1: multiplexing is the last of the three. Its acceptance is volume: a page load and a session in one connection, sharing one budget.
- MUX-2: the multiplexer keeps the cover's own traffic first. A tunnel that starves the cover site is a tunnel that gets noticed.

#### Gate

- A switch against a local cover: an authenticated client gets the tunnel, an unauthenticated one gets a real page, and both present the same certificate. A capture of both shows no distinguishable first record.
- A replayed first byte outside the window is refused and the connection continues as the cover.
- A script replay reproduces the cover's record sizes within the stated tolerance; a missing script fails at load.
- A page load plus a session over one connection, with the cover's traffic served first.

#### Relations

The switch form needs the cover site and the delegated handshake. The record split it reuses is already tested in `fragment.rs`, so this does not need a new TLS stack.

## GitHub #493: [DEFERRED] Shape the record layer: padding profiles, entropy, first flight, pacing

Opened 2026-11-12 by Nemo-010.

A record layer that pads to one fixed bucket is a record layer with a signature. Padding has to follow the protocol it claims to be, and the session has to know what shape it declared.

Read at `podcrates` 45b145a on 2026-11-12. T-300 in `podnet/TODO/link.md` records "padding of records to buckets" and is tracked by GitHub #472; T-298 states that the length field covers payload plus padding and that a padding run must not be constant. No source file holds a padding profile as data, and no source file reports the padded fraction of a transfer. The session layer is `podcrates/crates/pod-session` (13,869 lines).

#### Entries

| Capability | Status | What is missing |
| --- | --- | --- |
| Padding profiles as data per protocol | absent | the profile files and the selector |
| Content-entropy control | absent | the band and the printable run |
| First-flight sizing | partial | the coalesce target and the rate bound |
| Pacing with jittered keepalives | partial | a per-channel window pacer for SSH (`podssh-ssh/src/pace/`, T-661); record pacing by measured rate and keepalive jitter are absent |
| Declared versus observed shape in `doctor` | absent | the check and its three states |

#### Rules

- PAD-1: a profile is a data file named for the traffic it imitates: page load, video call, terminal, file transfer.
- PAD-2: a profile is a distribution over record sizes plus a rule for when to pad: always, only in the first N records, or only below a threshold.
- PAD-3: the two positions that matter are before the first write and after every write. A profile states which one it uses.
- PAD-4: pad to buckets, not to a random length. The bucket set here is 0, 128, 512, 1024 and 2048 bytes. A bucket is chosen per record and never in a repeating pattern.
- PAD-5: report padded bytes as a fraction of total bytes in `status`, and in the transfer's own summary.
- ENT-1: where the carrier's payload is inspected for entropy, hold entropy per window and the longest printable run inside a target band. Do not set a target entropy value; a value is a signature too.
- ENT-2: salt the nonce with a coarse timestamp and accept a window of about four minutes. Refuse a replay outside it.
- FLY-1: coalesce the opening write to a size that matches the claimed protocol, and scale the target by the measured rate. A fixed target on a slow link is a stall.
- PACE-1: pace writes to the measured rate and jitter keepalives by about 20%. `podssh-ssh/src/pace/` already paces SSH channels by the path's own rate and round trip (T-661), but it is window-based flow control at the SSH layer; a record pacer and a jittered keepalive are still missing.
- PACE-2: never merge two logical writes into one record. Merging changes the application's own timing, and the receiving application sees it.
- PACE-3: the pacing decision belongs to the session, not the terminal, so a forwarded port gets it too.
- DECL-1: every road declares its profile and its claimed shape. `podssh doctor --shape` prints `declared`, `observed` and `diverged` for each road.
- DECL-2: a divergence is a named warning, not a failed session.

#### Gate

- Per profile: 1,000 sampled records against a recorded reference histogram, within a stated tolerance.
- Entropy inside the band over 1,000 windows, with one printable run of 16 bytes per 64 KiB.
- The first outbound record matches the configured size on a fast link, and the size that fits two seconds at the measured rate on a slow link.
- The echo benchmark at 1, 4 and 20 Kbit/s passes the two-second bound.
- `doctor --shape` prints one line per road, and a perturbed profile raises the warning.
- A transfer reports its padded fraction, and 1 MiB at the 512 bucket stays under 25%.

#### Relations

#472 scopes the record layer these profiles feed. #471 scopes the services that a forwarded port needs before pacing applies to it.

## GitHub #494: [DEFERRED] Shape the application: deferred responses, a generator, carriers in the payload

Opened 2026-11-12 by Nemo-010.

A tunnel that answers a request faster than the origin could is a tunnel. A tunnel that answers from a prepared copy, on the next request, is not. The first is a detector's easiest case, and the second is a change at the application layer with no new cryptography.

Read at `podcrates` 45b145a and `podssh` ef3666f on 2026-11-12. `podcrates/crates/pod-relay` (28,455 lines) is where a response cache would live; `podssh-relay/src/serve/` is where a response is forwarded today. No source file holds a response cache, a traffic generator, or a shape profile for an application. `pod-e2e` (5,165 lines) runs a session end to end and `pod-testkit` (819 lines) holds fixtures, so a generator has a home.

#### Entries

| Capability | Status | What is missing |
| --- | --- | --- |
| Deferred response replacement | absent | the cache, the serve-next rule, the overhead bound |
| Traffic generator with an application profile | absent | the generator and its profile files |
| Carriers inside an application's own bytes | absent | the class and its first member |

#### Rules

- DEFER-1: prepare the next response while serving a plausible one now. Never replace a response in line; the timing of a replacement is what a classifier sees.
- DEFER-2: the cache is local to the relay, and the bandwidth overhead is bounded by the configured chunk size.
- DEFER-3: reproduce the published result before claiming it. The published figures are 2 MiB chunks, classifier accuracy falling from 91% to 12%, 21.3% bandwidth overhead, a size-distribution distance of D = 0.03 at p = 0.98, and first-request latency at 214% of baseline. Those are one implementation on one testbed. Treat them as a target.
- DEFER-4: a cache miss is served live, and its timing is the origin's timing. A miss that is faked is a worse defect than no cache.
- SIM-1: generate request/response alternation at human timescales, object sizes from the application's own distribution, and idle periods.
- SIM-2: human-scale gaps are 100 ms to 2 s between objects, with longer pauses while the user reads.
- SIM-3: the generator is driven by a profile file, so a new application is a new file. Without it, "looks realistic" is an opinion.
- SIM-4: the generator is a tool in `pod-e2e` or `pod-testkit`, not a hidden mode of a road.
- APP-1: the application-carrier class is a roadmap, not a product. Only two members are plausible as crates: WebRTC beside `pod-quic` and `pod-ws`, and a speech-cadence shim as a research artefact.
- APP-2: game and video-call carriers are per-application adapters. If built, they are roads in `podssh-ws`, not core crates.
- APP-3: the first video-call design was detectable in over 90% of cases after measurement. Record that, so the class is not presented as solved.
- APP-4: for any member that is built, the application's own client works through the same path.

#### Gate

- A page load replayed twice through the relay: the second request's timing is compared against a direct fetch, and the first request is not faster than the origin.
- A bandwidth accounting test bounds the overhead by the chunk size.
- A cache miss shows the origin's timing.
- The generator's output scores as the claimed application in the classifier harness, within a stated margin.
- For a WebRTC member: a linkage bound of at most 1/K flows, with K stated.

#### Relations

#472 owns the record layer, which none of this needs. #471 owns the services a forwarded port needs. The classifier harness this is measured against is the subject of the verification work.

## GitHub #495: [DEFERRED] Add a packet sidecar: desync search, disagreement probe, fake TCP, bonding

Opened 2026-11-12 by Nemo-010.

A network that rewrites packets cannot be trusted to carry a flow unmangled, and a network that refuses UDP cannot be trusted to carry a datagram. Both answers need a component that sits below the stream and above the link, and neither exists in the tree.

Read at `podcrates` 45b145a, `podssh` ef3666f and `podnet` 59c0313 on 2026-11-12. `podcrates/crates/pod-hostprobe` (8,175 lines) already returns three values for a capability check. `podcrates/crates/pod-netstack` (5,368) and `podcrates/crates/pod-proxy` (11,675) are the places a packet path would live. No source file implements a desync primitive, a split-and-tamper behaviour, fake TCP, or link bonding: a search of all three trees for `desync`, `geneva`, `phantun` and `symtcp` returns nothing. `podssh-relay/tests/relay_serve_h2.rs` inserts the h2 protocol name `connect-udp` and no client uses it.

#### Entries

| Capability | Status | What is missing |
| --- | --- | --- |
| Desync sidecar with a genetic search | absent | the crate, the middlebox simulator, the search |
| Disagreement probe with a third outcome | partial | the probe and its third value |
| Split-and-tamper as one library | absent | the library and one config format |
| Fake TCP datagram channel | absent | the channel and its framing |
| Link bonding | absent | the scheduler |
| Stated limits | absent | the page and the conditions (`podvm/docs/limits.md` covers the chroot, not the carriers) |

#### Rules

- SYN-1: the sidecar needs raw sockets or a TUN device. It is never a built-in path. Where raw sockets are unavailable it reports a named skip, not a failure.
- SYN-2: five primitives: drop, tamper, duplicate, fragment, coalesce. A strategy is an ordered list with triggers: first outbound data packet, SYN-ACK, first inbound RST.
- SYN-3: the genetic search runs against a local middlebox simulator, never against a live path in CI.
- SYN-4: a strategy is a file with a date and the network it was found on. A strategy with no date has no evidence behind it.
- PROBE-1: find the disagreement by sending legal but unusual segments: an out-of-window segment, a retransmission with a different payload, a FIN before the data is acknowledged.
- PROBE-2: report "could not determine" as a third value. A probe that cannot run is not a passing probe.
- SPLIT-1: split at an offset, disorder, fake TTL, tamper a checksum, pad the first segment. One library, one configuration format.
- SPLIT-2: document that reassembly defeats split and disorder, and that a valid-checksum requirement defeats tamper. A behaviour with no stated defeat is a claim, not a technique.
- FAKE-1: prefer a real TCP carrier with message framing. Fake TCP exists for an application that needs datagram semantics below our framing, and it is the last resort.
- FAKE-2: state the cost: an extra round trip, a changed TTL, and a checksum that a middlebox may repair.
- BOND-1: per-flow assignment, not per-packet round robin. A flow does not hop paths mid-stream unless the path fails.
- BOND-2: aggregate throughput within 20% of the sum on two links at 10:1 bandwidth.
- LIM-1: the limits of this group go in a page that names, for each one, the condition that triggers it and the check that detects it. `podvm/docs/limits.md` is the model: it states what a chroot does not isolate. A carrier page states what a carrier does not do.
- LIM-2: do not build the sidecar into the default binary. A build feature is the only way it ships.

#### Gate

- Against the simulator: each primitive produces the intended state divergence, and the search converges on a hand-written strategy.
- The simulator exposes a known disagreement and the probe finds it; a silent path reports "could not determine".
- Each split behaviour has a simulator test, and a named failure when the device reassembles.
- UDP echo through fake TCP against a middlebox that refuses UDP; a named skip when raw sockets are unavailable.
- Two emulated links at 10:1 carry one session within 20% of the sum, and the flow does not move mid-stream.
- The limits page names a detection for each limit, and a test fails when one is missing.

#### Relations

#473 scopes the direction split, which composes with the sidecar but does not replace it. #469 scopes the datagram carriers above this layer. The three-valued probe is `pod-hostprobe`'s existing contract.

## GitHub #496: [DEFERRED] Keep the session end to end: opaque destination and keys from the ends

Opened 2026-11-12 by Nemo-010.

A relay that can read the session is a relay that can be asked for it. The tree already has the end-to-end handshake; the record layer does not use its keys, and the destination is in the request path.

Read at `podnet` 59c0313, `podcrates` 45b145a and `podssh` ef3666f on 2026-11-12. `podcrates/crates/pod-e2e` (5,165 lines) speaks Noise XX over an arbitrary byte stream, with keys from `pod-identity` (1,732 lines). No crate in `podnet` depends on `pod-e2e`: a search of every `Cargo.toml` in the tree returns nothing. `podssh-relay/src/serve/forward.rs` serves `GET /connect/<host>/<port>`, and `podssh/docs/cli.md` states that a target and its token are visible to every hop on a cleartext road. T-298 in `podnet/TODO/link.md` states the flaw in its own premise: the record layer's keys come from `EXPORTER-podnet-record`, which a hop that terminates TLS holds on both sides.

#### Entries

| Capability | Status | What is missing |
| --- | --- | --- |
| End-to-end session authentication under the relay | absent | the dependency and the handshake |
| Opaque destination | absent | the envelope and the identifier |
| Record keys from the ends | absent | the key source |
| A stated table of what each hop learns | absent | the table and the check |

#### Rules

- E2E-1: run the Noise XX handshake inside the carrier, before any application byte.
- E2E-2: bind the handshake transcript to the relay's advertised pair, so a relay cannot splice two sessions into one.
- E2E-3: a relay that cannot open the handshake still forwards it. Forwarding must not require reading.
- E2E-4: `podnet` takes `pod-e2e` as a dependency, and the test dumps every byte at the relay. The dump contains no plaintext and no key.
- OPAQUE-1: the destination is not in the request path. Put it in the encrypted envelope, or behind an identifier that the relay resolves.
- OPAQUE-2: keep `/connect/<host>/<port>` as a compatibility path behind an explicit flag, and mark it as the path that leaks the destination.
- KEY-1: record keys come from the end-to-end handshake, not from a per-hop TLS exporter.
- KEY-2: keep the header exactly as specified: 24 bytes, `version` u8, `type` u8, `flags` u16, `stream` u32, `counter` u64, `length` u32, `pad` u16, `reserved` u16, plus a 16-byte tag. The nonce is a 4-byte direction prefix and a 64-bit counter. The suite is `chacha20-poly1305` by default and `aes-256-gcm` as the alternative.
- KEY-3: a suite mismatch exits 78, before any record is sent.
- KEY-4: rekey at 2^31 records or 1 GiB, whichever comes first, and never after the ceiling.
- LEARN-1: publish one table of what each hop knows. The first hop knows the client's address, the target relay and the volume. The relay knows the pair and the volume. The far hop knows the destination.
- LEARN-2: a token for one relay does not open another, and a token for one service does not open another.
- LEARN-3: `podnet tunnel --inspect` is how the table is checked: it shows the exchange as a hop sees it.

#### Gate

- A relayed session whose relay dumps every byte cannot decrypt a record; the dump contains no destination and no plaintext.
- A suite mismatch exits 78.
- A 1 GiB transfer rekeys before the ceiling and completes.
- A splice attempt by the relay fails, because the transcript is bound to the advertised pair.
- An instrumented relay's recorded metadata contains no destination, no service name and no peer identity.
- A token minted for one relay is refused by another, and a token for one service is refused by another.

#### Relations

#466 scopes the identity the keys come from. #472 scopes the record layer, and its own premise states that the layer can be built without #466 and would then be wrong. #435 requests the capture that makes the table checkable. #467 scopes the stream carriers this runs over.

## GitHub #497: [DEFERRED] Make the pool live, the budget per deployment, tokens expiring, hosts movable

Opened 2026-11-12 by Nemo-010.

A relay pool that is chosen once at startup is a pool of one, whatever it lists. Three defects share one cause: the alternates are read and never re-read, the budget is a compile-time constant, and the reverse road's calls reach one host.

Read at `podnet` 59c0313 and `podcrates` 45b145a on 2026-11-12. At HEAD the alternates are chosen once and never re-ordered; the relay leg's budget is 60 MiB over 11.5 hours, applied to every relay a user has; and the reverse road's pair, node and stop calls reach one relay host. The pool itself lives in `podcrates/crates/pod-relay` (28,455 lines), which became a handoff crate in v0.30.0 and v0.31.0. `podkey` exists as a repository with no source.

#### Entries

| Capability | Status | What is missing |
| --- | --- | --- |
| Live pool with a health score | partial | the re-read, the score, the attribution |
| Per-deployment budget | absent | the configuration key and the override |
| Tokens with a lifetime | absent | expiry, scope, renewal |
| A deployment that moves hosts | partial | the state file and the pair rule |

#### Rules

- POOL-1: the pool is a live list with a health score, re-read on a timer.
- POOL-2: selection is attributable. A probe result names the member that produced it.
- POOL-3: hysteresis and cooldown, not eviction on one failure. A pool that evicts on one failure oscillates.
- POOL-4: a degraded relay costs one start-up penalty, not one per attempt.
- POOL-5: a relay's budget comes from the relay's published limits, and a per-deployment value overrides the compile-time constant. Two consequences to fix: a user on a metered host cannot state a budget, and a relay with a different allowance cannot be used at all.
- POOL-6: the reverse road dials the alternates the pool loads. A pool whose members are never dialed is a list, not a pool.
- TOKEN-1: every token has a lifetime and a scope, and the client renews before expiry rather than on failure.
- TOKEN-2: the short-lived form is the default. A long-lived form needs an explicit flag and is recorded in `status`.
- TOKEN-3: a token is refused after its lifetime, and `status` shows the remaining lifetime.
- TOKEN-4: issuance and expiry belong in `podkey` when it has source, and in `pod-identity` until then. One place, not two.
- MOVE-1: a pair is renewable without a manual mint. A credential that cannot be rotated is a credential that is never rotated.
- MOVE-2: a pair works on a second host, so a relay can be rebuilt elsewhere.
- MOVE-3: the relay's state is a file, not a process. The state file alone rebuilds the deployment.
- MOVE-4: a moved relay keeps its pair and its name, and clients reconnect without re-pairing.
- MOVE-5: relocation is also the answer to an unreachable relay: move the name and the pair, not the clients.

#### Gate

- A pool with one black-holed relay loses one start-up penalty, not one per attempt.
- A mid-run alternates re-read takes effect without a restart.
- A per-deployment budget from configuration overrides the constant.
- The reverse road dials the second member after the first fails.
- A token is refused after its lifetime; a renewal happens before it expires; `status` shows the remaining lifetime.
- A relay moved from one host to another keeps its pair and its name; clients reconnect without re-pairing; the state file alone suffices.

#### Relations

#359, #360 and #397 are the three open records of the defects above. #373 asks for hysteresis, cooldown and attributable probes. #358 and #482 are the manual steps a pair rotation needs today. #466 scopes the registry the pool sits above.

## GitHub #498: [DEFERRED] Resume a session across hours, and serve the full service surface at a peer

Opened 2026-11-12 by Nemo-010.

A session that survives a ten-minute gap is a session that survives a network hiccup. A deployment on a satellite link or a scheduled radio window is offline for eight to twelve hours by design. The same session layer has to serve both, and it has to survive losing its carrier rather than its connection.

Read at `podcrates` 45b145a and `podssh` ef3666f on 2026-11-12. `podcrates/crates/pod-session` (13,869 lines) implements resume, with a 10-minute acceptance and a 1 MiB digest test; 300 echoes run before a gap so stall detection is exercised. The carrier seam exists and takes `--carrier stdio`, `fd:N` or `exec:CMD` (`podssh/crates/podssh-cli/src/carrier.rs`). `podcrates/crates/pod-job` (1,620 lines) models a job with input and output, and `pod-supervise` (3,073) runs it; no queue survives a disconnect. `podcrates/crates/pod-services` holds the service data and `podnet` holds the listeners. SOCKS5 `CONNECT` and `UDP ASSOCIATE` are served at HEAD (`podnet/src/warp/proxy.rs`; `UDP ASSOCIATE` behind the `udp` flag), and `BIND` (command `0x02`) is refused by the ruleset with reply `0x02`. `podnet/src/proxy.rs` caps request headers at 32 KiB (`MAX_HEAD_BYTES`), above the 8 KiB a browser with many cookies sends, and the forward path shuts the target down (`podssh-relay/src/serve/forward.rs:286`). `serve-at` does not exist.

#### Entries

| Capability | Status | What is missing |
| --- | --- | --- |
| Resume measured in hours | partial | the policy value and the bound |
| Carrier handover without a new session | absent | the identity, the sequence, the inherit |
| Degraded-link profile with three carriers | absent | the profile and its derived timeouts |
| Store-and-forward shell | absent | the queue and the collect-by-id client |
| Framing for lossy carriers | partial | the shared layer |
| Full SOCKS5 surface at a peer | partial | `BIND` refused by the ruleset; tests for `UDP ASSOCIATE`, the header cap and the half-close |

#### Rules

- RESUME-1: the resume window is a policy value with a floor of 10 minutes and a ceiling the operator chooses.
- RESUME-2: the state a resume needs is bounded: the unacknowledged window plus the replay cache, not the whole stream.
- RESUME-3: the digest is the acceptance, not the reconnection. A resume that reconnects and loses the tail is worse than a reconnect that admits it did.
- RESUME-4: a stalled reader is isolated within the stated timeout, not left holding the session.
- HAND-1: handover needs a session identity that is not the connection, a sequence that is not the socket's, and a window the new carrier inherits.
- HAND-2: handover moves to the cheapest available carrier from the registry, not the first that answers.
- HAND-3: handover survives the old carrier disappearing mid-write, and it is a named event in `status` and in the log.
- HAND-4: carrier handover is not QUIC connection migration. It crosses protocols, and it must not be built on the QUIC path.
- PROF-1: the degraded profile states bandwidth, latency, loss, jitter, maximum outage and reconnection budget. Every session timeout derives from it.
- PROF-2: at 572 bit/s a 1 KiB record takes 14 s, so a 30 s idle timeout is shorter than one record. No default may be shorter than one record under the profile.
- PROF-3: the profile is selected, not detected. A host that must measure its link before it can talk has already failed.
- SHELL-1: the unit is a job: a command with its environment, its input and its output, submitted to a queue and collected by id on the next connection.
- SHELL-2: output is bounded and truncated at a stated limit with the truncation recorded, or a `yes` in a queued command fills the disk.
- FRAME-1: sequenced chunks with an explicit length; consume-then-delete, so a crash does not lose an acknowledged chunk; micro-batching with a bounded delay.
- FRAME-2: the framing lives in `pod-session`, so `podnet` and `podssh` use one implementation rather than two.
- SOCKS-1: `CONNECT`, `BIND` and `UDP ASSOCIATE`. `CONNECT` and `UDP ASSOCIATE` exist; `BIND` is refused by the ruleset, so give it one of two outcomes: an implementation, or a `doctor` line that states the refusal (`BIND` is used by FTP and by some file-transfer tools).
- SOCKS-2: keep the header cap above 8 KiB, because a browser with many cookies sends more than 4 KiB. `MAX_HEAD_BYTES` is 32 KiB; the test that asserts the cap is what is missing.
- SOCKS-3: raw TCP carries 1 MiB with a half-close, so a client that shuts down its write side and waits for the response works. The shutdown is implemented; the 1 MiB test is not.
- SOCKS-4: each service is a row with an acceptance test, not a code path. Do not add a transport that already runs.
- SOCKS-5: `serve-at` is absent. It is the one listener here that has no code at all.

#### Gate

- `tests/link_resume.rs` with a gap configurable from 10 minutes to 12 hours, and the 1 MiB digest equal across the gap.
- A carrier killed mid-transfer: the session continues on a second carrier with a matching digest, and `status` names the handover.
- The echo benchmark under the profile at 1 Kbit/s, 5% loss and a 60 s outage, passed by the serial, radio and spool carriers.
- A command queued while offline runs on reconnect and is collected by id; a 1 GiB output is truncated at the limit with the truncation recorded.
- 1 MiB over a carrier that drops 1 message in 20; a consumer crash between receive and consume loses nothing acknowledged; 1,000 small writes produce fewer than 100 chunks.
- `tests/service_socks.rs` covers `CONNECT`, `UDP ASSOCIATE` and the `BIND` outcome; the 8 KiB header test; the 1 MiB half-close test; and `serve-at` if it is built.

#### Relations

#466 scopes resume and the carrier registry; its acceptance is a 10-minute gap, which is the floor here. #467 scopes the stream carriers that handover moves between. #471 scopes the eight services, and the unusual parts above are the ones a client notices. #55 asks for exactly the three framing properties.

## GitHub #499: [DEFERRED] Carry a session with no network: serial, HF, spool, discovery, pairing

Opened 2026-11-12 by Nemo-010.

A host with no path has a serial port, a radio and a disk. Those three carriers are what a disconnected deployment actually has, and each one needs the session layer to work without a network at all.

Read at `podcrates` 45b145a, `podssh` ef3666f and `podnet` 59c0313 on 2026-11-12. A serial line is already a local end of `podssh pipe` (`serial:PATH[,BAUD]`, `podssh-cli/src/pipe/serial.rs`, 354 lines, T-181: raw 8N1, `O_NOCTTY`, `TIOCEXCL`, settings restored at each end), and `--carrier` takes `stdio`, `fd:N` and `exec:CMD`, so a modem process is reachable today. What no source file implements is the framing above a serial line (AX.25, KISS), an ARDOP or VARA modem driver, a spool, mDNS discovery, an offline bundle, a capture sink and a replication store; short-code pairing exists in `pod-wormhole` and the relay's mailbox: a search for `ax25`, `kiss` and `spool` finds no implementation in the three trees, and `serial` finds only the `pipe` end. `podcrates/crates/pod-extract` (2,945 lines) and `pod-identity` (1,732) are the pieces a spool and a bundle would reuse.

#### Entries

| Capability | Status | What is missing |
| --- | --- | --- |
| Serial line as a `--carrier` address | partial | the `serial:` parse arm; the end itself exists in `pipe` |
| AX.25/KISS framing over that line | absent | the framing and the baud-derived timer |
| HF radio carrier | absent | the modem driver and the busy handling |
| Store-and-forward spool | absent | the spool and its checksum |
| Local discovery and one radio interface | absent | the signed announcement |
| Offline bundle | absent | the manifest and the delta update |
| Capture sinks | absent | the record format and the merge |
| Pairing with a short code | partial | `pod-wormhole` and the relay mailbox exist; the code is not wired to the peer identity |
| Replication across a partition | absent | the model and the merge rule |

#### Rules

- SERIAL-1: AX.25 with KISS framing, 256-byte frames, a window of 1 to 7 with acknowledgements. The line itself is already open: `pipe serial:PATH[,BAUD]` sets raw 8N1 and restores the settings at each end. Add the same kind to `--carrier`, do not write a second serial implementation.
- SERIAL-2: the retransmission timeout derives from the baud rate. At 1,200 baud a 256-byte frame takes 2.1 s, so a 200 ms floor sits below one frame time and is wrong.
- SERIAL-3: refuse a frame larger than the carrier's maximum with a named error, not by fragmenting silently.
- HF-1: treat an ARDOP or VARA modem as a serial device with an ASCII command set. The ranges are 300 bit/s to 3,000 bit/s.
- HF-2: the modem has its own ARQ. Two ARQs in series stack their retransmissions, so the session's ARQ is configured to defer to the modem's.
- HF-3: a half-duplex link has a busy signal and a turnaround delay. Model both, or the session will transmit into a receiving modem.
- HF-4: the degraded-link profile is mandatory on this carrier.
- SPOOL-1: a spool entry carries a checksum, a sequence and the sender's identity, and it is consumed only after the receiver takes it.
- SPOOL-2: a spool is a directory of files, not a database. It must be repairable by hand.
- DISC-1: announce over mDNS with a signature. An unsigned announcement is a peer that anyone can claim.
- DISC-2: one radio interface, implemented. A second interface is a second implementation of the same announcement.
- BUNDLE-1: a bundle is self-describing, with a signed manifest, and it supports a delta update. A full download over a 572 bit/s link is not an update.
- SINK-1: one record format for captures, merged by timestamp, with redaction at write time.
- PAIR-1: a four-word single-use code, exchanged through any carrier, including a spool. `pod-wormhole` (4,612 lines) already runs the magic-wormhole rendezvous and `podssh-relay/src/serve/mailbox/` serves it (T-086); the missing part is the binding to `pod-identity`, not the code path.
- PAIR-2: a code that is reused is refused.
- REPL-1: state the model: last-writer-wins per key with a vector clock, or an append-only log with a merge. State what a conflict does.
- REPL-2: the unit of replication is the session state and the peer registry, because those are what a reconnecting host needs.

#### Gate

- A pseudo-terminal pair at 1,200 baud: a shell echo inside 3 s, and a frame above 256 bytes refused with a named error.
- A modem simulator with a busy signal: no transmit during receive, and one retransmission chain rather than two.
- A spool survives a process kill between receive and consume with nothing lost.
- A discovery announcement with a bad signature is ignored.
- A bundle applied as a delta on a host with the previous bundle.
- Two capture files merge with a stable order and no secret in either.
- A code used twice is refused, and a paired key is bound to the identity in `pod-identity` rather than carried beside it.
- Two replicas diverge, then converge, and the conflict is resolved by the stated rule and logged.

#### Relations

#466 supplies the carrier registry these implement. #467 scopes the stream carriers above them. The session framing these use is the lossy-carrier framing in the session work.

## GitHub #500: [DEFERRED] State the host runtime: no open port, classified egress, stated limits

Opened 2026-11-12 by Nemo-010.

A runtime that opens a port nobody asked for is a runtime that is reachable because it exists. A runtime that classifies its egress before it dials is a runtime whose policy is checkable. Both are configuration and plumbing, and both are prerequisites for putting this on a host that someone else owns.

Read at `podnet` 59c0313 and `podcrates` 45b145a on 2026-11-12. `podnet/src/networks.rs` already classifies a spec into six `Kind` variants: `Direct`, `Proxy`, `Hop`, `Relay`, `Netbird` and `Wg`. `podcrates/crates/pod-broker` (4,586 lines) and `pod-netstack` (5,368) hold the policy and the guest paths. `podvm/docs/limits.md` exists and states: "A chroot changes paths. It does not provide process or network isolation." `podfs`, `podlog`, `podstat`, `podinit`, `podnode`, `podforge`, `podci` and `podrepl` are description-only repositories with no source.

#### Entries

| Capability | Status | What is missing |
| --- | --- | --- |
| Filesystem expectations stated | absent | the five properties and the mount probe |
| Crash logs safe to send | absent | the hook, the redaction, the bound |
| Measurement from outside | absent | the observer and the file |
| No inbound port by default | absent | the default and the enrolment |
| A forge without one host | absent | the content-addressed mirror |
| CI under an egress policy | absent | the declaration and the refusal |
| A runtime that states its limits | partial | the stated limits beside the existing page |
| Proxy-only egress as a tested invariant | partial | the test |
| A status page derived from the tree | absent | the generation and the staleness check |

#### Rules

- FS-1: five properties differ: timestamps, atomic rename, hard links, permission bits, case sensitivity. State each as required or emulated.
- FS-2: a FAT volume has a two-second timestamp granularity and no permission bits, which breaks a sequence ordered by timestamp. Handle it in the probe, not mid-transfer.
- FS-3: the probe runs at mount time and the fallback is documented.
- LOG-1: a panic hook writes the backtrace, the component versions and the last N events.
- LOG-2: redaction happens at write time, and the marker is verifiable: a test searches the output for seeded values.
- LOG-3: the log is bounded and rotated, because a panic loop fills a disk.
- STAT-1: measure throughput, latency and loss from a peer that timestamps, not from the sender's counter.
- STAT-2: a disagreement between the two is reported as a disagreement, not averaged.
- STAT-3: the output is a capture file, so a regression is a diff.
- INIT-1: the default configuration listens on nothing.
- INIT-2: enrolment issues an identity, registers the node with a relay and writes an allow list. All three or none.
- INIT-3: a generated configuration is valid against the schema the generator emits, and the node refuses a configuration that listens, with a named error.
- FORGE-1: a repository identity is a key, not a hostname. Clone from a seed with no forge account, and the checkout hash matches.
- FORGE-2: a content-addressed mirror is verified by hash, so a mirror operator cannot alter the content.
- CI-1: a pipeline declares its egress needs: hosts, ports and whether it needs UDP. The runner refuses before the first step when the host's policy does not cover them.
- VM-1: keep the existing caveat and add the network surface to it. `podvm/docs/limits.md` already says a chroot changes paths and does not isolate.
- VM-2: the runtime's network surface is the egress broker, not the guest.
- VM-3: the page names every non-isolation, and the integration test runs in the network-lab environment.
- RUN-1: the runner has one egress path, a proxy. Add the test that fails when anything opens a socket directly.
- EGRESS-1: classify the destination before dialling: direct, proxy, relay, refuse or queue, using the existing six `Kind` variants. Add one only if a class cannot be expressed.
- EGRESS-2: the classification inputs are the destination, the resolved address family, the port and the deployment's policy.
- EGRESS-3: a refusal carries a reason, not a timeout, so a policy decision is distinguishable from a network one.
- EGRESS-4: the classification belongs where the policy is, not where the socket is. A guest does not decide its own egress class.
- EGRESS-5: `pod rules check` names a rule shadowed by an earlier rule, and raises an error when the shadowed rule is unreachable.
- PAGE-1: the status table is generated from the tree: repository, version, last commit and state.
- PAGE-2: a stale entry fails the check.

#### Gate

- A FAT image and a read-only image: each works with the documented fallback or refuses at mount with a named reason.
- A deliberately panicking test produces a log with a backtrace and no secret, proven by searching for seeded values.
- A run at 1 Kbit/s reports the rate within 5% of an external timestamped measurement; a disagreement with the session's own counter is reported as a disagreement.
- A generated configuration lists no listener; a hand-edited one that adds a listener is refused with a named error; a failed enrolment leaves no partial state.
- Clone from a seed with no account, with a matching hash.
- A pipeline declaring a host the policy forbids fails before its first step with the host named.
- The runner test fails when a direct socket is opened.
- A destination matching no rule is refused with a reason before a connect attempt; `pod rules check` names shadowed rules.
- A stale entry in the generated page fails the check.

#### Relations

#433 asks for the shadowed-rule warning that the egress check extends. #466 supplies the identity enrolment that the no-inbound default depends on. #373 asks for attributable probes, which is the same discipline as STAT-2. The runtime limits page predates this work and should not be duplicated.

## GitHub #501: [DEFERRED] Build the verification harness: classifier, exit codes, conformance matrix

Opened 2026-11-12 by Nemo-010.

Every claim in this set is measured by something. Without a harness, a classifier model and an exit-code contract, the measurements are one person's run on one day, and nobody can repeat them.

Read at `podcrates` 45b145a, `podssh` ef3666f and `podnet` 59c0313 on 2026-11-12. `podcrates/crates/pod-e2e` (5,165 lines) runs a session end to end and `pod-testkit` (819 lines) holds fixtures. `pod-hostprobe` (8,175 lines) already returns three values for a capability check. No source file holds a traffic classifier, a middlebox simulator or a conformance matrix. Exit codes are used across the toolkit but are not one constant, and no test invokes a binary and reads its status.

#### Entries

| Capability | Status | What is missing |
| --- | --- | --- |
| Classifier harness in CI | absent | the classifiers and the job |
| Exit codes as one contract | partial | the constant, the help text, the tests |
| Claims with a label and a date | partial | the check |
| Relocatable conformance archive | absent | the matrix and the artefact |

#### Rules

- DPI-1: three classifiers, in increasing cost: a field matcher (SNI, ALPN, the JA3 and JA4 hashes, record sizes), a flow-statistics classifier over size and timing distributions, and a first-N-bytes classifier.
- DPI-2: the middlebox simulator sits beside them, so a desync strategy and a classifier are tested against one path.
- DPI-3: the harness runs the benign traffic too and reports a false-positive rate on the cover site's own traffic. A classifier that flags everything is not a classifier.
- DPI-4: the output is a table: road, classifier and score, with the date. That table is the only honest statement of how visible a road is.
- DPI-5: the harness finishes in CI in under 10 minutes, and a deliberately unpadded road scores worse than a padded one.
- EXIT-1: one constant in `pod-core` holds the codes: 64 unknown or malformed name, carrier or query key; 66 missing key or identity file; 69 carrier or listener disabled by policy; 71 identity file permissions wrong; 77 authentication failed or wrong peer; 78 protocol or suite mismatch.
- EXIT-2: each binary prints the table in `--help`, generated from the constant, and the same table is in `docs/`.
- EXIT-3: a script that retries on 77 is making a different decision from one that retries on 78, so the codes stay distinct.
- EXIT-4: a test per code, per binary, invokes the binary and reads the status directly, never through a pipe.
- CLAIM-1: four labels: measured here, read from source, reported by a third party, inferred.
- CLAIM-2: a number carries its conditions: which host, which version, which input, how many runs.
- CLAIM-3: a claim whose conditions are not recorded is removed rather than softened.
- CLAIM-4: a documentation check fails on a bare comparative ("faster", "more secure", "robust") without a number and a date nearby.
- ARCH-1: the matrix is carriers by profiles: every carrier in the permitted-service and datagram set, plus the stream carriers, against three deployment profiles. A profile that needs a real middlebox is covered by the simulator instead.
- ARCH-2: two-process tests use unix sockets, which need no privilege and no network namespace. A test that needs two namespaces is a different test and is marked as such.
- ARCH-3: the archive is zipped and self-contained: fixtures, the classifier models, the middlebox simulator and the expected scores. A harness that needs the repository is not relocatable.
- ARCH-4: the harness reports a table, not a pass count. A pass count hides a row that never ran.

#### Gate

- The harness runs in CI in under 10 minutes and prints the road-by-classifier table with the date.
- A deliberately unpadded road scores worse than a padded one.
- The benign false-positive rate is printed and bounded.
- A test per exit code per binary, invoking the binary and reading the status directly.
- The documentation check fails on a bare comparative and passes on a claim with a number and a date.
- Unzip the archive on a host with no root and no toolchain, run it and get the matrix; a carrier that cannot run there reports a named skip, not a pass.

#### Relations

#474 and #475 scope the C ABI and the portable build subset, which the archive depends on if it ships a binary. #89 asks for a test guide for a host with limited networking; the relocatable archive is that guide's executable form. #84 is the CI tracker for this class of job.
