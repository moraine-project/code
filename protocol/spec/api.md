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

## Limits and timeouts

Every request is bounded. Metadata request bodies are limited to 256 KiB, and
the server enforces a 30-second request timeout. Blob uploads and downloads
stream and are bounded instead by the artifact limit advertised in the
capability document; the byte-serving endpoints are not buffered, so a large
file does not count against the metadata body limit.

## Outbound requests

When the server fetches a home or delivers a webhook it resolves the target
host and refuses the request if any resolved address is loopback, private,
link-local, shared (carrier-grade NAT), unspecified, or multicast. This keeps a
publisher-supplied URL from reaching internal services or cloud instance
metadata. Loopback is permitted only when the operator explicitly enables
insecure local federation. The resolution happens once and the connection is
pinned to the addresses that were checked, so a host whose name changes between
the check and the connection cannot swap a public address for an internal one.
Redirects are never followed, so a target cannot bounce a request to an address
that the initial check allowed. Outbound TLS
anchors are the bundled public roots; `MORAINE_TLS_EXTRA_ROOTS` names a PEM
bundle whose certificates are added as additional roots, which is how a
deployment reaches a home behind a private certificate authority. Both the
server and the publishing CLI read a bundle of one or more certificates; the
server reports how many blocks it could not parse, and the CLI refuses a file
with no certificates rather than silently trusting the public roots alone. A home response
is additionally capped at the advertised `max_response_bytes` budget; the
worker refuses a response whose declared or streamed length exceeds it.

Each credential or client address gets a request budget of
`requests_per_minute` (600 by default, `0` disables it) counted over a sliding
minute; exceeding it returns `429` with `Retry-After`. `GET /healthz`,
`/readyz`, and `/metrics` are exempt so monitoring is never throttled. When the
connection comes from a loopback address, as behind a local reverse proxy, the
first `X-Forwarded-For` entry is the client address; otherwise the peer address
is used and the header is ignored.

Every response also carries `X-Content-Type-Options: nosniff`,
`Referrer-Policy: no-referrer`, and a `Content-Security-Policy` that forbids
framing, plugin objects, and a non-self base URI. The website build embeds a
hash-based policy that additionally restricts scripts, styles, images, fonts,
and connections to the site itself.


## First run

`moraine-server migrate` creates the data directory if needed, applies any
pending schema migrations, and reports how many it applied. Migrations are
numbered SQL files under `migrations/<engine>/`, applied in order and recorded
in a `schema_migrations` table so re-running is a no-op. It is the explicit
upgrade step to run before starting a new binary.

The database is SQLite by default: a `metadata.sqlite` file in the data
directory. `MORAINE_DATABASE_URL` points the store at another engine instead,
for example `postgres://user:password@host/database`. Both engines run the same
statements through one driver layer, with a migration directory per engine
because the column types differ (`BLOB` and `INTEGER` against `BYTEA` and
`BIGINT`). PostgreSQL migrations take an advisory lock so two processes
starting together cannot apply one twice. Backup and restore read the SQLite
file directly and refuse a PostgreSQL database; use `pg_dump` there.

The server also migrates on startup, taking an immediate write lock per
migration so two processes starting together cannot both apply one. Set
`MORAINE_SKIP_MIGRATE_ON_START` to make the step explicit: the server then
refuses to start while a migration is pending, rather than serving requests
against an outdated schema.

`moraine-server bootstrap --email <address>` prepares a fresh data directory:
it runs the database migrations, creates the operator account with a generated
password, and writes the server's webhook signing key. The password is printed
once and never stored in plaintext. Running it again for the same address is an
error, so it cannot silently reset an operator's credentials. The default
publishing mode is `review`.

`moraine-server backup --out <directory>` writes a consistent copy of the
server's state before an upgrade: a `metadata.sqlite` snapshot taken with
SQLite's own consistent-dump mechanism, a `blobs/` directory holding the
artifact bytes, and a `blobs.txt` inventory of each digest and its size for
after-restore checking. A backup does not include a publisher's root keys, so
those need their own copy, and the server's webhook signing key should be
backed up alongside the database.

`moraine-server verify-backup --dir <directory>` opens the snapshot read-only,
counts its projects, and checks every inventory entry: that the blob file is
present, that its length matches the recorded size, and that its SHA-256 is the
digest that names it. A file that was truncated or altered after the backup
fails the check rather than passing silently.

`moraine-server restore --dir <directory>` verifies the backup first, then
copies the snapshot and blob bytes into the configured data directory. It
refuses to write over an existing database or blob directory unless `--force`
is given, so a restore cannot quietly replace a live host's data. A restore
does not recreate the webhook signing key, which lives outside the backup and
is regenerated if absent.

## Health

`GET /healthz` answers `ok` when the process is up. `GET /readyz` answers `ok`
only when the blob store can be read, and `503` otherwise. Readiness is
deliberately about storage: a server that cannot serve bytes is not ready.

`GET /metrics` renders Prometheus text with the object, submission, review,
subscription, delivery, definition, advisory, mirror, and artifact counts the
store holds, plus process uptime, total requests, 5xx responses, rejected
signatures, and federation failures split by cause: network transport,
protocol (URL or decode), signature rejection, local storage, and local
policy rejection. Counters separate signature rejection from transport failure, and
gauges report the age of the oldest queued submission and the oldest pending
webhook delivery, so an admission backlog is visible before it is asked about.
It carries no account or topology detail and is meant to be scraped from inside
the operator's network or restricted at the reverse proxy.

## Blobs

Artifacts are content addressed. An artifact digest renders as
`sha256:<64 hex>`. Object IDs use a separate `gd:sha256:` namespace and are not
blob addresses.

Committed blobs live either on the local filesystem or in an S3-compatible
object store. The choice is an operator setting and does not change a digest,
a URL, or a response: the store is addressed by digest on both backends, and a
client cannot tell which one holds the bytes. Uploads always stage on local
disk first, so a partial transfer never reaches the object store, and a
committed blob is read back by streaming, with byte ranges passed through to
the backend. An object-store instance names a bucket, an endpoint, a region, a
prefix, and credentials; a local instance needs only its data directory.

`POST /v1/blobs` streams the request body into private staging, computes the
digest as it goes, and refuses anything over the advertised artifact limit with
`413`. It requires the `artifacts:write` scope, so an unauthenticated caller
cannot fill the store; a session grants it, and an API key needs it explicitly.
Each account also has a total stored-bytes quota, five GiB by default and set
with `MORAINE_MAX_UPLOAD_BYTES_PER_ACCOUNT`, advertised in the capability
document. An upload that would cross it is refused with `403` before the body
is read; the quota is charged once the bytes commit and released when an
unreferenced blob is collected. On success it commits the object and returns `201`, a `Location` header
of `/v1/blobs/sha256/<hex>`, and a receipt `{ "digest": "sha256:<hex>", "size":
N }`. Committing the same bytes again is a no-op because the address is the
digest.

A blob is served only once a signed release location, an artifact index entry,
or a mirror commitment references it. Bytes sitting in staging or committed by
an upload whose release was never published are drafts and answer `404`, so an
unvalidated upload is not public merely because it reached the store.

`GET /v1/blobs/sha256/<hex>` and `HEAD` serve exact bytes. Responses carry
`Accept-Ranges: bytes`, an exact `Content-Length`, and
`Cache-Control: public, max-age=31536000, immutable`, because a blob URL is
immutable by construction. A single `bytes=` range returns `206` with a correct
`Content-Range`; a range past the end returns `416` with `Content-Range: bytes
*/<length>`. A partial read is never a verified download: verification requires
the full stream and happens in the client.

An unknown or malformed digest returns `404` or `400` and never a substituted
object.

Staging files left by an aborted upload are removed after an hour, and a
committed but still unreferenced draft is not served. A committed
blob that no release location, artifact index entry, or mirror commitment
references is removed after a seven-day window, which leaves a fresh upload
time to be published. Both windows are operator settings
(`MORAINE_STAGING_RETENTION_SECONDS`, `MORAINE_BLOB_RETENTION_SECONDS`). The
collection, the retention prune of notifications and webhook deliveries, and
the periodic definition and subscription resync all run on one maintenance
tick, an hour by default and set with `MORAINE_MAINTENANCE_INTERVAL_SECONDS`; a
value of zero turns the background loop off, which leaves those jobs to be
triggered externally. A release whose bytes were collected still resolves; its
artifact is simply unavailable at this host, and byte serving answers `404`
rather than a substitute.

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
object against the root keys and threshold, and stores it. A genesis fixes a
closed set of authorized kinds, and an object whose kind is not in that set is
rejected with `400` even when its signature is valid; a kind that is not listed
cannot be published without a new project identity. `{kind}` is one of
`delegation`, `release`, `profile`, `advisory`, `attestation`, `game-def`,
`loader-def`, `runtime-def`, `modpack`, or `changelog`. A changelog is a signed
object referenced by a release's `changelog_digest`; it carries locale-tagged
sections with an optional severity each, and its text is indexed for search
under the project the release names, so a phrase that only appears in release
notes still finds the project.

`POST /v1/projects/{id}/feed` appends a signed feed entry. The entry must be
the next sequence, its `previous` must equal the current head digest, and the
referenced object must already be stored. One transaction writes the entry and
advances the head; a profile-updated entry also updates the project's current
profile.

`GET /v1/lookup?sha256=<hex>` resolves an artifact digest back to the releases
that publish it. The hex may carry the `sha256:` prefix. A release is indexed
when it is stored, and federation indexes releases it ingests, so the lookup
covers every release this instance knows about. A match names the project, the
release object, and the artifact's version and filename. An unknown digest
returns an empty match list, not a `404`, because "not here" is not an error.

`GET /v1/projects/{id}` returns the genesis ID, head sequence and entry,
current profile ID, and the owner if one is recorded.

`POST /v1/projects/{id}/transfer` verifies and stores a signed
ownership-transfer object. It must carry two signatures over the same payload,
one from each side, and it returns the transfer object's ID. It does not change
the owner by itself.

The owner changes when a feed entry of kind `ownership-transferred` that
references the transfer object is accepted. The same rule runs on the home, in
review acceptance, and during federation sync, and it re-verifies the transfer
and requires that it start from the currently recorded owner. Ownership is
therefore a feed fact: a directory learns it by verifying the same signed
material, not by trusting the home's summary. `GET /v1/projects/{id}/feed?after=N&limit=M` returns a
bounded page of entries. Optional `game_version`, `loader`, `loader_version`,
`runtime`, and `runtime_version` filters restrict the page to release entries
whose declared compatibility matches them. The game version is evaluated
through the game's declared version ordering, the loader version through the
loader definition's declared ordering, and the runtime version through the
runtime definition's declared ordering, rather than string comparison; the
loader is matched by the loader ID a release declares. A `runtime_version`
without a `runtime` matches nothing, and the runtime filter matches a release
whose `runtime_predicate` accepts the version. A version range is only
evaluated when the definition that declares its ordering is hosted here;
without it the entry is treated as unsatisfied rather than assumed to be
semver, so a range that cannot be evaluated without an ordered list of versions
is treated as unsatisfied. Non-release entries are kept. The page reads
further when a full batch yields fewer than the requested entries, up to
`max_feed_scan_pages` batches (50 by default, advertised in the capability
document), so a filter does not return a short page while matching entries sit
immediately after. When the scan stops at that bound with the page still
short, the page sets `truncated` and a client continues from `next`. Each entry carries a human `title` derived from the
referenced object (a release's version and channel, a profile's display name,
an advisory's severity and category, a delegation's purpose), so a page can say
what changed without fetching every object. A release entry also carries a
`release` summary with its channel, game ID, and the loaders its compatibility
entries declare, so a client can filter a feed by loader without fetching each
release. Both are conveniences for display; the object digest next to them is
the fact. `GET /v1/objects/{hex}` returns the exact signed wire
bytes with immutable caching, and supports `HEAD` and single byte ranges, so a
large object can be resumed like a blob. A `416` reports an unsatisfiable
range.

Two read-only projections decode a stored signed object into JSON so a browser
does not have to reimplement the canonical decoder. They are display views, not
trust anchors: the signed bytes remain the source of truth, and a client that
verifies must fetch the object document instead.

- `GET /v1/projects/{id}/profile` returns the current profile revision's
  display name, summary, description, categories, tags, links, and communities.
  It is `404` until a `profile-updated` entry has been accepted.
- `GET /v1/projects/{id}/releases/{hex}` returns one release's version,
  channel, kind, license, artifacts, compatibility entries, dependencies, and
  rights. The digest is the release object's identity digest. A `withdrawal`
  field appears once a `release-withdrawn` feed entry accepts a signed
  withdrawal for that release; the release record and its digest never change.
  When the release binds a changelog, a `changelog` field carries that object's
  identity digest.
- `GET /v1/projects/{id}/changelog/{hex}` returns a changelog's locale-tagged
  sections, each with a heading, body, and optional severity. The digest is the
  changelog object's identity digest. It is `404` unless the object was stored
  under this project, so a digest from another project cannot be read here.

A withdrawal is a signed statement published as a stored object and made
effective by a feed entry, exactly like an ownership transfer. Its reason is
one of `compromise`, `harmful`, `broken`, `legal`, or `author-preference`, and
it marks a release without erasing it.

Verification accepts the root threshold first. If that fails it loads the
project's stored `delegation` objects, verifies each key delegation against the
root, and accepts an object signed by a delegated key whose `allowed_kinds`
contains that object kind and whose `expires_at` has not passed. So a build
system can hold a release key while the root stays offline.

A delegation is honored only when it is unrestricted. If it carries
`channels`, `max_version_scope`, or `valid_from_seq`, the verifier refuses it,
because it does not evaluate those restrictions and honoring the delegation
would grant more authority than the issuer declared. The same rule applies
wherever delegation verification runs, including a launcher reading a home's
feed. A delegation is not revoked by a later one; revocation will need an
explicit revocation record tied to the feed sequence.

Object import is signature-gated but unauthenticated: anyone can store a
validly signed object, because the signature is what authorizes it.

Appending a feed entry directly, though, is accepted only while the instance is
`open`. Under `review` it returns `409`, and the only way to add a feed entry
is the submission flow below. That closes the obvious bypass: admission review
would mean nothing if a publisher could commit straight to the feed. Federation
ingest is unaffected, because a directory mirroring a home is not publishing to
its own feed.

## Admission review

The instance has one setting, `publishing`, chosen on the command line and
advertised in the capability document: `review` (the default) or `open`.

`POST /v1/submissions` accepts a signed feed entry, verifies its authorization
and that the referenced object is stored, and then either:

- under `open`, commits it immediately and records an `auto-accepted`
  submission; or
- under `review`, records a `submitted` submission and returns `202`.

`GET /v1/review-queue` lists submissions that are `submitted` or
`under_review`. `POST /v1/submissions/{id}/assign` moves a `submitted`
submission to `under_review` and records the calling reviewer as its assignee,
so two reviewers do not work the same submission by accident. `GET
/v1/submissions/{id}` returns a submission and its decisions to the submitter
or a reviewer. `POST /v1/submissions/{id}/review` takes one of:

- `accept` — re-checks continuity against the current head, commits the entry
  in one transaction, and records an `accept` decision;
- `reject` or `quarantine` — records the decision with a required reason code
  from the version-1 taxonomy and never commits the entry.

A submission can be decided while `submitted` by any reviewer, or while
`under_review` by its assignee. `GET /v1/submissions` lists the calling
account's own submissions with their decisions, so an author can see the state
of a submission, the reason code behind a rejection, and any appeal route
without holding a reviewer scope. Both it and `/v1/review-queue` take `limit`
(capped at 200) and an opaque `cursor` of the form `created_at:id`, taken from
the last submission of the previous page. The ID is the tiebreaker, so two
submissions created in the same second cannot be skipped or repeated across a
page boundary.

A decision applies to one object digest and is an instance-attributed policy
record; it never alters signed bytes, and the signed entry stays stored for
audit even when rejected. Submitting needs the `submissions:write` scope;
reviewing needs `submissions:review`.

Because the head can move between submission and acceptance, `accept`
re-validates sequence continuity at commit time and returns `409` if the entry
is no longer the next sequence. The author resubmits against the new head.

## Accounts and credentials

`POST /v1/auth/register` takes an email and password, hashes the password with
Argon2id, and returns the user ID. Passwords must be at least 12 characters.

`POST /v1/auth/session` verifies the password and creates a server-side
session. It sets an `HttpOnly`, `Secure`, `SameSite=Lax` session cookie and a
readable CSRF cookie, and returns the session's absolute expiry. Sessions are
idle-limited to 30 days and absolutely limited to 90 days.

`DELETE /v1/auth/session` revokes the session and clears both cookies.

Failed sign-ins are budgeted per account: ten wrong passwords within fifteen
minutes exhaust the budget, and further attempts are refused with `429` and a
`Retry-After` header until the window passes. A successful sign-in clears the
budget.

`GET /v1/auth/me` returns the account and whether the request authenticated
with a session or an API key.

`GET /v1/auth/keys`, `POST /v1/auth/keys`, and
`DELETE /v1/auth/keys/{id}` list, create, and revoke scoped API keys. A created
key is returned exactly once; only its SHA-256 hash is stored. Keys are
scoped, expire by default after 90 days, and are revocable independently. The
known scopes are `account:read`, `keys:manage`, `projects:write`,
`artifacts:write`, `submissions:write`, `submissions:review`,
`federation:manage`, `orgs:manage`, `directory:manage`, and
`notifications:read`. There is no wildcard, and no scope can sign a release.

A request authenticates either with the session cookie or an
`Authorization: Bearer` API key. A cookie-authenticated request that changes
state must also send `X-CSRF-Token` equal to the CSRF cookie. A session carries
the whole account; an API key carries only the scopes it was granted, and
`keys:manage` is required to manage keys through a key.

## Federation

A directory pulls from a home. `POST /v1/federation/sync` takes a `home_url`
and `project_id`, fetches the project's genesis and object documents, and
ingests its feed:

1. fetch the genesis, verify it against its own roots, and confirm the genesis
   ID matches the requested project;
2. ensure a local project with the same genesis exists;
3. fetch feed entries after the stored cursor in pages, following the home's
   page size, resolving each referenced object, verifying it against the root
   and any stored delegations, and storing it;
4. append each entry with the same continuity checks a direct import uses;
5. advance the subscription cursor to the home's head once no page remains.

A home that serves fewer entries per page than the subscriber asked for is
followed to the end of its feed, so a large backlog is applied in full rather
than truncated at the first page.

A page whose head is lower than the stored cursor is refused with `409` and the
cursor is left untouched. A feed sequence that goes backwards is either an
operator pruning history or a home trying to walk a subscriber back to an
earlier view, and neither is applied silently. When the operator knows the feed
legitimately shrank, `POST /v1/subscriptions/reset?home_url=&project_id=&cursor=`
sets both the cursor and the recorded head, which is the documented recovery;
the next sync then re-reads from that point. A reset increments a per-project
`resets` count, visible in the subscription list and summed by
`moraine_subscription_resets`, so a deliberate recovery stays distinguishable
from a home that quietly rewound.

The event kind maps to an object kind (`release-published` to a release,
`profile-updated` to a profile, `key-changed`/`migration`/`recovery` to a
delegation, `advisory` to an advisory), and an entry whose object kind is
unknown is skipped rather than guessed. A release that binds a changelog is
followed by one extra fetch for that object, so a follower can render release
notes instead of holding a digest it cannot resolve; a home that omits the
changelog fails that sync rather than storing a dangling reference.

Sync is idempotent: re-running it re-fetches nothing past the cursor and
re-verifies everything it does fetch. The cursor advances after each page's
entries are durably stored, so a sync interrupted between pages resumes rather
than replaying entries the local feed already holds. A single sync follows at
most `max_sync_pages` pages and then reports an error, so a home that keeps
growing its head cannot hold the request open indefinitely; the entries already
applied stay applied. The budget is advertised in the capability document and
set with `MORAINE_MAX_SYNC_PAGES`. `GET /v1/subscriptions` lists the followed
homes and their cursors. Both routes need the `federation:manage` scope.

A game, loader, or runtime identity is pulled the same way with
`POST /v1/federation/sync-definition`.

A home URL must use HTTPS. Plain HTTP is rejected unless it points at loopback
and the operator explicitly enabled it, which exists for development. The
client does not follow redirects, and every fetch has a timeout. A response
that runs past the size bound is refused partway through, before its bytes are
buffered. The host is resolved once, every resolved address is checked against
the public ranges, and the connection is then pinned to those addresses, so a
name that would resolve inward on a second lookup never reaches one. A sync is
a synchronous request; the same syncs also run on the maintenance tick.

## Cross-origin reads

A static website runs on a different origin than the registry, so read requests
need CORS. The server answers `GET` and `HEAD` with `Access-Control-Allow-Origin:
*` and no credentials, which lets any static site resolve projects and fetch
blobs. `POST`, `PUT`, `PATCH`, and `DELETE` are deliberately absent from the
allowed methods, so a browser cannot use a cross-origin credential to write.
Automation that must write uses an API key over a direct connection, not a
browser fetch.

Allowing every read origin is safe here because reads are public and carry no
credentials. An operator that wants to restrict reads can narrow the allowed
origin; the protocol does not require a particular policy.

## Search

`GET /v1/search?q=&game=&loader=&tag=&category=&sort=&cursor=&limit=` returns the
portable search response as JSON. Search runs over a local index built from
validated records: a project enters the index when a `profile-updated` entry
is accepted, so the name and summary come from the publisher's signed profile,
never from a directory edit. The index is disposable and can be rebuilt from
stored objects.

`q` matches the display name, summary, description, and any changelog text this
instance holds, case-insensitively. `game`, `tag`, `category`, and `loader` are
exact facet filters. `sort` is one of `relevance`, `updated`, `created`,
`name`, and `popularity`, and any other value is a `400` rather than silently
treated as a different order. `limit` is bounded to 100. Pagination uses an
opaque `next_cursor` and keyset ordering, not an offset, so it does not skip or
repeat rows under concurrent writes.

Ranking is the instance's own policy and never a safety signal. The response's
`source_instance` names the host that answered, and merging several instances'
responses is by `project_id`, keeping each source's attribution. Results are
not signed objects; a client that needs to trust a result fetches and verifies
the underlying records.

## Directory policy

An instance decides what it lists. `PUT /v1/directory/policy/{project_id}`
records a local `directory_policy` for one project and requires the
`directory:manage` scope; `GET /v1/directory/policy/{project_id}` reads it back
to anyone. A project with no recorded policy is `listed`.

`listing_state` is one of `listed`, `unlisted`, `quarantined`, `blocked`,
`withdrawn`, or `unavailable`, and an optional `reason_code` from the published
taxonomy and a short note explain it. Setting `listed` clears the record.

The policy is applied where it matters, and it is local policy rather than a
statement about the release: `unlisted` and `blocked` projects are left out of
search results, `quarantined` ones appear with a `quarantined` annotation
telling a client not to fetch them automatically, and `withdrawn` and
`unavailable` ones appear with that state. The project summary reports the
state and reason, and a `blocked` project answers `404` to its summary so this
instance does not serve it. Listing policy never touches signed bytes; it
records what one operator decided, which another operator is free to disagree
with.

## Legal and takedown requests

`POST /v1/legal-requests` records a legal or policy request against a project or
a release, and needs the `directory:manage` scope. A record names the request
`kind` (`copyright-notice`, `counter-notice`, `court-order`,
`law-enforcement-request`, `platform-policy-action`, or `other`), the
`claimant_ref`, the `target_kind` and `target_id`, the `stated_basis`, the
`received_at` time, the `action_taken` (`none`, `noted`,
`availability-disabled`, or `availability-restored`), an optional
`designated_agent_ref`, and an optional `responds_to` naming an earlier record,
which is how a counter-notice or appeal is filed.

`GET /v1/legal-requests?target_kind=&target_id=` lists the records against one
target, newest first, and `GET /v1/legal-requests/{id}` reads one by its
assigned id. Both need `directory:manage`, because a record can contain a
claimant and a stated legal basis.

Records are append-only: there is no update or delete, and a later decision is
another record. A record never rewrites signed bytes and never claims a release
was cryptographically invalid; it records what an operator received and what it
did. Disabling availability at this instance is a separate `directory_policy`
setting, so the record of the request and the decision to act on it stay
distinct and each is attributed to the operator that made it. An operator that
must delete stored bytes can do so while the record, the feed history, and the
publisher signature still show what existed and who published it.

## Organizations

`GET /v1/orgs` lists the organizations the authenticated account belongs to as
`{ id, handle, display_name, role }`, which is what a management console needs
to show before it knows any handle.

An organization is a named group that owns projects. It is not a login: it has
no password and no session, and every action taken for it is performed by an
authenticated member whose role permits it.

`POST /v1/orgs` creates one from a `handle` and `display_name`; the handle is
validated and unique per instance, and the creator becomes its first `owner`.

- `GET /v1/orgs/{handle}` returns the org and its teams to a member.
- `GET /v1/orgs/{handle}/members` lists members with their roles.
- `POST /v1/orgs/{handle}/members` adds a member by email with a role.
- `DELETE /v1/orgs/{handle}/members/{user_id}` removes a member.
- `GET`/`POST /v1/orgs/{handle}/teams` list and create teams; a team's optional
  parent must belong to the same org, so nesting stays inside one organization.
- `PATCH /v1/orgs/{handle}/teams/{team_id}` moves a team under a new parent, or
  to the top level with a null parent. A move that would point a team at itself
  or at one of its own descendants is refused, so the hierarchy stays a tree.

Roles are `owner`, `admin`, and `member`. Owners and admins manage membership
and teams, only an owner may grant `owner`, and an org must always keep at
least one owner, so the last one cannot be removed.

None of this touches signing. An org role manages a project page; it does not
authorize a release. Signing authority still comes only from a key delegation
under the project's root.

## Advisories

An advisory is a signed statement by a provider about one artifact digest. A
provider is not a project, so it needs its own trust anchor.

`POST /v1/providers/{provider_id}/keys` pins a provider's Ed25519 public key.
It needs an authenticated account, and the pin is local to the instance: every
instance chooses which providers it trusts, and the same provider may be pinned
with a different key elsewhere. Pinning the same ID with a different key is a
conflict.

`POST /v1/advisories` accepts a signed advisory object. The server looks up the
advisory's `provider_id`, verifies the signature against the pinned key, stores
the object, and indexes it. An advisory for an unpinned provider is rejected
with `409`, because an unanchored signature proves nothing.

`GET /v1/advisories?project=<id>` or `?digest=<sha256>` returns attributed
advisories with their provider, severity, category, `block_promotion` flag, and
times. They also appear on the release view under `advisories`.

Only `malware` at `high` or `critical` may set `block_promotion`, and it applies
to the affected digest or range, never the project. An advisory is evidence: it
never alters signed bytes, and a retraction is a later advisory about the same
target.

## Mirrors and locations

Two different things are kept apart, and this is where they are read.

A **location record** is publisher-signed (kind `release`, `type: location`). It
names where an artifact digest may be fetched. The server indexes it when the
object is stored, so `GET /v1/mirrors/{sha256}` lists a digest's locations with
their kind and operator.

A **mirror commitment** is the mirror's own signed statement that it holds the
bytes. A mirror is not a project, so its key is pinned like a provider's:
`POST /v1/mirrors/{mirror_id}/keys` records an Ed25519 key for an authenticated
account, unique to the instance. `POST /v1/mirror-commitments` accepts a signed
commitment, verifies it against the pinned key, and records it; an unpinned
mirror is rejected.

`GET /v1/mirrors/{sha256}` returns `{ digest, locations, commitments }`. A
commitment proves the mirror stored the bytes once, never that it will keep
them, so the endpoint reports evidence, not a promise. A consumer may fetch
from any hint because it checks the digest, but the decision to distribute
still belongs to the operator and the publisher.

Because a commitment is a claim, the maintenance tick re-checks each one
against its endpoint: it fetches `/v1/blobs/sha256/{digest}`, hashes the stream,
and records whether the length and digest matched. Each commitment carries
`last_checked_at` and `reachable` when a check has run, and `null` before the
first one. A failed check is evidence that the bytes are unreachable now, not a
deletion; the commitment stands until the mirror or the operator withdraws it.
Endpoints must be HTTPS, or loopback HTTP when insecure local federation is
enabled, and the same public-address rule as other outbound requests applies.

## Follows and notifications

A signed-in user can follow a project with `POST /v1/follows/{project_id}`
(`DELETE` to stop, `GET /v1/follows` to list). When a feed entry is accepted —
on the home, through review, or during a sync — the server records a
notification for every follower of that project.

`GET /v1/notifications?unread=true` lists them newest first; `POST
/v1/notifications/{id}/read` marks one read and `POST /v1/notifications/read-all`
marks all. A notification names the project, the event kind, the referenced
object, and the feed sequence, and is a local convenience: the feed remains the
source of truth and every notification is reproducible from it. A background
maintenance task prunes notifications older than 90 days.

## Webhooks

An operator can register an outbound webhook with `POST /v1/webhooks`, giving a
URL and an optional list of event kinds to receive (empty means all). `GET
/v1/webhooks` lists the caller's hooks and `DELETE /v1/webhooks/{id}` revokes
one. URLs must be HTTPS, or loopback when the operator explicitly enabled
insecure local fetches; redirects are not followed.

When a feed entry is accepted, the instance builds the shared event payload and
enqueues one signed delivery per matching webhook. A worker retries with
exponential backoff for up to eight attempts, then marks the delivery failed. A
delivery body is:

```json
{ "protocol": 1, "event_id": "…", "event_kind": "release-published",
  "project_id": "gd:sha256:…", "payload": "<canonical CBOR, hex>",
  "signature": "<hex>" }
```

The signature covers `GAMEDIST/v1/webhook\0 || payload`, and the instance's
public key is advertised as `webhook_public_key` in the capability document so
a receiver can pin it. The receiver treats a delivery as a hint and re-fetches
the referenced record before acting; idempotency is by `event_id`. Delivery
records are pruned after 30 days by the same maintenance task.

## Game, loader, and runtime definitions

A game, loader, or runtime is a signed identity of its own, separate from any
project. `POST /v1/games`, `POST /v1/loaders`, and `POST /v1/runtimes` each
import a signed genesis of that kind; the object's digest becomes the ID, and
importing the same ID with different genesis bytes is a `409`.

`GET /v1/games`, `GET /v1/loaders`, and `GET /v1/runtimes` list the served
identities of that kind as `{ id, kind, current, display_name }`. `current` is
the current definition ID or null before one is published, and `display_name`
comes from that definition's payload so a browser can label an entry without a
second request.

`POST /v1/{games|loaders|runtimes}/{id}/definitions` stores a signed definition
object, verified against that identity's root. A game takes a `game-def`, a
loader takes any `loader-def` shape (definition, release, or acceptance
mapping), and a runtime takes a `runtime-def`. A game or runtime definition
becomes the identity's current definition; for a loader only the definition
shape does, because a release or acceptance mapping is an additional object
rather than a replacement, and letting one become current would lose the
loader's display name and ordering scheme. Earlier revisions remain
addressable by digest.

`GET /v1/loaders/{id}/releases` lists a loader's published releases as
`{ version, release, declared_time, game_version_predicate, runtime_id,
runtime_predicate }`, newest first. A loader release records one loader
version, the game versions it supports, and the runtime it needs. The pair
`(loader_id, version_id)` is bound to exactly one object: re-publishing a
version that already exists with different bytes is a `409` rather than a
rewrite, so a loader's version history cannot be silently replaced. The
release objects are signed and remain the source of truth; the listing is an
index over them.

At startup the server reads every regular file in a `definitions` directory
beside the data directory. A file holding a game, loader, or runtime genesis is
imported as that identity; a file holding a signed definition object is
verified against its identity's root and becomes its current definition.
Genesis files are applied before definition files regardless of filename order,
so a single directory can seed an identity and its first definition. Files that
are neither are ignored, and a file that fails to import is reported and does
not stop the server.

`GET /v1/{games|loaders|runtimes}/{id}` returns the identity, its genesis ID,
the current definition ID, and `payload`, a JSON rendering of the canonical
signed payload. The rendering is a convenience for display; the signed bytes
remain the source of truth. A definition is not authoritative because this
instance serves it, only because it verifies against a pinned identity.

A definition is not part of a project feed, so it syncs on its own:
`POST /v1/federation/sync-definition` takes a `home_url`, an `id`, and a `kind`
(`game`, `loader`, or `runtime`), fetches the identity's genesis and current
definition, verifies both against the genesis root, and stores them. A loader
sync also pulls every published loader release, so a follower can resolve
loader versions rather than holding only the family definition.

`POST /v1/federation/subscribe-definition` syncs once and records the identity,
so a periodic task refreshes it; `GET /v1/definition-subscriptions` lists the
followed identities. Both need the `federation:manage` scope. An operator pulls
the identities it wants rather than following another instance's whole catalog,
which keeps a definition curated rather than implicitly trusted.

## Modpacks

A modpack is a signed `modpack` object, not an ordinary release. It names the
game and optional loader, an ordered list of entries, and a set of overrides.
Each entry pins a target kind, a stable ID, an exact release ID, and a digest;
each override is a content-addressed blob with a target path. The object is
stored like any other signed object at
`/v1/projects/{id}/objects/modpack`.

`GET /v1/packs/{hex}` returns the manifest's canonical payload as JSON. Override
bytes are fetched by digest from the blob endpoint. Override paths are validated
when the manifest is decoded: a path must be relative and free of `..`
segments, so a pack can never write outside the adapter's declared roots.

A pack gains no authority over the projects it includes. Their own signatures,
withdrawals, and rights still apply, and a pack that includes a project whose
rights forbid redistribution must reference it as a link or leave it out. A
lockfile is the resolved, client-specific instance of a manifest and is not a
signed publisher object.

## Not implemented yet

The desktop launcher GUI. The verifier, resolver, installer core, metadata
extractors, and install adapters are built; only the toolkit choice and its
prototype remain, deliberately, until installer requirements are known.
