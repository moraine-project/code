# Search responses

Search results are JSON, not signed objects. A directory builds them from
validated signed records plus its own local policy, and the index is
disposable: it can be rebuilt at any time and never becomes an authority.

## The portable shape

A response carries the protocol version, the query that produced it, an array
of results, an optional opaque `next_cursor`, and an optional total estimate.
The query records the text, game, loader, category, tag, game version, sort,
cursor, and limit.

A result carries the project and game IDs, display name, summary, optional
icon and release IDs, a listing state, the source instance, annotations, and
an optional instance popularity.

Mapping a project ID to a result and a result to its source is what makes the
shape portable across directories.

## Listing states

`listed`, `unlisted`, `quarantined`, `blocked`, `withdrawn`, `unavailable`.
A directory decides these locally. A release can be listed by one directory
and blocked by another at the same time, and both decisions are legitimate.

## Instance popularity

Popularity is a count of that instance's own observed downloads and follows,
windowed (30 days) and labeled. It is never federated, summed, or averaged
across instances, because no shared user identity exists to deduplicate it.
When results from several instances are merged, popularity stays attached to
its source listing.

## Ranking

An instance must publish which ranking inputs it uses — text relevance,
recency, instance-local popularity, a policy boost — without publishing its
weights or algorithm. Rank is an opinion, not a fact, and never a safety
signal. The published input list must not be silently contradicted by the
implementation.

Impersonation detection belongs here: a project whose display name or handle
closely matches a well-known project in the same game while its stable ID
differs is flagged and shown beside the original, not quietly reordered.

## Merging

Merging is by `project_id`, never by title. One project ID yields one merged
result; every source keeps its own listing state, annotations, and popularity.
Conflicting states are shown as conflicting attributed decisions rather than
resolved by picking a winner. Duplicate listings for one ID collapse into one
entry with several sources, and names that look identical but have different
IDs stay separate with their publisher context.
