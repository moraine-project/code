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
search response.

A 68-vector corpus covers the frozen objects and predicates. A separate Python
checker reproduces every verdict without sharing code with the Rust
implementation.

### Server

`moraine-server` hosts projects, feeds, definitions, blobs with range support,
and federation. It has first-party sessions and scoped revocable API keys,
organizations with nested teams and roles, admission review with a
`review`/`open` switch, profiles with history, and faceted search with
instance-local popularity.

Beyond that: mirrors and health checks, follows and signed webhooks, legal and
takedown records, sanctions, impersonation claims, directory policy, evidence
attestations, recovery and migration, and signed deny lists. Storage is a
Postgres and SQLite schema with tested migrations, local or S3-compatible
blobs, backup, restore, and verification. Per-instance and per-account quotas,
and a Prometheus metrics endpoint with collector profiles for Grafana and
SigNoz.

### Website

`web` browses and searches games, loaders, runtimes, and projects. It shows a
release with its artifacts, fingerprint checks, declared compatibility,
attributed evidence, and release notes. It hosts the publishing console and the
submission and review flows, and manages organizations, teams, follows, and an
account.

### Tooling

`moraine-publish` authors and signs releases, profiles, definitions, and
attestations, including readable TOML definitions compiled into signed objects.
`moraine-verify` checks objects, artifacts, and the vector corpus.
`moraine-resolver` resolves a lockfile deterministically. `moraine-launcher`
verifies and applies a lockfile through a game adapter, and `moraine-install`
and `moraine-metadata` carry the per-game placement and manifest reading.
