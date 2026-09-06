# Game adapters and install placement

Everything game-specific meets the protocol at a small, named boundary. A game
definition names a metadata extractor and an install adapter by identifier, and
those identifiers resolve to implementations, not to code inside signed bytes.

## Metadata extraction

A metadata extractor reads a mod archive's own manifest without executing
anything in it. The Minecraft extractor reads `fabric.mod.json`,
`quilt.mod.json`, or `META-INF/mods.toml` and returns the loader, mod ID, name,
version, and environment.

Reading is bounded: a candidate entry above a size limit is refused rather than
decompressed, and a non-archive is an error. Nothing from an archive is run
during extraction or indexing.

## Install placement

An install adapter decides where files go for a game. It takes the artifacts of
a release and the overrides of a modpack and returns a plan: one relative path
per digest. It does not write to a user's disk. The launcher applies the plan
only after the bytes are downloaded and verified.

Placement is contained by construction:

- a mod file name must be a single path segment, with no separator, drive
  letter, null byte, or `..`;
- an override path must be relative and free of `..` and `.` segments;
- joining a planned path onto an instance root is rejected, not normalized, if
  it would escape that root.

A path that fails any of these is refused before the launcher touches the
filesystem, so a malicious modpack cannot write outside the adapter's declared
roots. The adapter's declared roots are the only places it may place files.

The launcher core verifies before it writes. It fetches each locked artifact,
checks its byte length and SHA-256 digest against the lockfile, and only then
applies the plan, staging each file and renaming it into place. A mismatch
stops the install with nothing written for that artifact.

The core is UI-neutral: it emits a `Verifying` event per artifact and a
`Placing` event per destination, and checks a cancellation flag between steps,
so a GUI can show progress and stop a run without embedding any install logic. Signature
verification is a separate step, done by the verifier with a pinned root; the
launcher's digest check proves byte identity, not authorship.

Adding a game means adding an adapter and a game definition. It must not change
the resolver, the verifier, the signed schema, or the server.

## Shipped adapters

An adapter identifier resolves against the adapters a build implements. A
consumer that is asked to install for an identifier it does not implement
refuses rather than guessing, so two games with the same-looking layout are
still told apart by their identifier. Two adapters ship today:

- `minecraft/default` places release files under `mods/` and a modpack's
  overrides at their declared relative paths.
- `sims4/default` places release files and overrides under `Mods/`, the folder
  The Sims 4 loads from, because the game has no loader layer and its overrides
  are additional packages rather than configuration files.

The Sims 4 adapter exists as much to test the boundary as to serve the game: it
shares the containment rules and the plan shape but differs in the root it may
write under, and adding it changed neither the core nor the signed formats. A
game definition names its adapter; the launcher reads that name, so a build
that lacks the adapter reports that plainly instead of placing files somewhere
plausible-looking.
