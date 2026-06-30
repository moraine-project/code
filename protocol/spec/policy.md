# Advisories, moderation, and events

## Advisories

An advisory is a signed statement by a provider about one artifact digest or a
bounded version range in one project. It names the provider, the affected
project and game, a severity, a category, a taxonomy version, an evidence
reference, publication and optional expiry times, and an optional retraction
time.

Severity is `info`, `low`, `moderate`, `high`, or `critical`. Category is
`malware`, `vulnerability`, `known-incompatibility`, `privacy`, `policy`, or
`other`. Both come from a versioned taxonomy, and a consumer that sees a higher
version than it knows must show the finding as unrecognized rather than map it
onto something it understands.

The one blocking rule is deliberately narrow: only `malware` at `high` or
`critical` severity may set `block_promotion`. Every other combination may
reduce ranking or show a warning, but must not silently remove a release. A
block applies to the affected digest or range, never the whole project. A
retraction is another advisory state, and the retracted advisory stays
queryable so caches can learn about it.

An advisory is evidence, never a takedown and never a signature. It does not
alter signed release bytes or revoke a publisher key.

## Moderation

Every listing and review decision carries a reason code from a versioned
taxonomy plus its version. This implementation freezes version 1:

`policy-disallowed`, `malware-suspected`, `malware-confirmed`,
`impersonation`, `trademark-claim`, `rights-complaint`, `spam`,
`fork-detected`, `broken`, `author-request`, `legal-order`, `adult-content`.

A code from a higher version is `Unknown` and is never guessed. A decision
applies at a scope: an instance, a project, a release digest, or an account.

A sanction names an account, a kind (`warning`, `upload-restriction`,
`suspension`), a reason, a scope, a window, and who decided it. A sanction
restricts what an account may do on one instance. It is never a protocol-wide
identity ban, because no global identity exists.

Handles are claimed atomically, unique per instance, and are not a global
namespace. A collision with an existing handle is a `Collision`; a collision
with a protected name is routed to an impersonation or trademark decision
(`Dispute`) rather than silently renaming someone else's project.
Impersonation reports carry a claim kind, an evidence reference, the affected
project or handle, a status, and a decision time.

## Events

A notification and a webhook deliver the same payload: an event id, an event
kind, the project and game, an optional feed sequence, an optional object or
advisory digest, and an issue time. Feed-derived events must carry a feed
sequence; an advisory event must carry the advisory digest.

The event id is the idempotency key. A webhook delivery is signed by the
sending instance, but the receiver treats it as a hint and re-fetches and
verifies the referenced object before acting. An instance can be compromised
and can assert anything in a webhook, so the payload is never trusted data.

Retention defaults: notifications are kept 90 days, webhook delivery records
30 days, and instance-local counts use a rolling 30-day window. Counts are
never federated, because there is no shared user identity to deduplicate them.
