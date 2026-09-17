# Definitions

Readable authoring files for games, loaders, and runtimes, compiled to signed
records. This directory is the source of truth for the *content* of a
definition; it is not the identity.

```sh
cargo run -p moraine-publish -- keygen --key definitions.key
cargo run -p moraine-publish -- define --key definitions.key --dir definitions/minecraft --out data/definitions
```

That writes signed objects to `data/definitions`, which the server loads on
startup. To import them into a **running** instance in one command, point the
same command at a home:

```sh
cargo run -p moraine-publish -- define --key definitions.key \
  --dir definitions/minecraft --out data/definitions \
  --home https://your-instance
```

Compiling mints the identities once and writes them to `--out`; importing is a
separate step that reads that directory, so re-importing is idempotent and never
changes the IDs:

```sh
cargo run -p moraine-publish -- define --out data/definitions --home https://your-instance
```

If you do not want the whole default set, delete the files you do not want from
`definitions/minecraft/` (for example, drop loaders you will not support) and
compile again. Files refer to each other by `name`, so the compiler resolves the
remaining IDs for you; you never paste an ID into a TOML. To attach a new
version to an identity you already published, set `revision_of` instead of
minting a new one.

## Identity is not a name

A game or loader ID is the digest of its signed genesis. Compiling the same TOML
with a different key produces a *different* identity, so two instances that each
compile this directory do **not** agree that their "Minecraft" is the same game,
and releases published against one will not match the other.

There is no global registry of games. To agree, instances must share the same
signed genesis, which happens one of two ways:

- **One authority publishes, others sync.** A home authors the definition once
  and serves it; other instances pull it with
  `POST /v1/federation/sync-definition` (or the CLI and settings UI) and store it
  under the same ID. This is what makes a shared "Minecraft" possible.
- **Everyone authors their own.** Fine for a private or unreleased game, but the
  identities differ, and the instance labels them local.

## Adding a version

A definition's identity never changes, so a new game version is a **revision**:
set `revision_of` to the existing ID and compile. The catalogue is append-only.
See `protocol/spec/definitions.md` for the rules.

```toml
kind = "game"
revision_of = "gd:sha256:..."
display_name = "Minecraft"
version_ordering = "ordered-list"
versions = ["1.20", "1.21", "1.22"]
```

## The canonical set

`definitions/minecraft/` is the content. `definitions/canonical/` is that content
compiled to signed objects, so every instance that takes it shares the *same*
game, loader, and runtime IDs, and releases published against one match the
other. `definitions/curated.lock` lists those IDs.

Two ways to take it, both one command:

```sh
# import the shipped signed objects directly; no home required
moraine-publish define --out definitions/canonical --home https://your-instance

# or pull the same set from a home that serves it
moraine-publish sync-definitions --home https://your-instance \
  --from https://the-home --lock definitions/curated.lock \
  --api-key "$MORAINE_API_KEY"
```

Importing the shipped objects is idempotent: the identities are already fixed,
so running it again changes nothing. An instance that does not want this set
compiles its own from the TOML instead and gets its own local IDs.

## Regenerating the canonical set

The authority key is `definitions/canonical.key`; it is git-ignored and not part
of the repository. Whoever holds it authors the next revision, using `revision_of`
to add versions without changing the IDs, then rebuilds:

```sh
moraine-publish define --key definitions/canonical.key \
  --dir definitions/minecraft --out definitions/canonical
moraine-publish lock-definitions --dir definitions/canonical --out definitions/curated.lock
```

Keep that key safe. If it is lost, the set can only be replaced under a new
identity, which breaks the shared IDs.
