# Changelog

This is a preview. The signed formats, the API, and the website are still
moving, and things will break without a compatibility shim. Entries here
summarize a release; the commit history is the detailed record.

## 0.1.0-alpha — unreleased

First public preview of the protocol, the server, and the website.

### Protocol

Deterministic CBOR canonical encoding, object kinds and domain separation,
project genesis, delegation and ownership transfer, release payloads, feed
entries, profiles, changelogs, modpacks, advisories, deny and advisory lists,
attestations, key recovery, migration records, location and mirror commitments,
game/loader/runtime definitions with version tables, loader acceptance
mappings, the moderation taxonomy, notifications and webhook payloads, and the
search response. A 68-vector corpus covers the frozen objects and predicates,
and a separate Python checker reproduces every verdict without sharing code
with the Rust implementation.

### Server

`moraine-server` hosts projects, feeds, definitions, blobs with range support,
and federation. It has first-party sessions, scoped revocable API keys,
organizations with nested teams and roles, admission review with a
`review`/`open` switch, profiles with history, faceted search with instance-local
popularity, mirrors and health checks, follows and signed webhooks, legal and
takedown records, sanctions, impersonation claims, directory policy, evidence
attestations, recovery and migration, signed deny lists, a Postgres and
SQLite schema with tested migrations, local and S3-compatible blob storage,
backup, restore, and verification, per-instance and per-account quotas, and a
Prometheus metrics endpoint with collector profiles for Grafana and SigNoz.

### Website

`web` browses and searches games, loaders, runtimes, and projects; shows a
release with its artifacts, fingerprint checks, declared compatibility,
attributed evidence, and release notes; hosts the publishing console and
submission and review flows; and manages organizations, teams, follows, and an
account.

### Tooling

`moraine-publish` authors and signs releases, profiles, definitions, and
attestations, including readable TOML definitions compiled into signed
objects. `moraine-verify` checks objects, artifacts, and the vector corpus.
`moraine-resolver` resolves a lockfile deterministically. `moraine-launcher`
verifies and applies a lockfile through a game adapter, and `moraine-install`
and `moraine-metadata` carry the per-game placement and manifest reading.
