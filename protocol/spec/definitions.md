# Definitions, version ordering, and compatibility

A game, a loader, and a runtime are signed objects like everything else. They
are what make compatibility claims mean something: a release points at these
IDs instead of a free-text name, so a lookalike name can never be silently
matched.

## Definitions

- A game definition (`game-def`) fixes the display name, how versions are
  written, the ordering scheme, the loader authorities allowed to publish for
  it, its category and tag vocabulary, and the adapter identifiers used to read
  metadata and place files.
- A loader definition (`loader-def`) names a loader family, the game it targets,
  its ordering scheme, an optional bootstrap artifact, and the loaders whose
  artifacts it may accept.
- A runtime definition (`runtime-def`) names a runtime such as Java, its kind,
  and its ordering scheme.

A definition object carries the detail. The object ID and the authorized-kind
set live in the corresponding genesis, which is a separate object of kind
`genesis`. Editing a definition is a new signed revision, never a mutation of
the old one.

## Loader objects carry a `type` discriminant

A loader publishes three different shapes under the one object kind
`loader-def`: the loader definition, a loader release, and a loader acceptance
mapping. They are told apart by a `type` key:

| `type` | Shape |
| --- | --- |
| `definition` | loader family, game, ordering, bootstrap |
| `release` | one loader version and the game versions it supports |
| `mapping` | a directional acceptance of another loader's artifacts |

The field is explicit because a decoder must know which shape to expect before
it can validate unknown-key rejection. `type` is a plain text value, and an
unrecognized value is a hard error, not a fallback. This is the one wire
decision the design text left open; it is recorded here so a second
implementation can match it.

## Version ordering schemes

A game or loader declares exactly one ordering scheme:

- `semver` — dotted numeric versions with optional prerelease and build
  metadata. Comparison follows SemVer precedence: release outranks prerelease,
  and numeric prerelease identifiers outrank text ones.
- `ordered-list` — the game supplies an explicit ascending list of known
  versions. A version absent from it is unknown, never guessed.
- `calendar` — date-like identifiers compared by the declared list order, not
  by parsing a date at evaluation time.
- `opaque` — no ordering. Only equality is meaningful.

## Predicate evaluation

A predicate names a scheme and a list of values. Evaluation never guesses:

- An unknown scheme is `unknown`.
- A scheme that does not match the catalog's ordering scheme is `unknown`
  for range schemes. `exact` and `set` compare identifiers bytewise and work
  against any catalog.
- `any` matches every declared version. It is explicit and never implied.
- `semver` values are comparators (`<`, `<=`, `>`, `>=`, `=`) or bare
  versions. Multiple comparators are ANDed. A bare version with fewer than
  three components is a prefix range, which is what makes `1.20` match every
  `1.20.x`.
- `ordered-list` and `calendar` values are identifiers or ranges. `x..y` is
  exclusive of `y`, `x..=y` includes it. Multiple values are ORed. A version
  that is not in the declared list is `unknown`.

The outcome of a predicate is one of `satisfied`, `not-satisfied`, or
`unknown`. A consumer that cannot resolve the scheme or the version reports
`unknown` and must not treat it as compatible.

## Loader acceptance is directional

A mapping says the accepting loader may run artifacts declared for the accepted
loader, in one direction only. Nothing is inferred from names, shared game
versions, or ancestry, and a mapping that references another mapping is not
followed: there is no chain resolution. The qualification (`native`, `most`,
`experimental`, `untested`) is carried through to the UI, and auto-selection
through a mapping is off unless the operator explicitly enables it.
