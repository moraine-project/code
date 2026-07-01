# Canonical encoding, identity, and signature scope

These notes record the decisions the implementation makes. They are the
authority for byte-level questions, so a second implementation can match
Moraine exactly.

## One `gd:` namespace for object IDs

Every object renders as `gd:sha256:<hex>`, whatever its kind. Kinds never get
their own prefix. Two objects of different kinds cannot collide because their
digests are computed over different domain tags:

```
DomainTag(kind) = "GAMEDIST/v1/" || kind || 0x00
object_id       = SHA-256(DomainTag(kind) || payload)
```

The kind is recovered by decoding the payload, not by reading the ID. An
earlier draft used a per-kind letter prefix; it was dropped because the domain
tag already separates kinds and a prefix only invites mistakes.

## Feed references use object identity digests

A feed entry's `object_digest` is the identity digest of the referenced object,
the same value embedded in that object's `gd:sha256:` ID. The previous-entry
link is the identity digest of the prior feed entry. Both are raw 32-byte
digests inside signed bytes, not hex, and not the artifact blob digest.

## Key IDs are raw digests on the wire

```
KeyId = SHA-256(alg_id || public_key)
```

Inside signed bytes a key ID is those 32 raw bytes, never hex text. The
algorithm byte lives in a separate field: `Signature.alg`, or the
Ed25519-only root list of a genesis. The `ed25519:<hex>` form is for display
and logs only.

## The envelope is never signed and never hashed

A signed object is `{ envelope, payload }`. The object ID and every signature
cover `DomainTag(kind) || payload`. Adding or removing a signature changes
neither. That is what lets a genesis carry threshold signatures without the
circularity of a document that would have to sign itself.

## Threshold accounting

- Duplicate key IDs count once, however many signatures carry them.
- The valid, distinct, authorized signatures must meet the declared threshold.
- An envelope with more signatures than the threshold is accepted only when
  every one of them verifies. One bad extra signature invalidates the object.

A key set and its threshold come from the genesis. A delegation can name a
narrower capability, never a wider one.

## Unknown fields are rejected

Each object type carries an explicit allow-list of keys and rejects anything
else. Unknown does not mean ignorable. A signed record means exactly what it
says, and a consumer that does not understand a field must not silently treat
it as absent.

## Canonical form

Signed bytes are deterministic CBOR:

- Map keys sort by encoded length first, then bytewise.
- Integers and lengths use the smallest form that fits.
- No indefinite-length items, no floats, no CBOR tags.
- Duplicate map keys are rejected, and two text keys that differ only by
  Unicode normalization are rejected as a collision.
- An optional field is omitted when absent. Omitted and explicit `null` are
  different, and `null` is accepted only where a field allows it.
- Digests are raw bytes inside signed objects; hex is a display form.
- Timestamps are integer seconds since the Unix epoch.

The decoder validates all of this. It rejects bytes that are not already
canonical rather than normalizing them, so a signature can never depend on how
lenient a parser happened to be.

## Primary artifact rule

Exactly one artifact is primary per compatibility class. The implementation
groups artifacts by their `os_predicate` and `arch_predicate` (sorted, absent
treated as empty) and requires exactly one primary in each group. A group with
zero or several primaries is rejected as `primary-artifact-ambiguous`. When
compatibility entries gain per-loader grouping, this rule moves to that
grouping.

## Critical extensions

`critical_extensions` names extension keys a consumer must understand. Version
1 defines none, so any non-empty list is rejected as
`unknown-critical-extension`. Unknown-critical means reject, never ignore.

## Predicates

A predicate is structurally accepted whatever its scheme, so objects can be
stored and forwarded. Evaluation is separate. A scheme the consumer does not
know, or a game or loader ordering the consumer cannot resolve, yields
`unknown`, never `satisfied`. `exact` and `set` compare identifiers bytewise.
`semver`, `ordered-list`, and `calendar` currently evaluate only exact
membership; range comparison arrives with the game and loader definition
objects.

## Ownership transfers need two signatures

A transfer requires two valid signatures over the same payload. A new owner
reference carries no key, so the implementation requires two distinct
signatures, both authorized under the current root set. A project that wants a
new owner to sign prepares a delegation for that key first. One signature is
rejected as `transfer-needs-two-signatures`. The property that matters is
kept: no single signature moves a project.

## Not covered yet

Search responses are JSON rather than canonical CBOR and live beside the
corpus. Notifications, webhooks, and moderation records are covered by unit
tests. None of this changes any byte rule above.
