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

A definition may be authored as a readable TOML file and compiled to canonical
bytes before signing, so the input stays reviewable and diffable. The file is
an input convenience and is never the wire format or the signed artifact:
duplicate keys, implicit typing, and ordering differences make a text format
unsuitable for signatures. TOML is used because it has explicit types, rejects
duplicate keys, and has no anchors, so a file has one unambiguous meaning
before compilation. The compiler rejects unknown fields, so a typo cannot
silently change a definition. The repository ships a compiled canonical set under
`definitions/canonical/`, but it does not ship the private authority key or a
recovery mechanism for that key. Import those signed objects, or obtain the
same signed genesis from a home you trust, when shared identities matter;
compiling the TOML with another key creates new identities.

A loader definition may also declare `game_versions`, the game versions the
loader family supports as a whole. In TOML that is `game_versions` with an
optional `game_version_scheme`. With the default `exact` scheme the list is
literal versions; with `ordered-list` (or `calendar`) it can hold ranges such as
`1.14..=26.3`, resolved through the game's version table.

It is the cheap way to record the game-version-to-loader connection: one list
on the loader, instead of a per-loader-version table of which game versions each
build targets. A loader release's own `game_versions` stays authoritative for
that exact version when it is present. The family list is a fallback for game
versions where no per-version record exists, not an override. A loader with no
releases at all is still useful: a mod can name the loader family, and the
family's list answers the game-version question. Naming a loader version in a
request is optional for the same reason.

A game, loader, or runtime definition also carries an optional ordered
`version_catalog`, authored in TOML as `versions`, listing the versions it
recognises in ascending order. Loaders need one as much as games do, because a
loader may number its releases in a scheme that is not SemVer, so a loader
version such as `26.3.0.7-beta` is ordered by the loader's own table.

The catalog is what makes the `ordered-list` and `calendar` ordering schemes
evaluable. A predicate over a range such as `1.19..1.21` walks the table, and a
version absent from the table is unknown rather than guessed at. A definition
with no catalog, or one whose scheme needs no table, still evaluates `exact`,
`set`, `semver`, and `any` predicates. The table is fixed at authoring time, so
a new game version is a new signed revision of the definition; a duplicate or
empty entry is refused.

## Revisions are append-only

A definition's identity is its genesis, and that never changes. What changes is
the current signed revision. Adding a game version, a category, or a tag means
publishing a new revision: the definition gets a new object digest, but the
game or loader ID a client already pinned stays the same, and this instance
serves the newest revision at the same URL. Adding a loader version is even
lighter — it is a separate `loader-def` object of the release shape, so the
loader definition is not republished at all.

Authoring a revision reuses the identity instead of minting a new one. In a
readable definition file, set `revision_of` to the existing ID; the compiler
signs a new definition under that ID and writes it over the file for that
identity, so a definitions directory holds one current definition per game,
loader, or runtime:

```toml
kind = "game"
revision_of = "gd:sha256:..."
display_name = "Minecraft"
version_ordering = "ordered-list"
versions = ["1.20", "1.21", "1.22"]
```

The revision must be signed by the key that owns the identity's genesis, or by a
delegate authorized for that kind. The instance verifies it against the stored
genesis and applies the append-only checks below. A `loader-release` or
`mapping` is a standalone object with its own digest and no identity of its own,
so `revision_of` does not apply to those shapes.

The catalogue is append-only, and the instance enforces it. A revision that
drops a version, category, or tag, that changes the version ordering scheme, or
that names a different identity than the one it is stored under is refused with
`400`. This is the same rollback rule the feed uses: a client that already saw a
version can rely on it staying visible, and cannot be fed a definition that
quietly rewrites what it published.

The same file format authors the other loader shapes: `kind = "loader"` for a
loader definition, and `kind = "mapping"` for an acceptance mapping, naming
`accepting_loader`, `accepted_loader`, `game_id`, an optional `qualification`,
and the `declared_by` source. A mapping file therefore declares the direction
explicitly and cannot be read as a reverse acceptance.

A game definition's `loader_authorities` and `loaders_allowed` are enforced when
a loader definition, release, or acceptance mapping is stored, not merely
declared: a mapping that names a game whose definition this instance holds is
refused if that game forbids loaders or if its authority list is non-empty and
does not name the loader. A game definition this instance does not hold cannot
be checked, so the loader is stored and compatibility with it stays unknown
rather than assumed.

Category and tag identifiers are declared by the game and referenced by ID.
When a game definition with a non-empty category or tag vocabulary is held, a
profile revision naming an identifier outside it is refused, so a facet cannot
be populated by an identifier the game never declared. A game with no declared
vocabulary constrains nothing.

## Local and federated definitions

An instance holds definitions it authored or imported locally, and definitions
it pulled from other homes. Both are signed records stored under their own ID;
the difference is where the instance first saw them.

The definition list routes (`GET /v1/games`, `/v1/loaders`, `/v1/runtimes`) and
the detail routes return a `source_home` field: `null` when the definition is
local, and the home's URL when it was pulled from that home. The distinction is
informational — verification always runs against the definition's genesis — but
it tells an operator which identities their instance serves and where they came
from.

A definition is pulled with `POST /v1/federation/sync-definition`, which fetches
the genesis and the current definition from a home, verifies both, and stores
them under the same ID. Pulling a definition the instance already holds locally
leaves the local record and its source untouched.
`POST /v1/federation/subscribe-definition` does the same and records a
subscription, so the maintenance task refreshes it.

Because identity is the genesis digest, a shared game means a shared signed
genesis, not a shared name. An operator that wants its "Minecraft" to be the
same game as another home's pins that home's definition by ID and pulls it,
rather than compiling the authoring files with its own key.

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
