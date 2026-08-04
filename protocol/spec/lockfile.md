# Dependency resolution and lockfiles

Resolution turns a request and a set of candidate releases into one pinned set.
It is deterministic: the same inputs and candidates produce the same lockfile,
and no step depends on network order or wall-clock time.

## Inputs

- the game ID and game version;
- an optional loader ID and version;
- an optional runtime ID and version;
- the side (`client`, `server`, or `both`);
- a root project and a predicate on its version;
- the candidate releases, each with its payload, compatibility, and
  dependencies.

## The algorithm

Start from the root project and pick a release that matches the request, in
this order:

1. its compatibility includes an entry whose game-version predicate is
   satisfied, whose side matches, whose loader matches the request (including
   when the request has none), and whose runtime predicate is satisfied;
2. its version satisfies the requested predicate.

Candidates are tried newest-first. From the chosen release, each **required**
project dependency becomes a new constraint, with the game ID checked first so
a dependency can never resolve across games. Optional, embedded, and
recommended dependencies are not auto-resolved. A required dependency of kind
`incompatible` is reported rather than selected. Loader and runtime dependencies
must be satisfied by the request itself; they never download anything.

A predicate whose scheme is unknown, or whose ordered-list version is absent,
never counts as satisfied. Unknown is not compatible.

The solver backtracks: if a choice leads to a branch where no release can
satisfy a constraint, it tries the next candidate. A cycle is reported as an
error rather than silently accepted. A constraint that no candidate can satisfy
is reported with the project and the reason.

## Lockfile

The result is a lockfile: the game, loader, runtime, and side, plus one entry
per resolved project with its exact release ID, version, primary artifact
digest and size, and its dependency edges. `trust_policy_version` records which
verification policy produced it.

A lockfile is a client-specific, unsigned artifact. It is not a publisher
object and never substitutes for the signed records it points at: a resolver
consumer re-fetches and verifies those records, and a lockfile only records
which ones it chose. A mirror URL is a retrievable hint, never the identity of
a locked file.

A launcher builds a lockfile from a home by walking the root project's required
dependency closure through that home's feed. Before a project's entries are
read, its genesis is fetched and verified against its own roots, and the
delegated signing keys are collected from delegation entries that verify
against that genesis. A home response is capped at 16 MiB, so a home cannot
exhaust the launcher by serving an oversized object or feed page. Every release
object is then verified against the root or
against a key the project delegated for releases, and its bytes must hash to
the ID the feed named. A home that serves a release the project did not
authorize, or object bytes that do not match the advertised ID, is refused
rather than resolved. The lockfile records the verified IDs, and the installer
re-checks artifact bytes against their digests when it installs.

A lockfile also records the feed head it saw per project. Passing a previous
lockfile when resolving again makes a home whose head is lower than the
recorded one a hard error, so a home cannot walk a launcher back to an earlier
view by serving an old feed. Without a previous lockfile there is nothing to
compare against.
