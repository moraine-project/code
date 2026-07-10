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
  scoped revocable API keys), organizations with roles and nested teams,
  admission review with a `review`/`open` setting, publisher withdrawals,
  provider advisories and mirror commitments with pinned keys, follows with
  local notifications, and a pull-based federation sync that fetches, verifies,
  and indexes a remote home's feed.

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

Check a downloaded file against a signed release, offline, with an explicit
trust root:

```sh
cargo run -p moraine-verify -- artifact --release release.cbor --file mod.jar --root <hex>
```

That verifies the release signature, hashes the file, and confirms the bytes
match a recorded artifact. It prints `status: verified` or fails.

Run the registry:

```sh
cargo run -p moraine-server -- --data-dir ./data --bind 127.0.0.1:8080 --publishing review
curl http://127.0.0.1:8080/.well-known/mod-registry
```

To serve the built website from the same origin as the API, point the server at
it. That is what browser authentication needs, because sessions are cookies and
login is a `POST` the server does not offer to other origins:

```sh
cd web && npm run build:static && cd ..
cargo run -p moraine-server -- --data-dir ./data --web-dir web/build
```

`--web-dir` serves files and falls back to `index.html` for client-side routes;
API paths still get API responses.

Publish a release with the CLI:

```sh
cargo run -p moraine-publish -- keygen --key publisher.key
cargo run -p moraine-publish -- init --key publisher.key --home http://127.0.0.1:8080
cargo run -p moraine-publish -- upload --home http://127.0.0.1:8080 --file mod.jar
cargo run -p moraine-publish -- release --key publisher.key --home http://127.0.0.1:8080 \
  --project <project-id> --game <game-id> --game-version 1.20.1 \
  --version 1.2.3 --file mod.jar
cargo run -p moraine-publish -- publish --key publisher.key --home http://127.0.0.1:8080 \
  --project <project-id> --object <release-id>
```

A project has a display name once you publish a profile:

```sh
cargo run -p moraine-publish -- profile --key publisher.key --home http://127.0.0.1:8080 \
  --project <project-id> --game <game-id> --name "Minimap" --summary "A map overlay" --tag client
cargo run -p moraine-publish -- publish --key publisher.key --home http://127.0.0.1:8080 \
  --project <project-id> --object <profile-id> --kind profile-updated
```

As a provider, pin a key on a home and publish an advisory:

```sh
cargo run -p moraine-publish -- provider --key scanner.key --home http://127.0.0.1:8080 \
  --id my-scanner --api-key $MORAINE_API_KEY
cargo run -p moraine-publish -- advisory --key scanner.key --home http://127.0.0.1:8080 \
  --provider my-scanner --project <project-id> --game <game-id> \
  --digest sha256:<hex> --severity high --category malware --block
```

An advisory is attributed evidence, not a takedown: it is shown on the release
page and in the advisories API, and it never changes the signed record. Only
`malware` at `high` or `critical` may block promotion.

Withdraw a release without rewriting it:

```sh
cargo run -p moraine-publish -- withdraw --key publisher.key --home http://127.0.0.1:8080 \
  --project <project-id> --release <release-id> --reason compromise --note "key leak"
cargo run -p moraine-publish -- publish --key publisher.key --home http://127.0.0.1:8080 \
  --project <project-id> --object <withdrawal-id> --kind release-withdrawn
```

Transfer ownership with both sides signing:

```sh
cargo run -p moraine-publish -- transfer --key old-owner.key --cosign-key new-owner.key \
  --home http://127.0.0.1:8080 --project <project-id> --from user:<id> --to org:<id>
# then publish or submit the transfer entry:
cargo run -p moraine-publish -- publish --key old-owner.key --home http://127.0.0.1:8080 \
  --project <project-id> --object <transfer-id> --kind ownership-transferred
```

The transfer object only becomes effective when its feed entry is accepted, so
it reaches directories through sync like any other feed fact.

`publish` and `submit` take `--kind` and default to `release-published`. The
key is your project's root. Keep it safe: losing it means losing the project
identity. `init` signs a fresh project, `upload` stores the artifact bytes at
the home, `release` signs and stores a release object, `profile` signs and
stores display metadata, and `publish` appends the feed entry that makes an
object visible.

To go through admission review instead of publishing directly, use `submit`
with an API key that carries `submissions:write`:

```sh
cargo run -p moraine-publish -- submit --key publisher.key --home http://127.0.0.1:8080 \
  --project <project-id> --object <release-id> --api-key <token>
```

Under `open` publishing it is auto-accepted; under `review` it waits in the
queue, and `MORAINE_API_KEY` can supply the token instead of the flag. `publish`
refuses early on a `review` home and points at `submit` rather than failing
with a bare conflict.

Run the website against it:

```sh
cd web
npm install
npm run dev
```

`npm run build:static` produces a static site. `npm run build:cloudflare`
produces a Worker build from the same source. The site reads
`PUBLIC_MORAINE_REGISTRY` for its default home. The server answers read
requests with permissive CORS and no credentials, so a static site on another
origin can resolve projects and fetch blobs. Writes are not offered
cross-origin.

A release page can hash a file you already downloaded and check it against the
release's artifact digests. That is a byte check in the browser; the verifier
CLI is still what checks the publisher's signature. A release marked withdrawn
shows a warning at the top of its page, and the feed names the withdrawal.

The account console (`/account`) and review queue (`/review`) use session
cookies, so they work when the site is served by the registry under one origin
via `--web-dir`. From a separate origin only the public read pages work.

The publish console (`/publish`) uploads an artifact to the home and prints the
exact `moraine-publish` command to run locally, filled in with what you entered
and the origin you are on. Signing stays on your machine; the browser never
holds a release key.

## Layout

```
codec/          deterministic CBOR canonical profile
crypto/         keys, signatures, domain separation, object IDs
model/          typed, validated protocol objects
verify/         moraine-verify CLI and vector runner
publish/        moraine-publish CLI that signs and publishes releases
server/         moraine-server registry, directory, and worker binary
web/            SvelteKit website with static and Cloudflare build targets
protocol/
  spec/         notes that pin implementation decisions
  vectors/      the test-vector corpus
```

## License

Not settled yet. The intent is AGPL-3.0-or-later for the server and permissive
terms for the protocol libraries, so independent clients stay welcome. The
decision lands before the first release.
