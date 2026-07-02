# Moraine

A federated registry for games and their mods. Each project lives at a home its
publisher controls, releases are signed and immutable, and independent
directories and mirrors can index and serve them without owning your project.

Nothing here is finished. This repository holds the protocol implementation and
the tools that check it.

## What works right now

- **`codec`** — the deterministic CBOR profile from the protocol spec. It
  enforces minimal integer widths, definite lengths, sorted map keys, no
  floats, no tags, and duplicate-key rejection. The decoder rejects anything
  that is not already canonical, so a signature can never depend on parser
  leniency.
- **`crypto`** — Ed25519 keys, key IDs, domain tags, and object IDs.
- **`model`** — typed protocol objects: project genesis, key delegation and
  ownership transfer, release payloads, profile revisions, and feed entries.
  Each type validates itself on decode and refuses unknown fields.
- **`verify`** — the `moraine-verify` CLI plus the protocol test-vector corpus.
- **`server`** — an Axum registry with a SQLite metadata store. It serves
  capability discovery, health, digest-addressed blobs with range support, and
  a first signed-object and feed API: import a genesis, store verified
  objects, append feed entries with continuity checks, and read them back. It
  also has accounts (Argon2id passwords, server-side sessions with CSRF, and
  scoped revocable API keys), admission review with a `review`/`open` setting,
  and a pull-based federation sync that fetches, verifies, and indexes a
  remote home's feed.

## Try it

```sh
cargo run -p moraine-verify -- vectors
cargo run -p moraine-verify -- gen-vectors
cargo run -p moraine-verify -- hash Cargo.toml
cargo run -p moraine-verify -- key-id --public <ed25519-public-key-hex>
```

Verify a signed object you already have:

```sh
cargo run -p moraine-verify -- object --kind release --file release.cbor --root <hex> --threshold 1
```

Run the registry:

```sh
cargo run -p moraine-server -- --data-dir ./data --bind 127.0.0.1:8080 --publishing review
curl http://127.0.0.1:8080/.well-known/mod-registry
```

## Layout

```
codec/          deterministic CBOR canonical profile
crypto/         keys, signatures, domain separation, object IDs
model/          typed, validated protocol objects
verify/         moraine-verify CLI and vector runner
server/         moraine-server registry, directory, and worker binary
protocol/
  spec/         notes that pin implementation decisions
  vectors/      the test-vector corpus
```

## License

Not settled yet. The intent is AGPL-3.0-or-later for the server and permissive
terms for the protocol libraries, so independent clients stay welcome. The
decision lands before the first release.
