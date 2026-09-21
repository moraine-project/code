# Running an instance

An instance is one process, one data directory, and one database. SQLite and
the local filesystem are the default and are enough for a real instance.
PostgreSQL and S3-compatible storage are there for when you outgrow them.

The website ships with the server, so a single deployment serves both the API
and the pages people browse.

## What you need

- A host, a domain, and something that terminates TLS: Caddy, nginx, or a
  cloud load balancer.
- A directory for data on a disk you back up.

TLS is not optional here. The server refuses to bind a non-loopback address
unless you pass `--tls-terminated` to say a proxy is in front of it, or set
`MORAINE_ALLOW_INSECURE_HTTP=true` for a throwaway test. It does not handle
certificates itself; your proxy owns those.

## Install

With the container:

```sh
docker build -t moraine/server .
docker run -d --name moraine \
	-v moraine-data:/data \
	-p 127.0.0.1:8080:8080 \
	moraine/server
```

`deploy/compose/docker-compose.yml` does the same and binds the published port
to localhost so only the host proxy can reach it.
`deploy/systemd/moraine.service` runs the binary as an unprivileged user with a
state directory.

From source:

```sh
cargo build --release -p moraine-server
./target/release/moraine-server \
	--data-dir /var/lib/moraine \
	--bind 127.0.0.1:8080 \
	--tls-terminated
```

Create the first operator account once:

```sh
moraine-server --data-dir /var/lib/moraine bootstrap --email you@example.org
```

The password is printed once. Store it in your password manager now; the server
keeps only a hash and cannot show it to you again.

Then point the proxy at the instance and visit it. The first load runs the
migrations unless `MORAINE_SKIP_MIGRATE_ON_START=true`, in which case run
`moraine-server migrate` yourself as part of the deploy.

## Settings

Every setting has a `MORAINE_*` environment variable and the same flag.

| Variable | Default | Meaning |
| --- | --- | --- |
| `MORAINE_BIND` | `127.0.0.1:8080` | Address to listen on |
| `MORAINE_TLS_TERMINATED` | `false` | A proxy in front terminates TLS |
| `MORAINE_ALLOW_INSECURE_HTTP` | `false` | Serve plain HTTP on a public bind anyway |
| `MORAINE_DATA_DIR` | `./data` | Blobs and instance keys |
| `MORAINE_DATABASE_URL` | unset | PostgreSQL URL; SQLite under the data dir when unset |
| `MORAINE_WEB_DIR` | unset | Serve the built website from this directory |
| `MORAINE_WEB_ORIGINS` | unset | Comma-separated origins allowed to write with a session cookie |
| `MORAINE_S3_BUCKET` | unset | Store blobs in S3 instead of the filesystem |
| `MORAINE_S3_ENDPOINT` | unset | S3 endpoint for non-AWS providers |
| `MORAINE_S3_REGION` | unset | S3 region |
| `MORAINE_S3_ACCESS_KEY_ID` | unset | S3 credential |
| `MORAINE_S3_SECRET_ACCESS_KEY` | unset | S3 credential |
| `MORAINE_S3_PREFIX` | `moraine` | Key prefix inside the bucket |
| `MORAINE_PUBLISHING` | `review` | `review` gates submissions, `progressive` grants automatic publication after an accepted release, `open` does not gate publication |
| `MORAINE_REGISTRATION` | `closed` | `open` lets anyone create an account; `closed` only lets the operator add accounts |
| `MORAINE_OPERATOR_EMAIL` | unset | Used by `bootstrap` |
| `MORAINE_TLS_EXTRA_ROOTS` | unset | PEM bundle trusted in addition to the system roots, for federation |

Retention and limits:

| Variable | Default | Meaning |
| --- | --- | --- |
| `MORAINE_MAX_ARTIFACT_BYTES` | 512 MiB | Largest single artifact |
| `MORAINE_MAX_UPLOAD_BYTES_PER_ACCOUNT` | 5 GiB | Per-account stored bytes; `0` disables |
| `MORAINE_MAX_PROJECTS` | 10000 | Projects on this instance; `0` disables |
| `MORAINE_MAX_DEFINITIONS` | 1000 | Game, loader, and runtime definitions this instance holds; `0` disables |
| `MORAINE_MAX_SYNC_ENTRIES` | 10000 | Loader releases one federation sync will follow |
| `MORAINE_METRICS_TOKEN` | unset | When set, `/metrics` requires `Authorization: Bearer <token>` |
| `MORAINE_SMTP_URL` | unset | SMTP URL (`smtp://user:pass@host:587`); enables email verification |
| `MORAINE_MAIL_FROM` | unset | From address for outgoing mail |
| `MORAINE_PUBLIC_URL` | unset | Public base URL, used to build the verification link |
| `MORAINE_REQUIRE_VERIFIED_EMAIL` | `false` | When true, unverified accounts cannot upload or submit |
| `MORAINE_MAX_FEED_PAGE_ENTRIES` | 100 | Feed page size |
| `MORAINE_MAX_FEED_SCAN_PAGES` | 50 | Pages a feed read will walk |
| `MORAINE_MAX_SYNC_PAGES` | 200 | Pages one federation sync will walk |
| `MORAINE_MAX_CONCURRENT_SYNCS` | 4 | Mirrors syncing at once |
| `MORAINE_MAX_MIRROR_PROBES_PER_CYCLE` | 20 | Mirror probes per maintenance cycle |
| `MORAINE_MAX_MIRROR_PROBE_BYTES` | 256 MiB | Bytes one probe cycle may fetch |
| `MORAINE_MAX_RESPONSE_BYTES` | 16 MiB | Largest inbound response from another home |
| `MORAINE_REQUESTS_PER_MINUTE` | 600 | Per-client request budget |
| `MORAINE_STAGING_RETENTION_SECONDS` | 3600 | Unreferenced staging blobs kept this long |
| `MORAINE_BLOB_RETENTION_SECONDS` | 604800 | Unreferenced blobs kept this long |
| `MORAINE_MAINTENANCE_INTERVAL_SECONDS` | 3600 | Background cycle interval |
| `MORAINE_FEDERATION_ALLOW_HTTP_LOCAL` | `false` | Allow `http://` federation to loopback, for local tests |

Set a limit to `0` to turn that limit off. An operator running a public
instance wants the defaults; an operator running one for a small group may
lower them.

## The dashboard

Operators get a dashboard at `/dashboard` with an overview of the instance,
account management, definition import and federation. The navbar links to it for
operators; members never see it. Accounts are created there, or through
`POST /v1/auth/users`, which returns a temporary password shown once. Accounts
can export their own data and delete themselves from the account page; a
deletion is refused while the account is the last owner of an organization.

## Email verification (optional)

Email is off unless you configure it. Set `MORAINE_SMTP_URL`, `MORAINE_MAIL_FROM`,
and `MORAINE_PUBLIC_URL`, and registration starts sending a verification link
that expires in 24 hours. Without those, accounts are created already verified
and recovery codes cover password reset, so a self-hosted instance needs no mail
server at all. With `MORAINE_REQUIRE_VERIFIED_EMAIL=true`, an unverified account
cannot upload or submit until it follows the link.

## Accounts and roles

The first account is the **operator**, created with `bootstrap`; it holds every
scope and can review submissions, manage federation, set directory policy, and
pin provider and mirror keys. Ordinary accounts are **members**: they can
publish, upload artifacts, submit releases, and mint API keys, but they cannot
reach the operator routes.

Registration is **closed by default**. Set `MORAINE_REGISTRATION=open` to let
people create their own accounts, or keep it closed and create accounts for the
people you trust. Either way, only the operator can grant operator-level
access.

Accounts manage their own password from the account page, and can generate
**recovery codes**: one-time codes, stored hashed, that set a new password
without any email. For a member who never saved codes, the operator has a
fallback: `POST /v1/auth/users/reset-password` issues a temporary password and
signs the account's sessions out. No mail server is required for any of this.

## Storage

SQLite keeps metadata in `<data-dir>/moraine.sqlite3`, and blobs live under
`<data-dir>/blobs` unless S3 is configured. PostgreSQL gets metadata only, and
S3 gets blobs only; the two choices are independent.

Blobs are content-addressed, so the same bytes uploaded twice are stored once.
Nothing is deleted while a signed record references it. Withdrawal, quarantine,
and takedown stop serving and stop indexing; they do not rewrite or delete the
bytes, because a signature over them still exists.

## Backups

```sh
moraine-server --data-dir /var/lib/moraine backup --out /srv/backups/2026-09-20
moraine-server --data-dir /var/lib/moraine verify-backup --dir /srv/backups/2026-09-20
moraine-server --data-dir /var/lib/moraine restore --dir /srv/backups/2026-09-20 --force
```

A backup takes a consistent snapshot of the database and a listing of every
blob it references. It does not include publisher root keys, and it does not
include the instance webhook signing key; copy both separately or you will lose
the ability to sign on behalf of the instance and to verify past webhook
deliveries.

Run the drill. A backup you have never restored is a hope, not a backup.
Restore into an empty data directory, start the instance against it, and fetch
a release with `moraine-verify` before you trust it.

## Monitoring

`GET /metrics` returns Prometheus text: request counts and durations by route,
sessions and API keys created and revoked, staging and blob collections, blob
serve failures, and key-change events. Scrape it; the server does not push.

`deploy/otel/` has two OpenTelemetry Collector profiles, one that writes to
Grafana and one that writes to SigNoz. Pick one at deploy time. A collector is
the only component that needs the backend's credentials, and an instance with
no collector configured sends nothing off the host.

Watch `moraine_request_duration_seconds` for latency, `moraine_key_changes_total`
for key activity, and the collection counters to confirm maintenance is running.

## Hosting the website on another origin

The website ships with the server and is served from it, which is the simplest
setup. If you host it elsewhere, such as a static site on a CDN while the API
runs on a VPS, list the site's origin so the account and upload routes accept
its requests:

```sh
MORAINE_WEB_ORIGINS=https://mods.example,https://preview.mods.example
```

With the list set, the server answers those origins with credentials, switches
the session cookies to `SameSite=None`, returns the CSRF token in the login
response body, and rejects a session write whose `Origin` is neither the
request's host nor a listed origin. Origins not on the list can still read.
Build the site with `PUBLIC_MORAINE_REGISTRY` pointing at the API, for example
`https://api.example`.

Two cautions. Every origin you list can drive a logged-in visitor's session, so
list only origins you operate and keep the list tight. If your proxy rewrites
the `Host` header, the same-origin check cannot match and you must list the
public origin explicitly.

If the site and the API share one hostname, for example a proxy that forwards
`/api` to the server, leave the list empty: requests are same-origin, the
cookies stay `SameSite=Lax`, and none of this applies.

## Game and loader definitions

A game, loader, or runtime is a signed identity, not a name. Its ID is the
digest of its signed genesis, so two instances that each compile the same
authoring file with their own key get *different* IDs, and releases published
against one will not match the other.

The repository ships a **canonical signed set** under `definitions/canonical/`,
with the IDs listed in `definitions/curated.lock`. Importing those objects gives
an instance the same game and loader IDs as every other instance that took them,
so federation matches out of the box:

```sh
moraine-publish define --out definitions/canonical --home https://your-instance
```

Or pull the set from a home that serves it:
`moraine-publish sync-definitions --home … --from … --lock definitions/curated.lock`.
An instance that does not want the default set compiles the TOML itself and gets
local IDs. See `definitions/README.md`.

The simplest path for a custom set is to compile the bundled `definitions/` set
and import it into a running instance in one command. Compiling mints the identities once into
`--out`; importing reads that directory, so running it again is idempotent:

```sh
moraine-publish define --key definitions.key \
  --dir definitions/minecraft --out data/definitions \
  --home https://your-instance
moraine-publish define --out data/definitions --home https://your-instance
```

Edit or delete files under `definitions/` first if you do not want the default
set; the compiler resolves the remaining references by name, so no IDs need to
be pasted anywhere.

Instances can also agree by sharing the same signed genesis. One home authors
the definition and serves it; others pull it and store it under the same ID:

```sh
moraine-publish sync-definition --home https://your-instance \
  --from https://definitions.example --kind game --id gd:sha256:... \
  --api-key "$MORAINE_API_KEY"
```

To pin a whole set, list the IDs and their homes in a lock file and apply it in
one command. `definitions/curated.lock` is a template:

```sh
moraine-publish sync-definitions --home https://your-instance \
  --lock definitions/curated.lock --api-key "$MORAINE_API_KEY"
```

The settings page shows every definition the instance holds, marked **Local**
(authored or imported here) or **Federated** (pulled from the home named on the
row), and can pull one from the UI. Adding a version to a definition is a
revision, not a new identity: set `revision_of` in the authoring file, as
described in `definitions/README.md`.

## Policy

Every instance publishes its own rules: what it will host, how long it keeps
things, how it handles a takedown, and who to contact. `docs/policy-template.md`
is a starting point. Put the finished policy somewhere stable and link it from
the instance, because reporters and publishers will look for it.

## Upgrading

Replace the binary or the image, run `moraine-server migrate`, and restart.
Migrations are forward-only; take a backup first, and if you need to roll back,
restore that backup rather than moving the database backwards. The signed
formats are frozen, so an upgrade does not invalidate anything already stored.
