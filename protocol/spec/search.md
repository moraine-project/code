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
windowed (30 days) and labeled. A full byte-serving `GET` of an artifact counts
as a download for every project whose signed release references that digest,
and a follow counts from the moment it was made. The window is a rolling 30
days ending today, recorded per day so a later interval can be computed without
rewriting history. It is never federated, summed, or averaged across instances,
because no shared user identity exists to deduplicate it. When results from
several instances are merged, popularity stays attached to its source listing.

## Ranking

An instance must publish which ranking inputs it uses — text relevance,
recency, instance-local popularity, a policy boost — without publishing its
weights or algorithm. Rank is an opinion, not a fact, and never a safety
signal. The published input list must not be silently contradicted by the
implementation.

The sorts this implementation offers are `relevance`, `updated`, `created`,
`name`, and `popularity`. Its relevance input is recency, so `relevance` and `updated` order
identically today; `created` is when this instance first indexed the project,
which is not the same as when the publisher declared it; and `name` is
case-insensitive, and `popularity` orders by the instance-local count below,
highest first. Any other value is rejected with `400` rather than silently
treated as a different order, because an echoed sort that the server did not
honour would contradict the published inputs. Text matching covers the display
name and summary.

The facets this implementation serves are `game`, `loader`, `category`, and
`tag`. A project's loader labels come from the loaders its signed releases
declare, so a loader filter matches a project that has published for it.

Impersonation detection begins here: when two results in one game share a
normalized display name (case and punctuation removed) under different stable
IDs, both carry a `name-collision` annotation telling the reader to compare
IDs rather than names. The comparison runs against the whole index through a stored
normalized name, not just the returned page, so two projects separated by
pagination are still flagged. It is not a check against a registry of
well-known projects.

## Merging

Merging is by `project_id`, never by title. One project ID yields one merged
result; every source keeps its own listing state, annotations, and popularity.
Conflicting states are shown as conflicting attributed decisions rather than
resolved by picking a winner. Duplicate listings for one ID collapse into one
entry with several sources, and names that look identical but have different
IDs stay separate with their publisher context.
