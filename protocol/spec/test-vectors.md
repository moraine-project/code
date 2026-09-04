# Test vectors

The corpus lives in `protocol/vectors/vectors.json`. It is the artifact that
lets two implementations compare verdicts instead of arguing about prose.

## A vector

```json
{
  "name": "release-ambiguous-primary",
  "category": "release",
  "kind": "release",
  "payload_hex": "a...",
  "envelope": { "alg": 1, "signatures": [ { "alg": 1, "key_id": "<hex>", "sig": "<hex>" } ], "key_ids": ["<hex>"] },
  "expected_id": "gd:sha256:...",
  "expected_verdict": "reject",
  "reason_code": "primary-artifact-ambiguous"
}
```

`payload_hex` is the canonical payload bytes, not the full wire object. The
verifier rebuilds the signed object from the payload and the envelope and
checks the same facts a federation consumer would.

Fields beyond the minimum, all optional:

- `verify_kind` — verify the signature over a different domain tag than the
  payload's own kind. This is how cross-object substitution is tested: the
  payload decodes as one kind, the signature was made over another, and the
  verdict is `bad-signature`.
- `trust` — the keys and threshold used to check signatures.
  - `roots` — trusted Ed25519 public keys, hex.
  - `threshold` — how many must sign.
  - `authorized_kinds` — the closed kind set a delegation may grant. Empty
    means every kind.
  - `profile_roots` — keys holding an explicit profile delegation. A profile
    signed by a root key that is not here is rejected as `profile-authority`.
  - `delegated_keys` — keys with some delegation but no profile delegation,
    used to tell `profile-authority` apart from `bad-signature`.
- `prior_feed_hex` — earlier feed entry payloads, in order, so the verifier can
  check sequence continuity and the `previous` link.

A vector with `kind: "canonical"` carries raw bytes in `payload_hex` and no
envelope. It tests the codec directly: minimal widths, key order, duplicate
keys, indefinite lengths, floats, tags, and Unicode normalization collisions.

A vector with `kind: "predicate"` carries no object at all. Its
`predicate_case` holds the ordering scheme, an optional version catalog, the
predicate's scheme and values, a version to test, and the expected outcome
(`satisfied`, `not-satisfied`, or `unknown`). This is how version-order
semantics are pinned without wrapping them in a signed record.

## Categories

1. Canonical encoding
2. Genesis and IDs
3. Delegation
4. Release
5. Feed
6. Profile
7. Cross-object
8. Definitions and version schemes
9. Loader acceptance
10. Migration
11. Locations and mirror commitments
12. Key recovery
13. Moderation and handles
14. Events and webhooks
15. Search response

Categories 1 through 12 and advisories have vectors now, including predicate
evaluation, which is tested directly rather than through a signed object.
Moderation taxonomy behavior and event idempotency are covered by unit tests.
Search results are JSON, not signed objects, so they live beside the corpus:
`protocol/vectors/search.json` holds two directory responses that a test
validates and merges by project ID.

## Regenerating

```sh
cargo run -p moraine-verify -- gen-vectors
cargo run -p moraine-verify -- vectors
python3 interop/verify_vectors.py
```

The generator and the Rust verifier share code, so those two only prove
regression safety. Interoperability needs a second implementation that shares
no code, and `interop/` is one: it decodes the canonical bytes, derives object
IDs, and verifies Ed25519 signatures in Python, then checks every accept and
reject, including the reason code. A corpus that only one implementation passes
is not evidence of anything, so both are expected to agree before a change to
the signed formats is considered done.
