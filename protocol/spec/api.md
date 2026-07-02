# HTTP API

The server is a plain HTTP service. This note records the routes that exist
today; more arrive with metadata, authentication, and federation.

## Discovery

`GET /.well-known/mod-registry` returns the capability document: the supported
protocol versions, the artifact size limit, the feed page limit, which artifact
sources the instance serves, which upload modes it accepts, and which roles it
takes.

The document is a hint about limits, not an authority. A rejected request still
answers with its own status.

## Health

`GET /healthz` answers `ok` when the process is up. `GET /readyz` answers `ok`
only when the blob store can be read, and `503` otherwise. Readiness is
deliberately about storage: a server that cannot serve bytes is not ready.

## Blobs

Artifacts are content addressed. An artifact digest renders as
`sha256:<64 hex>`. Object IDs use a separate `gd:sha256:` namespace and are not
blob addresses.

`POST /v1/blobs` streams the request body into private staging, computes the
digest as it goes, and refuses anything over the advertised artifact limit with
`413`. On success it commits the object and returns `201`, a `Location` header
of `/v1/blobs/sha256/<hex>`, and a receipt `{ "digest": "sha256:<hex>", "size":
N }`. Committing the same bytes again is a no-op because the address is the
digest.

`GET /v1/blobs/sha256/<hex>` and `HEAD` serve exact bytes. Responses carry
`Accept-Ranges: bytes`, an exact `Content-Length`, and
`Cache-Control: public, max-age=31536000, immutable`, because a blob URL is
immutable by construction. A single `bytes=` range returns `206` with a correct
`Content-Range`; a range past the end returns `416` with `Content-Range: bytes
*/<length>`. A partial read is never a verified download: verification requires
the full stream and happens in the client.

An unknown or malformed digest returns `404` or `400` and never a substituted
object.

## Projects, objects, and feeds

Signed objects are stored after their signatures are checked. An object is
addressed by its identity digest, the SHA-256 of the domain tag and canonical
payload. That is the same digest embedded in the `gd:sha256:` ID, so a feed
entry's `object_digest` points at an object identity, not at a blob.

`POST /v1/projects` imports a signed project genesis. The genesis verifies
against its own root keys, defines the project ID, and becomes the trust anchor
for everything else. Importing the same ID with different genesis bytes is a
`409`.

`POST /v1/projects/{id}/objects/{kind}` imports any signed object except a
genesis or a feed entry. The server loads the project genesis, verifies the
object against the root keys and threshold, and stores it. `{kind}` is one of
`delegation`, `release`, `profile`, `advisory`, `attestation`, `game-def`,
`loader-def`, or `runtime-def`.

`POST /v1/projects/{id}/feed` appends a signed feed entry. The entry must be
the next sequence, its `previous` must equal the current head digest, and the
referenced object must already be stored. One transaction writes the entry and
advances the head; a profile-updated entry also updates the project's current
profile.

`GET /v1/projects/{id}` returns the genesis ID, head sequence and entry, and
current profile ID. `GET /v1/projects/{id}/feed?after=N&limit=M` returns a
bounded page of entries. `GET /v1/objects/{hex}` returns the exact signed wire
bytes with immutable caching.

Verification accepts the root threshold first. If that fails it loads the
project's stored `delegation` objects, verifies each key delegation against the
root, and accepts an object signed by a delegated key whose `allowed_kinds`
contains that object kind and whose `expires_at` has not passed. So a build
system can hold a release key while the root stays offline.

`channels` and `max_version_scope` are recorded but not enforced yet, and a
delegation is not revoked by a later one: revocation will need an explicit
revocation record tied to the feed sequence.

Writes are not authenticated yet. A valid signature is required, but anyone
can submit validly signed objects and grow storage. Authentication, quotas,
and admission review come with the authoring workflow.

## Not implemented yet

Game and loader definition hosting, digest lookup, range caching of object
documents, directory indexing, authentication and sessions, admission review,
advisories, and search.
