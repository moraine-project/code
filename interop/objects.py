"""Signed-object parsing and validation for the vector corpus."""

import hashlib

from cbor import Map
from definitions import parse_game_def, parse_loader_object, parse_runtime_def
from fields import (
    Fields,
    array,
    artifact,
    artifact_ref,
    blob,
    boolean,
    compatibility,
    dependency,
    derive_key_id,
    expect_type,
    i64,
    key_id32,
    location,
    one_of,
    predicate,
    rights,
    root_key,
    SIDES,
    text,
    text_array,
    u32,
    u64,
    validate_primary,
)
from reject import Reject

GENESIS_KINDS = {
    "project": ["delegation", "release", "profile"],
    "game": ["delegation", "game-def"],
    "loader": ["delegation", "loader-def"],
    "runtime": ["delegation", "runtime-def"],
}
DELEGATION_PURPOSES = {"key", "ownership-transfer", "migration", "recovery"}
SEVERITIES = {"info", "low", "moderate", "high", "critical"}
CATEGORIES = {
    "malware",
    "vulnerability",
    "known-incompatibility",
    "privacy",
    "policy",
    "other",
}
ATTESTATION_KINDS = {
    "build-provenance",
    "review",
    "scanner-result",
    "sbom",
    "compatibility-test",
}
TARGET_KINDS = {"project", "loader", "runtime"}
SCOPES = {"instance", "project", "release-digest", "account"}
WITHDRAWAL_REASONS = {"compromise", "harmful", "broken", "legal", "author-preference"}


def parse_object(kind, value):
    return {
        "genesis": parse_genesis,
        "delegation": parse_delegation,
        "release": parse_release,
        "feed-entry": parse_feed,
        "profile": parse_profile,
        "changelog": parse_changelog,
        "modpack": parse_modpack,
        "deny-list": parse_deny_list,
        "advisory": parse_advisory,
        "attestation": parse_attestation,
        "game-def": parse_game_def,
        "loader-def": parse_loader_object,
        "runtime-def": parse_runtime_def,
    }[kind](value)


def parse_genesis(value):
    fields = Fields("Genesis", value).known(
        [
            "protocol",
            "kind",
            "nonce",
            "roots",
            "threshold",
            "authorized_kinds",
            "home_hint",
            "contacts",
            "created_at",
        ]
    )
    kind = one_of(fields.required("kind"), "kind", set(GENESIS_KINDS))
    roots = [root_key(item) for item in array(fields.required("roots"), "roots")]
    contacts = (
        text_array(fields.optional("contacts"), "contacts")
        if fields.optional("contacts") is not None
        else None
    )
    genesis = {
        "protocol": u32(fields.required("protocol"), "protocol"),
        "kind": kind,
        "nonce": blob(fields.required("nonce"), "nonce"),
        "roots": roots,
        "threshold": u32(fields.required("threshold"), "threshold"),
        "authorized_kinds": text_array(
            fields.required("authorized_kinds"), "authorized_kinds"
        ),
        "home_hint": text(fields.optional("home_hint"), "home_hint")
        if fields.optional("home_hint") is not None
        else None,
        "contacts": contacts,
        "created_at": i64(fields.required("created_at"), "created_at"),
    }
    validate_genesis(genesis)
    return genesis


def validate_genesis(genesis):
    if genesis["protocol"] != 1:
        raise Reject("invalid-field-value", "protocol")
    if len(genesis["nonce"]) < 16:
        raise Reject("invalid-field-value", "nonce must be at least 16 bytes")
    if not genesis["roots"]:
        raise Reject("invalid-field-value", "genesis must list at least one root key")
    seen = set()
    for root in genesis["roots"]:
        if derive_key_id(root["public_key"]) != root["key_id"]:
            raise Reject(
                "invalid-field-value", "root key_id does not match its public key"
            )
        if root["key_id"] in seen:
            raise Reject("duplicate-key", "duplicate root key id")
        seen.add(root["key_id"])
    threshold = genesis["threshold"]
    if threshold == 0 or threshold > len(genesis["roots"]):
        raise Reject(
            "invalid-field-value", "threshold must be between 1 and the number of roots"
        )
    for required in GENESIS_KINDS[genesis["kind"]]:
        if required not in genesis["authorized_kinds"]:
            raise Reject("genesis-kind-requirement", f"missing {required}")


def parse_delegation(value):
    if not isinstance(value, Map):
        raise Reject("invalid-field-type", "Delegation")
    purpose = value.get("purpose")
    if type(purpose) is not str:
        raise Reject("missing-field", "purpose")
    if purpose not in DELEGATION_PURPOSES:
        raise Reject("invalid-field-value", "purpose")
    if purpose == "key":
        return ("key", key_delegation(value))
    if purpose == "ownership-transfer":
        ownership_transfer(value)
        return ("ownership-transfer", {})
    if purpose == "migration":
        migration(value)
        return ("migration", {})
    recovery(value)
    return ("recovery", {})


def key_delegation(value):
    fields = Fields("KeyDelegation", value).known(
        [
            "protocol",
            "purpose",
            "project_id",
            "delegate_key",
            "allowed_kinds",
            "channels",
            "max_version_scope",
            "valid_from_seq",
            "expires_at",
            "issued_at",
            "previous_delegation_digest",
        ]
    )
    if text(fields.required("purpose"), "purpose") != "key":
        raise Reject("invalid-field-value", "purpose")
    delegate = root_key(fields.required("delegate_key"))
    if derive_key_id(delegate["public_key"]) != delegate["key_id"]:
        raise Reject(
            "invalid-field-value", "delegate key_id does not match its public key"
        )
    protocol = u32(fields.required("protocol"), "protocol")
    text(fields.required("project_id"), "project_id")
    allowed_kinds = text_array(fields.required("allowed_kinds"), "allowed_kinds")
    if fields.optional("channels") is not None:
        text_array(fields.optional("channels"), "channels")
    if fields.optional("max_version_scope") is not None:
        text(fields.optional("max_version_scope"), "max_version_scope")
    if fields.optional("valid_from_seq") is not None:
        u64(fields.optional("valid_from_seq"), "valid_from_seq")
    if fields.optional("expires_at") is not None:
        i64(fields.optional("expires_at"), "expires_at")
    i64(fields.required("issued_at"), "issued_at")
    if fields.optional("previous_delegation_digest") is not None:
        blob(
            fields.optional("previous_delegation_digest"), "previous_delegation_digest"
        )
    if protocol != 1:
        raise Reject("invalid-field-value", "protocol")
    if not allowed_kinds:
        raise Reject("invalid-field-value", "allowed_kinds")
    return {"allowed_kinds": allowed_kinds}


def owner_ref(value):
    fields = Fields("OwnerRef", value).known(["kind", "id"])
    kind = one_of(fields.required("kind"), "kind", {"user", "org"})
    owner = text(fields.required("id"), "id")
    if owner == "":
        raise Reject("invalid-field-value", "id")
    return (kind, owner)


def ownership_transfer(value):
    fields = Fields("OwnershipTransfer", value).known(
        [
            "protocol",
            "purpose",
            "project_id",
            "from_owner",
            "to_owner",
            "issued_at",
            "previous_delegation_digest",
        ]
    )
    if text(fields.required("purpose"), "purpose") != "ownership-transfer":
        raise Reject("invalid-field-value", "purpose")
    protocol = u32(fields.required("protocol"), "protocol")
    text(fields.required("project_id"), "project_id")
    from_owner = owner_ref(fields.required("from_owner"))
    to_owner = owner_ref(fields.required("to_owner"))
    i64(fields.required("issued_at"), "issued_at")
    if fields.optional("previous_delegation_digest") is not None:
        blob(
            fields.optional("previous_delegation_digest"), "previous_delegation_digest"
        )
    if protocol != 1:
        raise Reject("invalid-field-value", "protocol")
    if from_owner == to_owner:
        raise Reject("invalid-field-value", "to_owner")


def migration(value):
    fields = Fields("Migration", value).known(
        [
            "protocol",
            "purpose",
            "project_id",
            "old_home",
            "new_home",
            "cutover_seq",
            "reason",
            "declared_time",
        ]
    )
    if text(fields.required("purpose"), "purpose") != "migration":
        raise Reject("invalid-field-value", "purpose")
    protocol = u32(fields.required("protocol"), "protocol")
    text(fields.required("project_id"), "project_id")
    old_home = text(fields.required("old_home"), "old_home")
    new_home = text(fields.required("new_home"), "new_home")
    u64(fields.required("cutover_seq"), "cutover_seq")
    if fields.optional("reason") is not None:
        text(fields.optional("reason"), "reason")
    i64(fields.required("declared_time"), "declared_time")
    if protocol != 1:
        raise Reject("invalid-field-value", "protocol")
    if old_home == "" or new_home == "" or old_home == new_home:
        raise Reject("invalid-field-value", "new_home")


def recovery(value):
    fields = Fields("RecoveryEvent", value).known(
        [
            "protocol",
            "purpose",
            "project_id",
            "compromised_key_ids",
            "valid_from_seq",
            "replacement_roots",
            "affected_release_window",
            "reason",
            "declared_time",
        ]
    )
    if text(fields.required("purpose"), "purpose") != "recovery":
        raise Reject("invalid-field-value", "purpose")
    protocol = u32(fields.required("protocol"), "protocol")
    text(fields.required("project_id"), "project_id")
    compromised = [
        key_id32(item, "compromised_key_ids")
        for item in array(fields.required("compromised_key_ids"), "compromised_key_ids")
    ]
    u64(fields.required("valid_from_seq"), "valid_from_seq")
    replacements = [
        root_key(item)
        for item in array(fields.required("replacement_roots"), "replacement_roots")
    ]
    release_window(fields.required("affected_release_window"))
    text(fields.required("reason"), "reason")
    i64(fields.required("declared_time"), "declared_time")
    if protocol != 1:
        raise Reject("invalid-field-value", "protocol")
    if not compromised:
        raise Reject("invalid-field-value", "compromised_key_ids")
    if not replacements:
        raise Reject("invalid-field-value", "replacement_roots")
    for root in replacements:
        if derive_key_id(root["public_key"]) != root["key_id"]:
            raise Reject(
                "invalid-field-value",
                "replacement root key_id does not match its public key",
            )


def release_window(value):
    fields = Fields("ReleaseWindow", value).known(["from_seq", "to_seq"])
    start = u64(fields.required("from_seq"), "from_seq")
    end = u64(fields.required("to_seq"), "to_seq")
    if start > end:
        raise Reject("invalid-field-value", "from_seq")


def parse_release(value):
    if not isinstance(value, Map):
        raise Reject("invalid-field-type", "ReleaseObject")
    discriminant = value.get("type")
    if type(discriminant) is not str:
        raise Reject("missing-field", "type")
    if discriminant == "release":
        release_payload(value)
        return
    if discriminant == "location":
        location_record(value)
        return
    if discriminant == "withdrawal":
        withdrawal(value)
        return
    raise Reject("invalid-field-value", "type")


def release_payload(value):
    fields = Fields("ReleasePayload", value).known(
        [
            "protocol",
            "type",
            "project_id",
            "game_id",
            "release_nonce",
            "human_version",
            "channel",
            "kind",
            "declared_time",
            "compatibility",
            "artifacts",
            "dependencies",
            "source_reference",
            "changelog_digest",
            "license_expression",
            "rights",
            "sbom_digest",
            "minimum_verifier_version",
            "critical_extensions",
        ]
    )
    expect_type(fields, "release")
    protocol = u32(fields.required("protocol"), "protocol")
    project_id = text(fields.required("project_id"), "project_id")
    game_id = text(fields.required("game_id"), "game_id")
    nonce = blob(fields.required("release_nonce"), "release_nonce")
    text(fields.required("human_version"), "human_version")
    text(fields.required("channel"), "channel")
    text(fields.required("kind"), "kind")
    i64(fields.required("declared_time"), "declared_time")
    for item in array(fields.required("compatibility"), "compatibility"):
        compatibility(item)
    artifacts = [
        artifact(item) for item in array(fields.required("artifacts"), "artifacts")
    ]
    for item in array(fields.required("dependencies"), "dependencies"):
        dependency(item)
    if fields.optional("source_reference") is not None:
        text(fields.optional("source_reference"), "source_reference")
    for name in ("changelog_digest", "sbom_digest"):
        if fields.optional(name) is not None:
            blob(fields.optional(name), name)
    if fields.optional("license_expression") is not None:
        text(fields.optional("license_expression"), "license_expression")
    if fields.optional("rights") is not None:
        rights(fields.optional("rights"))
    u32(fields.required("minimum_verifier_version"), "minimum_verifier_version")
    extensions = text_array(
        fields.required("critical_extensions"), "critical_extensions"
    )
    if protocol != 1:
        raise Reject("invalid-field-value", "protocol")
    if len(nonce) < 16:
        raise Reject("invalid-field-value", "release_nonce must be at least 16 bytes")
    if project_id == "":
        raise Reject("invalid-field-value", "project_id")
    if game_id == "":
        raise Reject("invalid-field-value", "game_id")
    if not artifacts:
        raise Reject("invalid-field-value", "release must list at least one artifact")
    if extensions:
        raise Reject(
            "unknown-critical-extension",
            f"unrecognized critical extension `{extensions[0]}`",
        )
    validate_primary(artifacts)


def location_record(value):
    fields = Fields("LocationRecord", value).known(
        ["protocol", "type", "artifact_digest", "locations", "declared_time"]
    )
    expect_type(fields, "location")
    protocol = u32(fields.required("protocol"), "protocol")
    digest = blob(fields.required("artifact_digest"), "artifact_digest")
    locations = array(fields.required("locations"), "locations")
    i64(fields.required("declared_time"), "declared_time")
    if protocol != 1:
        raise Reject("invalid-field-value", "protocol")
    if len(digest) != 32:
        raise Reject("invalid-field-value", "artifact_digest")
    if not locations:
        raise Reject("invalid-field-value", "locations")
    for item in locations:
        location(item)


def withdrawal(value):
    fields = Fields("Withdrawal", value).known(
        ["protocol", "type", "release_id", "reason", "note", "declared_time"]
    )
    expect_type(fields, "withdrawal")
    protocol = u32(fields.required("protocol"), "protocol")
    if text(fields.required("release_id"), "release_id") == "":
        raise Reject("invalid-field-value", "release_id")
    one_of(fields.required("reason"), "reason", WITHDRAWAL_REASONS)
    if fields.optional("note") is not None:
        text(fields.optional("note"), "note")
    i64(fields.required("declared_time"), "declared_time")
    if protocol != 1:
        raise Reject("invalid-field-value", "protocol")


def parse_feed(value):
    fields = Fields("FeedEntry", value).known(
        [
            "protocol",
            "project_id",
            "sequence",
            "previous",
            "kind",
            "object_digest",
            "declared_at",
        ]
    )
    entry = {
        "protocol": u32(fields.required("protocol"), "protocol"),
        "project_id": text(fields.required("project_id"), "project_id"),
        "sequence": u64(fields.required("sequence"), "sequence"),
        "previous": blob(fields.optional("previous"), "previous")
        if fields.optional("previous") is not None
        else None,
        "kind": text(fields.required("kind"), "kind"),
        "object_digest": blob(fields.required("object_digest"), "object_digest"),
        "declared_at": i64(fields.required("declared_at"), "declared_at"),
    }
    validate_feed(entry)
    return entry


def validate_feed(entry):
    if entry["protocol"] != 1:
        raise Reject("invalid-field-value", "protocol")
    if entry["sequence"] == 0:
        raise Reject("invalid-field-value", "sequence")
    if (
        entry["sequence"] == 1
        and entry["previous"] is not None
        and len(entry["previous"]) == 32
    ):
        raise Reject("previous-mismatch", "sequence 1 must not carry a previous digest")
    if not (entry["sequence"] == 1 and entry["previous"] is None):
        if entry["previous"] is None:
            raise Reject(
                "previous-mismatch",
                "entry after sequence 1 must carry a previous digest",
            )
        if len(entry["previous"]) != 32:
            raise Reject("invalid-field-value", "previous")
    if len(entry["object_digest"]) != 32:
        raise Reject("invalid-field-value", "object_digest")
    if entry["kind"] == "":
        raise Reject("invalid-field-value", "kind")


def feed_follows(entry, previous, previous_bytes):
    if entry["sequence"] != previous["sequence"] + 1:
        raise Reject("sequence-gap", "sequence is not the previous sequence plus one")
    expected = hashlib.sha256(b"GAMEDIST/v1/feed-entry\x00" + previous_bytes).digest()
    if entry["previous"] != expected:
        raise Reject(
            "previous-mismatch", "previous digest does not match the prior entry"
        )


def parse_profile(value):
    fields = Fields("ProfileRevision", value).known(
        [
            "protocol",
            "project_id",
            "game_id",
            "revision_nonce",
            "display_name",
            "summary",
            "description",
            "icon",
            "gallery",
            "links",
            "communities",
            "categories",
            "tags",
            "rights",
            "declared_time",
        ]
    )
    protocol = u32(fields.required("protocol"), "protocol")
    project_id = text(fields.required("project_id"), "project_id")
    game_id = text(fields.required("game_id"), "game_id")
    nonce = blob(fields.required("revision_nonce"), "revision_nonce")
    display_name = text(fields.required("display_name"), "display_name")
    text(fields.required("summary"), "summary")
    text(fields.required("description"), "description")
    if fields.optional("icon") is not None:
        artifact_ref(fields.optional("icon"))
    for item in array(fields.required("gallery"), "gallery"):
        artifact_ref(item)
    for name in ("links", "communities"):
        for item in array(fields.required(name), name):
            link(item)
    text_array(fields.required("categories"), "categories")
    text_array(fields.required("tags"), "tags")
    if fields.optional("rights") is not None:
        rights(fields.optional("rights"))
    i64(fields.required("declared_time"), "declared_time")
    if protocol != 1:
        raise Reject("invalid-field-value", "protocol")
    if len(nonce) < 16:
        raise Reject("invalid-field-value", "revision_nonce must be at least 16 bytes")
    if project_id == "":
        raise Reject("invalid-field-value", "project_id")
    if game_id == "":
        raise Reject("invalid-field-value", "game_id")
    if display_name == "":
        raise Reject("invalid-field-value", "display_name")


def parse_changelog(value):
    fields = Fields("Changelog", value).known(
        ["protocol", "project_id", "release_id", "locale_sections", "declared_time"]
    )
    protocol = u32(fields.required("protocol"), "protocol")
    project_id = text(fields.required("project_id"), "project_id")
    if fields.optional("release_id") is not None:
        text(fields.optional("release_id"), "release_id")
    locales = array(fields.required("locale_sections"), "locale_sections")
    if not locales:
        raise Reject("invalid-field-value", "locale_sections")
    for locale_value in locales:
        locale = Fields("LocaleSection", locale_value).known(["locale", "sections"])
        if text(locale.required("locale"), "locale") == "":
            raise Reject("invalid-field-value", "locale")
        for section_value in array(locale.required("sections"), "sections"):
            section = Fields("ChangelogSection", section_value).known(
                ["heading", "body", "severity"]
            )
            text(section.required("heading"), "heading")
            text(section.required("body"), "body")
            if section.optional("severity") is not None:
                text(section.optional("severity"), "severity")
    i64(fields.required("declared_time"), "declared_time")
    if protocol != 1 or project_id == "":
        raise Reject("invalid-field-value", "protocol" if protocol != 1 else "project_id")


def parse_modpack(value):
    fields = Fields("ModpackManifest", value).known(
        [
            "protocol", "project_id", "game_id", "loader_id", "entries",
            "overrides", "server_manifest_digest", "declared_time",
        ]
    )
    protocol = u32(fields.required("protocol"), "protocol")
    project_id = text(fields.required("project_id"), "project_id")
    game_id = text(fields.required("game_id"), "game_id")
    if fields.optional("loader_id") is not None:
        text(fields.optional("loader_id"), "loader_id")
    entries = array(fields.required("entries"), "entries")
    if not entries:
        raise Reject("invalid-field-value", "entries")
    ordinals = set()
    for entry_value in entries:
        entry = Fields("ModpackEntry", entry_value).known(
            ["ordinal", "target_kind", "target_id", "release_id", "digest", "applies_to"]
        )
        ordinal = u32(entry.required("ordinal"), "ordinal")
        if ordinal in ordinals:
            raise Reject("invalid-field-value", "ordinal")
        ordinals.add(ordinal)
        one_of(entry.required("target_kind"), "target_kind", TARGET_KINDS)
        text(entry.required("target_id"), "target_id")
        text(entry.required("release_id"), "release_id")
        if len(blob(entry.required("digest"), "digest")) != 32:
            raise Reject("invalid-field-value", "digest")
        one_of(entry.required("applies_to"), "applies_to", SIDES)
    for override_value in array(fields.required("overrides"), "overrides"):
        override = Fields("ModpackOverride", override_value).known(
            ["digest", "target_path", "applies_to"]
        )
        if len(blob(override.required("digest"), "digest")) != 32:
            raise Reject("invalid-field-value", "digest")
        path = text(override.required("target_path"), "target_path")
        if (
            not path
            or path.startswith(("/", "\\"))
            or ":" in path
            or any(not part or part in {".", ".."} for part in path.split("/"))
        ):
            raise Reject("invalid-field-value", "target_path")
        one_of(override.required("applies_to"), "applies_to", SIDES)
    if fields.optional("server_manifest_digest") is not None and len(
        blob(fields.optional("server_manifest_digest"), "server_manifest_digest")
    ) != 32:
        raise Reject("invalid-field-value", "server_manifest_digest")
    i64(fields.required("declared_time"), "declared_time")
    if protocol != 1 or not project_id or not game_id:
        raise Reject("invalid-field-value", "protocol" if protocol != 1 else "project_id")


def parse_deny_list(value):
    fields = Fields("DenyList", value).known(
        ["protocol", "issuer_id", "entries", "issued_at"]
    )
    protocol = u32(fields.required("protocol"), "protocol")
    issuer_id = text(fields.required("issuer_id"), "issuer_id")
    entries = array(fields.required("entries"), "entries")
    if not entries or len(entries) > 1000:
        raise Reject("invalid-field-value", "entries")
    for index, entry_value in enumerate(entries):
        entry = Fields("DenyListEntry", entry_value).known(
            [
                "target_kind", "target_id", "reason_code", "reason_taxonomy_version",
                "scope_kind", "scope_id", "valid_from", "valid_until",
            ]
        )
        target_kind = one_of(entry.required("target_kind"), "target_kind", {"project", "artifact-digest"})
        target_id = text(entry.required("target_id"), "target_id")
        if not target_id.strip():
            raise Reject("invalid-field-value", f"entries[{index}].target_id")
        if target_kind == "project" and not target_id.startswith("gd:sha256:"):
            raise Reject("invalid-field-value", f"entries[{index}].target_id")
        if target_kind == "artifact-digest":
            digest = target_id.removeprefix("sha256:")
            if len(digest) != 64 or any(char not in "0123456789abcdefABCDEF" for char in digest):
                raise Reject("invalid-field-value", f"entries[{index}].target_id")
        if not text(entry.required("reason_code"), "reason_code").strip():
            raise Reject("invalid-field-value", f"entries[{index}].reason_code")
        u32(entry.required("reason_taxonomy_version"), "reason_taxonomy_version")
        one_of(entry.required("scope_kind"), "scope_kind", SCOPES)
        if not text(entry.required("scope_id"), "scope_id").strip():
            raise Reject("invalid-field-value", f"entries[{index}].scope_id")
        valid_from = i64(entry.optional("valid_from"), "valid_from") if entry.optional("valid_from") is not None else None
        valid_until = i64(entry.optional("valid_until"), "valid_until") if entry.optional("valid_until") is not None else None
        if valid_from is not None and valid_until is not None and valid_until <= valid_from:
            raise Reject("invalid-field-value", f"entries[{index}].valid_until")
    i64(fields.required("issued_at"), "issued_at")
    if protocol != 1 or not issuer_id.strip():
        raise Reject("invalid-field-value", "protocol" if protocol != 1 else "issuer_id")


def link(value):
    fields = Fields("Link", value).known(["kind", "url"])
    text(fields.required("kind"), "kind")
    text(fields.required("url"), "url")


def parse_advisory(value):
    fields = Fields("Advisory", value).known(
        [
            "protocol",
            "provider_id",
            "project_id",
            "game_id",
            "affected",
            "severity",
            "category",
            "taxonomy_version",
            "block_promotion",
            "evidence_ref",
            "published_at",
            "expires_at",
            "retracted_at",
        ]
    )
    protocol = u32(fields.required("protocol"), "protocol")
    provider = text(fields.required("provider_id"), "provider_id")
    project = text(fields.required("project_id"), "project_id")
    game = text(fields.required("game_id"), "game_id")
    affected(fields.required("affected"))
    severity = one_of(fields.required("severity"), "severity", SEVERITIES)
    category = one_of(fields.required("category"), "category", CATEGORIES)
    taxonomy = u32(fields.required("taxonomy_version"), "taxonomy_version")
    block = boolean(fields.required("block_promotion"), "block_promotion")
    if fields.optional("evidence_ref") is not None:
        text(fields.optional("evidence_ref"), "evidence_ref")
    i64(fields.required("published_at"), "published_at")
    expires = (
        i64(fields.optional("expires_at"), "expires_at")
        if fields.optional("expires_at") is not None
        else None
    )
    retracted = (
        i64(fields.optional("retracted_at"), "retracted_at")
        if fields.optional("retracted_at") is not None
        else None
    )
    if protocol != 1:
        raise Reject("invalid-field-value", "protocol")
    if provider == "" or project == "" or game == "":
        raise Reject("invalid-field-value", "provider_id")
    if taxonomy == 0:
        raise Reject("invalid-field-value", "taxonomy_version")
    if block and not (category == "malware" and severity in {"high", "critical"}):
        raise Reject(
            "invalid-field-value",
            "block_promotion is only valid for malware at high or critical severity",
        )
    if retracted is not None and expires is not None and retracted > expires:
        raise Reject("invalid-field-value", "retracted_at")


def affected(value):
    fields = Fields("Affected", value).known(["digest", "predicate"])
    digest = fields.optional("digest")
    predicate_value = fields.optional("predicate")
    if digest is None and predicate_value is None:
        raise Reject("missing-field", "affected")
    if digest is not None:
        raw = blob(digest, "digest")
        if len(raw) != 32:
            raise Reject("invalid-field-value", "digest")
    if predicate_value is not None:
        predicate(predicate_value)


def parse_attestation(value):
    if not isinstance(value, Map):
        raise Reject("invalid-field-type", "AttestationObject")
    discriminant = value.get("type")
    if type(discriminant) is not str:
        raise Reject("missing-field", "type")
    if discriminant == "attestation":
        attestation(value)
        return
    if discriminant == "mirror-commitment":
        mirror_commitment(value)
        return
    raise Reject("invalid-field-value", "type")


def attestation(value):
    fields = Fields("Attestation", value).known(
        [
            "protocol",
            "type",
            "artifact_digest",
            "subject_kind",
            "subject_id",
            "kind",
            "media_type",
            "body_digest",
            "body_inline",
            "signer_id",
            "issued_at",
        ]
    )
    expect_type(fields, "attestation")
    protocol = u32(fields.required("protocol"), "protocol")
    digest = blob(fields.required("artifact_digest"), "artifact_digest")
    one_of(
        fields.required("subject_kind"),
        "subject_kind",
        {"project", "release", "loader-release"},
    )
    text(fields.required("subject_id"), "subject_id")
    one_of(fields.required("kind"), "kind", ATTESTATION_KINDS)
    text(fields.required("media_type"), "media_type")
    body_digest = fields.optional("body_digest")
    body_inline = fields.optional("body_inline")
    if body_digest is not None:
        raw = blob(body_digest, "body_digest")
        if len(raw) != 32:
            raise Reject("invalid-field-value", "body_digest")
    if body_inline is not None:
        blob(body_inline, "body_inline")
    signer = text(fields.required("signer_id"), "signer_id")
    i64(fields.required("issued_at"), "issued_at")
    if protocol != 1:
        raise Reject("invalid-field-value", "protocol")
    if len(digest) != 32:
        raise Reject("invalid-field-value", "artifact_digest")
    if signer == "":
        raise Reject("invalid-field-value", "signer_id")
    if body_digest is not None and body_inline is not None:
        raise Reject("invalid-field-value", "body_digest")


def mirror_commitment(value):
    fields = Fields("MirrorCommitment", value).known(
        [
            "protocol",
            "type",
            "mirror_id",
            "artifact_digest",
            "size",
            "accepted_at",
            "retention_until",
            "endpoint",
        ]
    )
    expect_type(fields, "mirror-commitment")
    protocol = u32(fields.required("protocol"), "protocol")
    mirror = text(fields.required("mirror_id"), "mirror_id")
    digest = blob(fields.required("artifact_digest"), "artifact_digest")
    u64(fields.required("size"), "size")
    i64(fields.required("accepted_at"), "accepted_at")
    if fields.optional("retention_until") is not None:
        i64(fields.optional("retention_until"), "retention_until")
    endpoint = text(fields.required("endpoint"), "endpoint")
    if protocol != 1:
        raise Reject("invalid-field-value", "protocol")
    if mirror == "" or endpoint == "":
        raise Reject("invalid-field-value", "mirror_id")
    if len(digest) != 32:
        raise Reject("invalid-field-value", "artifact_digest")
