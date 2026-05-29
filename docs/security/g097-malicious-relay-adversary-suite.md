# G097 malicious relay adversary suite

G097 adds executable adversary coverage for relay behavior that the threat model explicitly allows: relays may read visible bytes, corrupt bytes, replay frames, drop packets, reorder packets, and churn endpoints. The required product invariant is that these actions do not expose plaintext/key material and that receivers or overlay policy reject or recover from the active cases.

## Adversary cases

| Case | Harness evidence | Pass condition |
| --- | --- | --- |
| Passive read | `malicious_relay_adversary_smoke` inspects `RelayProtectedEnvelope::visible_bytes` for a protected media frame. | Visible relay bytes do not contain plaintext, MLS epoch secret, SFrame key, or content-key sentinels. |
| Tamper | The suite flips relay-visible ciphertext and opens through `SFrameReceiver`. | Receiver returns authentication failure. |
| Replay | The suite opens a protected frame once, then submits the same KID/counter/ciphertext again. | Receiver returns replay failure. |
| Drop | The suite models a dropped packet with `RedeliveryTracker::request_redelivery`. | Alternate relays are requested up to the fanout cap and extra relay requests are rejected. |
| Reorder | The suite accepts counters out of order inside the replay window. | Out-of-order first delivery succeeds; duplicate and stale repeats fail. |
| Endpoint churn | The suite exercises planned churn damping and hard-failure failover with `OverlayManager`. | Planned reparenting is rate-limited, while hard-failure recovery can bypass the planned-change delay and route through a backup relay. |

## Test entry points

- `cargo test -p discrypt-multinode-harness malicious_relay_adversary_smoke_covers_passive_active_and_churn_cases --quiet`
- `cargo test -p discrypt-relay-overlay --quiet`
- `cargo test -p discrypt-media sframe --quiet`
- `npm --prefix apps/ui run test:malicious-relay-g097`

## Scope boundary

This suite tests the relay adversary behavior at the Rust relay/media/overlay harness boundary. It does not replace the later full production E2E gate, which must repeat the malicious relay cases with live transport processes, packet capture artifacts, and release-dashboard sign-off.
