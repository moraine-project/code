# Publishing a project

You publish from a machine you control, with a key only you hold. A home stores
what you sign and serves it. It cannot change it, and moving to another home
does not change the project's identity.

## Get the tools

```sh
cargo build --release -p moraine-publish -p moraine-verify -p moraine-launcher
```

## Make a key and a project

```sh
moraine-publish keygen --key publisher.key
moraine-publish init \
	--key publisher.key \
	--home https://example.org \
	--project my-mod
```

`keygen` writes a root key. Back it up somewhere you would survive losing a
laptop, because it is the only thing that proves the project is yours. `init`
writes the project's genesis record and a request that the home admits it.

Create an account on the home, then submit the project from the website, or
with a scoped API key:

```sh
moraine-publish submit \
	--key publisher.key \
	--home https://example.org \
	--project my-mod \
	--kind project-admission
```

An instance may run in `review` mode, where a submission waits for an operator.
In `progressive` mode the first accepted release creates a scoped local grant for
that publisher and later matching releases can be admitted automatically until
the grant is suspended or revoked. In `open` mode it is admitted immediately.

## Sign a release

```sh
moraine-publish release \
	--key publisher.key \
	--home https://example.org \
	--project my-mod \
	--game minecraft \
	--game-version 1.21.1 \
	--loader fabric \
	--version 1.2.0 \
	--file my-mod-1.2.0.jar \
	--changelog CHANGELOG.md
```

Then upload the bytes and publish the record:

```sh
moraine-publish upload --home https://example.org --file my-mod-1.2.0.jar
moraine-publish publish \
	--key publisher.key \
	--home https://example.org \
	--project my-mod \
	--object release.json
```

`upload` runs first so the artifact digest is already known to the home, and
`publish` makes the signed record visible. A release is immutable once
published. Corrections take a new version.

The project's identity is fixed by genesis, so the same signed record verifies
at any home. What differs between homes is the ID in the URL, not the bytes.

## Describe the project

```sh
moraine-publish profile \
	--key publisher.key \
	--home https://example.org \
	--project my-mod \
	--game minecraft \
	--name "My Mod" \
	--summary "Does the thing" \
	--category gameplay \
	--tag magic
```

A profile is signed and versioned like everything else, and its history is
shown. Renaming or recategorizing is a new profile revision, not an edit in
place.

## Changelogs, advisories, and attestations

```sh
moraine-publish changelog --key publisher.key --home https://example.org \
	--project my-mod --version 1.2.0 --file notes.md
moraine-publish advisory --key publisher.key --home https://example.org \
	--project my-mod --id 2026-01 --severity high --summary "Fixes a crash" --file detail.md
```

Attestations are signed statements by whoever produces them, not by the
publisher. A scanner, a reviewer, or a build system signs its own.

## Game and loader definitions

Definitions describe a game, its loaders, its runtimes, and their versions.
They are as security-relevant as releases, because they decide which
compatibility claims are accepted. Write them as a directory of TOML files and
compile the whole set at once:

```sh
moraine-publish define \
	--dir definitions/minecraft \
	--out definitions/minecraft/objects
```

A bundle names its files, so one file can refer to another by name instead of
by digest, and the compiler resolves the order for you. A game's version list
grows by publishing a new revision; the definition keeps the same identity, so
nothing that referenced it changes.

## Publishing a revision

A definition keeps its identity forever, so a new game version is a revision of
the existing definition rather than a new one. Point the file at the ID you
already published:

```toml
kind = "game"
revision_of = "gd:sha256:..."
display_name = "Minecraft"
version_ordering = "ordered-list"
versions = ["1.20", "1.21", "1.22"]
```

Compile and import it the same way as the original. The revision must be signed
by the key that owns the identity, and the catalogue is append-only: adding a
version, category, or tag is fine; removing one, or changing the ordering
scheme, is refused. Loader versions are not revisions at all — a
`kind = "loader-release"` file is a standalone object, so a loader does not get
republished to add a version.

## Withdrawing and transferring

Withdrawing stops a release from being served and indexed. It does not erase
the signed record or the bytes, so anyone who already has them still can check
what they hold. It is an honest "do not use this", not a memory hole.

```sh
moraine-publish withdraw --key publisher.key --home https://example.org \
	--project my-mod --object release.json --reason "broken build"
moraine-publish transfer --key publisher.key --home https://example.org \
	--project my-mod --new-key new-publisher.key
```

Transfer signs the project over to a new root key. The new key is live from
that point forward, and the change sits in the feed where followers can see it.

## Publishing from the website

You do not need the CLI. The publish console on a home can create a project and
publish a release, signing in the browser with WebAssembly built from the same
crates the CLI uses. Bring a key by generating one, pasting it, or opening a key
file; it stays in the tab and is never uploaded.

Two things are worth knowing. A key generated in a browser is only as safe as
the backup you make, so download it; clearing the tab loses it. And the website
that serves the signing page is in your trust path. For a root key you care
about, use the CLI on a machine you control, or sign releases with a delegated
key and keep the root offline. The console tells you when the key you are using
is a project root.

## Checking what you published

```sh
moraine-verify object --home https://example.org --object release.json
moraine-verify release --home https://example.org --project my-mod --version 1.2.0
```

If you are automating publishing, run the same checks in CI that you run by
hand. The point of signing is that you can prove later what you shipped, so
verify the record you actually published rather than the one you meant to.
