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

## Not implemented yet

Project, game, loader, and definition documents; signed feeds; digest lookup;
authentication and sessions; admission review; advisories; and search. The
storage metadata layer and directory indexing are the next steps.
