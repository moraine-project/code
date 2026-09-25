# Migration, recovery, locations, and attestations

These objects share the delegation and attestation families, so they carry a
discriminant rather than getting their own object kind.

## Delegation-purpose objects

Kind `delegation` has a `purpose` key with four values:

| `purpose` | Meaning |
| --- | --- |
| `key` | delegate release or profile signing to a key |
| `ownership-transfer` | move project ownership between a user and an org |
| `migration` | point the project at a new home |
| `recovery` | replace compromised or lost root keys |

A decoder reads `purpose` first and then validates the variant, so a field
from one purpose is never accepted in another. An `ownership-transfer` carries
owner references whose `key_id` fields identify the required signatures; the
transfer must be signed by both named owner keys and its `project_id` must match
the root subject.
Migration and recovery records use the same subject and delegation-authority
checks.

## Migration

A migration record names the old and new home and the feed sequence at which
the move takes effect. It needs three things before it is valid:

1. the publisher root signature, which authorizes the move;
2. the old home's authorized publishing identity;
3. the new home.

A record with only one or two signatures is rejected as
`cross-signature-required`. The old home may serve a redirect as a courtesy,
but the signed record is what carries authority. A follower verifies the
publisher signature against the old trust root first, then checks that the new
home advertises the same feed head at or before the cutover sequence.

## Recovery

Recovery is the emergency path and is deliberately bounded. A recovery event
names the compromised key IDs, the sequence the recovery takes effect from,
the replacement root keys, and the affected release window. It is signed by
the recovery key set, and the replacement root set must meet the genesis
threshold counted over recovery keys. A below-threshold event is
`threshold-not-met`.

Recovery never erases history. Releases signed before the event remain
addressable, and a conflicting unresolved recovery claim must be surfaced
rather than silently resolved.

## Locations versus mirror commitments

Two things are kept apart on purpose.

- A **location record** is publisher-signed (`kind: release`, `type:
  location`). It names URLs where an artifact digest may be fetched and
  whether each is an origin, a mirror, or an external link.
- A **mirror commitment** is the mirror's own signed statement (`kind:
  attestation`, `type: mirror-commitment`) that it holds a digest. It is
  evidence of availability and nothing more. A commitment proves the bytes
  were stored once, never that they will be kept.

A consumer fetches from any location or observed hint because it verifies the
digest, but the decision to distribute still belongs to the operator and the
publisher. An endpoint is a retrievable hint, never the identity of the file.

## Attestations

An attestation (`kind: attestation`, `type: attestation`) is a signed
statement about an exact artifact digest. It carries a subject, an enumerated
kind (`build-provenance`, `review`, `scanner-result`, `sbom`,
`compatibility-test`), a media type, and either a body digest or a small
inline body. Both at once is rejected.

An attestation is evidence, never a guarantee. A verifier that does not
recognize an enumerated kind rejects it rather than ignoring it, following the
same fail-closed rule as an unknown critical field.
