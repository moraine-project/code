# Moraine

A federated registry for games and their mods. Each project lives at a home its
publisher controls, releases are signed and immutable, and independent
directories and mirrors can index and serve them without owning your project.

This is an alpha. The protocol and the server work and are covered by tests,
but nobody outside this repository has run two instances against each other
yet, there has been no external security review, and the desktop launcher is
not built. The website, not a launcher, is the way people use it.

Run an instance locally, publish something, and tell us where it breaks. For a
suspected vulnerability, follow [`SECURITY.md`](./SECURITY.md) rather than
opening a public issue. To build or contribute, read
[`CONTRIBUTING.md`](./CONTRIBUTING.md) and the operator and author guides under
[`docs/`](./docs/).

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
  **`interop`** is an independent Python implementation of the same corpus,
  sharing no code with the Rust one.
- **`launcher`** — resolves a lockfile against a home, then verifies and
  installs it. It reads the install adapter and metadata extractor a game
  definition declares instead of hard-coding a game, and refuses a declared
  adapter or extractor this build does not implement.
- **`server`** — an Axum registry with a SQLite metadata store. It serves
  capability discovery, health, digest-addressed blobs with range support, and
  a first signed-object and feed API: import a genesis, store verified
  objects, append feed entries with continuity checks, and read them back. It
  also has accounts (Argon2id passwords, server-side sessions with CSRF, and
  scoped revocable API keys), organizations with roles and nested teams,
  admission review with a `review`/`open` setting, publisher withdrawals,
  provider advisories and mirror commitments with pinned keys, follows with
  local notifications and signed outbound webhooks, game/loader/runtime
  definition hosting, signed modpack manifests with validated overrides, a
  per-project listing policy for unlisting or blocking, append-only legal,
  takedown, impersonation-claim, and account-sanction records with upload and
  submission enforcement, evidence attestations from pinned providers, key
  recovery that replaces a project's root set and revokes a compromised key,
  cross-signed migration records, subscribe-able signed deny and advisory
  lists, and a pull-based federation sync that fetches, verifies, indexes, and
  fork-checks a remote home's feed while recording the heads it observed so a
  rewritten sequence stays on the record.

## Try it

```sh
cargo run -p moraine-verify -- vectors
cargo run -p moraine-verify -- gen-vectors
cargo run -p moraine-verify -- hash Cargo.toml
cargo run -p moraine-verify -- key-id --public <ed25519-public-key-hex>
```

The corpus is checked by a second implementation written in Python, which uses
its own canonical decoder and its own Ed25519 code rather than calling into the
Rust crates. Two implementations agreeing on every verdict is the interoperability
evidence; a corpus only one implementation passes proves nothing:

```sh
python3 interop/verify_vectors.py
```

Verify a signed object you already have:

```sh
cargo run -p moraine-verify -- object --kind release release.cbor --root <hex> --threshold 1
```

`--kind` accepts `genesis`, `delegation`, `release`, `feed-entry`, `profile`,
`changelog`, `modpack`, `advisory`, `attestation`, `game-def`, `loader-def`,
and `runtime-def`.

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
cd web && pnpm build:static && cd ..
cargo run -p moraine-server -- --data-dir ./data --web-dir web/build
```

`--web-dir` serves files and falls back to `index.html` for client-side routes;
API paths still get API responses.

The store is SQLite by default. Point it at PostgreSQL instead with a
connection URL; the schema is migrated for whichever engine the URL names:

```sh
MORAINE_DATABASE_URL=postgres://user:password@host/moraine \
  cargo run -p moraine-server -- migrate
MORAINE_DATABASE_URL=postgres://user:password@host/moraine \
  cargo run -p moraine-server -- --data-dir ./data
```

Artifact bytes stay on the filesystem by default. To keep them in an
S3-compatible object store instead, set the bucket; Backblaze B2, RustFS, and
MinIO all speak this API, and the endpoint takes the provider's host with
path-style addressing:

```sh
MORAINE_S3_BUCKET=moraine \
MORAINE_S3_ENDPOINT=https://s3.us-west-004.backblazeb2.com \
MORAINE_S3_REGION=us-west-004 \
MORAINE_S3_ACCESS_KEY_ID=<key-id> \
MORAINE_S3_SECRET_ACCESS_KEY=<application-key> \
MORAINE_S3_PREFIX=moraine \
  cargo run -p moraine-server -- --data-dir ./data
```

Uploads still stage on local disk so a partial transfer never reaches the
bucket, and a committed blob is served to clients by streaming it back out of
the object store, including byte ranges. Set `MORAINE_S3_ACCESS_KEY_ID` and
`MORAINE_S3_SECRET_ACCESS_KEY` together; without them the client falls back to
the standard provider chain, which is what an instance role provides. The
prefix namespaces every object so one bucket can hold several instances.

Backup and restore read the SQLite file directly, so use `pg_dump` for a
PostgreSQL database, and the same S3 client or an operator tool for a bucket.
`moraine-server backup` copies committed blobs through the store, so it works
against either backend.

The role does not need to own the database or be a superuser. It needs
`CONNECT` on the database and `USAGE` and `CREATE` on the schema the connection
sets as `search_path`, since that is where the tables live; migrations take an
advisory lock, which every role may do. To run inside a schema it does not own:

```sh
MORAINE_DATABASE_URL='postgres://app:secret@host/moraine?options=-csearch_path%3Dapp' \
  cargo run -p moraine-server -- migrate
```

With this set, the whole server test suite runs against PostgreSQL: each test
runs in its own schema and touches nothing outside it, so the database only
needs to be one you do not mind filling with throwaway schemas. The role must
be able to create a schema in it, and nothing more; the suite is verified
against a role that owns its database rather than a superuser, and the server
against one that owns neither the database nor the schema it writes to.

```sh
MORAINE_TEST_POSTGRES=postgres://postgres:postgres@127.0.0.1:5432/moraine_test \
  cargo test -p moraine-server
```

Federation is exercised in `server/tests/two_hosts.rs`: the test starts two
server processes on their own ports and data directories, publishes a project
on one, and syncs it from the other over HTTP. Two routers in one process would
prove less, because they would share the code paths that a network hop is most
likely to break.

Read a mod archive's manifest without running it:

```sh
cargo run -p moraine-publish -- inspect mod.jar
```

It reads `fabric.mod.json`, `quilt.mod.json`, or `META-INF/mods.toml` and prints
the mod ID, name, version, and loader. It never executes archive contents and
refuses any metadata entry over a size limit. A game definition names the
extractor it expects, and `--extractor minecraft/fabric-json` (or
`minecraft/quilt-json`, `minecraft/forge-toml`) reads only that format instead
of trying each in turn, so a declared extractor is enough for a client to read a
manifest without guessing.

Preview where files would be installed for a game:

```sh
cargo run -p moraine-publish -- plan --adapter minecraft/default \
  --mod example.jar=sha256:<hex> --override config/example.toml=sha256:<hex>
```

The plan keeps every path inside the adapter's roots; nothing is written.

Install a lockfile from a directory of blobs named by digest:

```sh
cargo run -p moraine-launcher -- install --lockfile lock.json --blobs ./downloads \
  --root ./instance --adapter minecraft/default --dry-run
```

Without `--dry-run` it writes the plan under the root, but only after every
locked artifact's size and SHA-256 digest have been checked. Use
`--home https://home.example` instead of `--blobs` to fetch the bytes from a
registry; plain HTTP is only allowed for loopback with `--allow-http-local`.

When `--adapter` is omitted, the launcher fetches the game definition from
`--home`, verifies it against the game's own genesis, and uses the adapter that
definition declares. It refuses a declared adapter this build does not
implement rather than guessing:

```sh
cargo run -p moraine-launcher -- install --lockfile lock.json --home https://home.example \
  --root ./instance
```

The same definition also names a metadata extractor. Show both and whether this
launcher implements them, or read an artifact's manifest through the declared
extractor instead of guessing its format:

```sh
cargo run -p moraine-launcher -- definition --home https://home.example --game <game-id>
cargo run -p moraine-launcher -- inspect --home https://home.example --game <game-id> --file mod.jar
```

Build a lockfile from a home without one:

```sh
cargo run -p moraine-launcher -- resolve --home https://home.example \
  --project <project-id> --game <game-id> --game-version 1.20.1 --output lock.json
```

Resolution follows the root project's required dependency closure through the
home's feed, then runs the same deterministic solver as `moraine-resolver`.

Publish a release with the CLI:

```sh
cargo run -p moraine-publish -- keygen --key publisher.key
cargo run -p moraine-publish -- init --key publisher.key --home http://127.0.0.1:8080
cargo run -p moraine-publish -- upload --home http://127.0.0.1:8080 --file mod.jar \
  --api-key $MORAINE_API_KEY
cargo run -p moraine-publish -- release --key publisher.key --home http://127.0.0.1:8080 \
  --project <project-id> --game <game-id> --game-version 1.20.1 \
  --version 1.2.3 --file mod.jar
cargo run -p moraine-publish -- publish --key publisher.key --home http://127.0.0.1:8080 \
  --project <project-id> --object <release-id>
```

`upload` needs a key that carries `artifacts:write`, and `MORAINE_API_KEY` can
supply it instead of the flag; without it the home refuses the upload before
reading the body.

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

A changelog is a signed object, not a string on the release. Write release
notes in Markdown, sign them, then bind the digest from the release:

```sh
cargo run -p moraine-publish -- changelog --key publisher.key --home http://127.0.0.1:8080 \
  --project <project-id> --file CHANGELOG.md
cargo run -p moraine-publish -- release --key publisher.key --home http://127.0.0.1:8080 \
  --project <project-id> --game <game-id> --game-version 1.20.1 --version 1.2.3 \
  --file mod.jar --changelog sha256:<changelog-digest>
```

Each Markdown heading becomes a section with its body, and the whole text is
indexed for search under the project.

An advisory is attributed evidence, not a takedown: it is shown on the release
page and in the advisories API, and it never changes the signed record. Only
`malware` at `high` or `critical` may block promotion.

Scanner providers also publish signed evidence about an artifact digest:

```sh
cargo run -p moraine-publish -- attestation --key scanner.key --home http://127.0.0.1:8080 \
  --signer my-scanner --artifact sha256:<hex> --kind sbom \
  --media-type application/spdx+json --subject <project-id> --body sbom.spdx.json
```

The signer must be a provider pinned on the home. Kinds are `build-provenance`,
`review`, `scanner-result`, `sbom`, and `compatibility-test`; `--body` attaches
small evidence inline and `--body-digest sha256:<hex>` references larger evidence
by digest. Evidence is listed at `GET /v1/attestations/{sha256}`, optionally
filtered by `kind`.

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

A game, loader, or runtime is its own signed identity, and the server reads
them from a `definitions` directory beside the data directory at startup. Sign
one with a key of your own; the identity is the genesis, so its ID is derived
rather than chosen.

A definition can be authored as a readable TOML file and compiled to canonical
signed bytes, which keeps a definition reviewable and diffable. `examples/`
holds starting points:

```sh
cargo run -p moraine-publish -- keygen --key definitions.key
cargo run -p moraine-publish -- define --key definitions.key \
  --dir examples/definitions/minecraft --out data/definitions
```

`--dir` compiles every file under a directory, signing the whole set with one
key. Files refer to each other by name, not by ID: `game.toml` sets
`name = "minecraft"`, and a loader says `game_id = "minecraft"`. The tool
resolves those as it goes — game and runtime first, then loaders, then loader
versions and mappings — so a bundle is one command with no IDs to paste. Use
`--file` instead to define a single file when the referenced IDs already exist.

`examples/definitions/minecraft/` is a worked set for one game, laid out the way
the data is: the game definition with Minecraft's version table and category
vocabulary, a runtime, one file per loader under `loaders/`, each loader's
versions under `loaders/<loader>/<version>.toml`, and a `mapping` that says
Cleanroom accepts Forge mods in one direction.

Declaring loader versions is optional. The loader file itself can list
`game_versions` — the game versions the loader family supports — so the
game-to-loader connection is recorded once instead of once per build. With
`game_version_scheme = "ordered-list"` the list holds ranges like `1.14..=26.3`
resolved through the game's version table; `loaders/fabric.toml` does that, and
`loaders/neoforge.toml` lists the exact versions. Either way you do not have to
hunt down every loader build and work out which Minecraft version each one
targets.

When you do want precision, a loader's *versions* are separate
`kind = "loader-release"` files, each naming the `game_versions` it supports
and, optionally, the `runtime_id` and `runtime_versions` it needs. So
`loaders/neoforge/21.1.72.toml` says NeoForge 21.1.72 runs on Minecraft 1.21 and
1.21.1 while `loaders/neoforge/20.4.237.toml` says 20.4.237 runs on 1.20.4 — the
loader ID is the same for both, and the loader definition is not republished to
add a version. A per-version `game_versions` is used when present; the loader's
family list answers for game versions where no per-version record exists. A
loader whose versions are not SemVer, like NeoForge's `26.3.0.7-beta`, lists
them in its `versions` table so ranges can be ordered.

A loader's game versions must exist in the game's table to be ranged over, so
`game.toml` includes Minecraft's alpha/beta line. That lets `babric` cover
`b1.0..=b1.7.3` and `bta-babric` name `b1.7.3`, the Minecraft version Better
Than Adventure is built on. BTA's own release numbers are not Minecraft
versions; they would belong to a BTA game definition, so `bta-babric` is a
loader identity here rather than a full game declaration.

The same definitions can be authored from flags instead:

```sh
cargo run -p moraine-publish -- define-game --key game.key --name "Minecraft" \
  --version-ordering semver --out data/definitions
cargo run -p moraine-publish -- define-loader --key loader.key --game <game-id> \
  --name "Fabric" --out data/definitions
cargo run -p moraine-publish -- define-loader-release --key loader.key \
  --loader <loader-id> --version 0.15.0 --game-version 1.20.1 --out data/definitions
cargo run -p moraine-publish -- define-loader-acceptance --key loader.key \
  --loader <loader-id> --accepts <accepted-loader-id> --game <game-id> \
  --qualification most --out data/definitions
cargo run -p moraine-publish -- define-runtime --key runtime.key --kind java \
  --name "Java" --out data/definitions
```

A game definition can also name the adapter identifiers a launcher needs, so a
client does not hard-code a game:

```toml
kind = "game"
display_name = "The Sims 4"
version_ordering = "opaque"
loaders_allowed = false
install_adapter = "sims4/default"
```

`metadata_extractor` names how to read a mod archive's own manifest, and
`install_adapter` names where its files go. Both are identifiers resolved by
the consuming tool, not code in signed bytes, so publishing a definition that
names an adapter does not execute it. Two install adapters ship:
`minecraft/default` places files under `mods/`, and `sims4/default` places them
under `Mods/`, the folder The Sims 4 loads from. A launcher reads the name from
the definition and refuses an adapter this build does not implement rather than
placing files somewhere plausible-looking.

A definition is signed by the operator who authors it, so this repository ships
no pre-signed seed: a shipped definition would need a shipped private key, and
a game definition's `loader_authorities` would then name an identity nobody
else can extend. Author your own once and reuse the key.

Each command writes the signed bytes the definitions directory loads; drop them
in and restart, or `POST` them to `/v1/games`, `/v1/loaders`, or `/v1/runtimes`
and then to the matching `/definitions` route. A readable file is compiled and
signed locally; the text is never signed and never served, so editing it after
the fact changes nothing. A loader names the game it
targets, so publish the game first. `define-loader-release` is a loader object
of the release shape: it records one loader version and the game versions it
supports, and it does not replace the loader definition. Loader releases and
acceptance mappings are the other two shapes under `loader-def`, and the
directory loader recognizes all three. A published release is listed at
`GET /v1/loaders/{id}/releases`, and `(loader-id, version)` binds to one object:
re-publishing the same version with different bytes is refused, so a loader's
version history cannot be silently rewritten. An acceptance mapping is the
`mapping` shape: it says the accepting loader may run another loader's
artifacts in one direction only, is never followed transitively, and is listed
at `GET /v1/loaders/{id}/accepts`. Auto-selection through a mapping stays off
unless a client chooses to act on the label. The canonical example is
Cleanroom, which runs many Forge mods: a mapping from Cleanroom to Forge means
an instance running Cleanroom may offer a Forge-declared mod as a labelled
candidate, while an instance running Forge never offers a Cleanroom-only mod,
because no mapping authorizes that direction.

```
cargo run -p moraine-publish -- define-loader-acceptance --key cleanroom.key \
  --loader <cleanroom-id> --accepts <forge-id> --game <game-id> \
  --qualification most --out data/definitions
```

`publish` and `submit` take `--kind` and default to `release-published`. The
key is your project's root. Keep it safe: losing it means losing the project
identity. `init` signs a fresh project and authorizes the object kinds it may
publish, `upload` stores the artifact bytes at the home, `release` signs and
stores a release object, `profile` signs and stores display metadata,
`changelog` signs release notes, and `publish` appends the feed entry that
makes an object visible. A kind the genesis does not authorize is refused even
when the signature is valid, so `init` grants `delegation`, `release`,
`profile`, `changelog`, and `modpack`.

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
pnpm install
pnpm dev
```

`pnpm build:static` produces a static site. `pnpm build:cloudflare`
produces a Worker build from the same source. `pnpm lint` runs Oxlint,
`pnpm fmt:check` runs Oxfmt, `pnpm fmt` rewrites files in place, and `pnpm test`
runs the Vitest suite. The site reads
`PUBLIC_MORAINE_REGISTRY` for its default home, and uses it as the API base for
writes too. The server answers read requests with permissive CORS and no
credentials, so any static site can resolve projects and fetch blobs. Signed
writes need no credential either, so a cross-origin site can publish them.

Account-side writes need a credential. If you host the site somewhere other
than the registry, list its origin in `MORAINE_WEB_ORIGINS`; the server then
allows those origins to write with credentials, switches the session cookies to
`SameSite=None`, returns the CSRF token in the login response body, and checks
the request `Origin`. An origin that is not listed gets reads only. If the site
and the API share a hostname, no allowlist is needed.

A release page can hash a file you already downloaded and check it against the
release's artifact digests. That is a byte check in the browser; the verifier
CLI is still what checks the publisher's signature. A release marked withdrawn
shows a warning at the top of its page, and the feed names the withdrawal.

The account console (`/account`) and review queue (`/review`) use session
cookies. They work same-origin when the site is served by the registry via
`--web-dir`, and cross-origin when the site's origin is listed in
`MORAINE_WEB_ORIGINS`.

The publish console (`/publish`) can create a project and publish a release
without the CLI. It takes a key you generate, paste, or open from a file, and
signs genesis, releases, profiles, and changelogs in WebAssembly built from the
same crates as `moraine-publish`, so the bytes are identical. The key is never
uploaded or stored; it lives in the tab and is gone when you close it. The
console warns when you are about to sign with a project root rather than a
delegated release key, and offers the key file for download, because a browser
key that is not backed up is a project you cannot recover. The CLI remains the
better choice for a root key you care about.

## Operator limits

A few environment variables bound what one instance will do. They are not
advertised in the discovery document, since they are anti-abuse settings and
not part of the protocol.

- `MORAINE_MAX_PROJECTS` (default 10000, 0 disables) caps how many projects the
  instance holds; a new project past the cap is refused with 403.
- `MORAINE_MAX_UPLOAD_BYTES_PER_ACCOUNT` caps stored bytes per account and
  `MORAINE_MAX_ARTIFACT_BYTES` caps a single file.
- `MORAINE_REQUESTS_PER_MINUTE` caps request rate per credential and address.
- `MORAINE_MAX_MIRROR_PROBES_PER_CYCLE` caps how many mirror commitments the
  maintenance worker verifies per run, and `MORAINE_MAX_MIRROR_PROBE_BYTES`
  skips any single commitment larger than that, so a mirror cannot make the
  instance download an unbounded artifact. A skipped commitment is left
  unverified rather than marked unreachable.

## Metrics and observability

`GET /metrics` serves Prometheus text: request counts, request duration, server
errors, signature failures split out from federation network and protocol
failures, forks and key changes, admission queue depth and age, review
decisions, webhook backlog, subscription lag, blob collection, sessions and API
keys created and revoked, and failures to serve a committed artifact. Scrape it
with any Prometheus-compatible agent, or point a local collector at it.

Nothing is pushed. That is deliberate: the registry has no outbound telemetry,
so an instance that exports nothing sends nothing, and a backend you have not
chosen costs no egress. To ship the metrics to Grafana or SigNoz, run an
OpenTelemetry Collector next to the server and pick one profile:

```sh
docker run --rm -p 4317:4317 \
  -e MORAINE_METRICS_TARGET=host.docker.internal:8080 \
  -e GRAFANA_OTLP_ENDPOINT=https://otlp-gateway.example/otlp \
  -e GRAFANA_AUTH=<base64 instance:token> \
  -v "$PWD/deploy/otel/collector-grafana.yaml":/etc/otelcol/config.yaml \
  otel/opentelemetry-collector-contrib --config=/etc/otelcol/config.yaml
```

`deploy/otel/collector-signoz.yaml` is the same scrape with a SigNoz OTLP
exporter instead, so choosing a stack is choosing which config the collector
runs. Only the chosen exporter is in the pipeline; the other backend is not
contacted at all. Set `MORAINE_METRICS_TARGET` to the server's host and port.

## Layout

```
codec/          deterministic CBOR canonical profile
crypto/         keys, signatures, domain separation, object IDs
model/          typed, validated protocol objects
verify/         moraine-verify CLI and vector runner
publish/        moraine-publish CLI that signs and publishes releases
resolver/       deterministic dependency resolution and lockfiles
metadata/       safe mod-archive metadata extraction
install/        per-game install placement planning with path containment
launcher/       game-agnostic installer core that verifies locked bytes and applies a plan
wasm/           browser signer that compiles the model and crypto crates to WebAssembly
server/         moraine-server registry, directory, and worker binary
web/            SvelteKit website with static and Cloudflare build targets
protocol/
  spec/         notes that pin implementation decisions
  vectors/      the test-vector corpus
interop/        independent Python checker for the vector corpus
docs/           operator, security, and author guides, and a policy template
deploy/
  otel/         collector profiles for a Grafana or SigNoz metrics backend
  compose/      a compose file that keeps the port on localhost
  systemd/      a hardened unit file
```

`Dockerfile` builds the server and the website into one image. `SECURITY.md`
says how to report a vulnerability, and `CONTRIBUTING.md` covers building,
testing, and the rule that a signed-format change moves the corpus, the Rust
implementation, and the independent checker together.

## License

The server and the website are AGPL-3.0-or-later, so a modified hosted service
has to share its changes. See `LICENSE`.

Everything a client needs to speak the protocol without the server is MIT OR
Apache-2.0: the `codec`, `crypto`, `model`, `metadata`, `install`, `verify`,
`publish`, `resolver`, and `launcher` crates, and the `interop/` checker. See `LICENSE-MIT` and
`LICENSE-APACHE`, and take whichever of the two you prefer. That split is on
purpose. Independent clients should never need our permission, and the
protocol crates carry no service logic worth hiding.
