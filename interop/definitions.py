"""Game, loader, and runtime definition parsing for the vector corpus."""

from fields import (
    Fields,
    array,
    artifact_ref,
    blob,
    boolean,
    expect_type,
    i64,
    one_of,
    predicate,
    text,
    text_array,
    u32,
)
from reject import Reject

ORDERING_SCHEMES = {"semver", "ordered-list", "calendar", "opaque"}
RUNTIME_KINDS = {"java", "dotnet", "node", "native", "other"}
QUALIFICATIONS = {"native", "most", "experimental", "untested"}


def version_syntax(value):
    fields = Fields("VersionSyntax", value).known(["kind", "pattern"])
    one_of(fields.required("kind"), "kind", ORDERING_SCHEMES)
    if fields.optional("pattern") is not None:
        text(fields.optional("pattern"), "pattern")


def definition_category(value):
    fields = Fields("Category", value).known(["id", "label", "parent"])
    if text(fields.required("id"), "id") == "":
        raise Reject("invalid-field-value", "id")
    text(fields.required("label"), "label")
    if fields.optional("parent") is not None:
        text(fields.optional("parent"), "parent")
    return fields.value.get("id")


def definition_tag(value):
    fields = Fields("Tag", value).known(["id", "label"])
    if text(fields.required("id"), "id") == "":
        raise Reject("invalid-field-value", "id")
    text(fields.required("label"), "label")
    return fields.value.get("id")


def reject_duplicates(ids, key):
    seen = set()
    for item in ids:
        if item in seen:
            raise Reject("invalid-field-value", key)
        seen.add(item)


def version_catalog(value, name="version_catalog"):
    if value is None:
        return []
    entries = text_array(value, name)
    if any(entry == "" for entry in entries):
        raise Reject("invalid-field-value", name)
    reject_duplicates(entries, name)
    return entries


def parse_game_def(value):
    fields = Fields("GameDef", value).known(
        [
            "protocol",
            "game_id",
            "display_name",
            "version_syntax",
            "version_ordering",
            "version_catalog",
            "loaders_allowed",
            "loader_authorities",
            "categories",
            "tags",
            "metadata_extractor",
            "install_adapter",
            "declared_time",
        ]
    )
    protocol = u32(fields.required("protocol"), "protocol")
    game_id = text(fields.required("game_id"), "game_id")
    display_name = text(fields.required("display_name"), "display_name")
    version_syntax(fields.required("version_syntax"))
    ordering = one_of(
        fields.required("version_ordering"), "version_ordering", ORDERING_SCHEMES
    )
    version_catalog(fields.optional("version_catalog"))
    boolean(fields.required("loaders_allowed"), "loaders_allowed")
    text_array(fields.required("loader_authorities"), "loader_authorities")
    categories = [
        definition_category(item)
        for item in array(fields.required("categories"), "categories")
    ]
    tags = [definition_tag(item) for item in array(fields.required("tags"), "tags")]
    if fields.optional("metadata_extractor") is not None:
        text(fields.optional("metadata_extractor"), "metadata_extractor")
    if fields.optional("install_adapter") is not None:
        text(fields.optional("install_adapter"), "install_adapter")
    i64(fields.required("declared_time"), "declared_time")
    if protocol != 1:
        raise Reject("invalid-field-value", "protocol")
    if game_id == "" or display_name == "":
        raise Reject("invalid-field-value", "game_id")
    if ordering is None:
        raise Reject("invalid-field-value", "version_ordering")
    reject_duplicates(categories, "categories")
    reject_duplicates(tags, "tags")


def parse_runtime_def(value):
    fields = Fields("RuntimeDef", value).known(
        [
            "protocol",
            "runtime_id",
            "kind",
            "display_name",
            "version_ordering",
            "version_catalog",
            "declared_time",
        ]
    )
    protocol = u32(fields.required("protocol"), "protocol")
    runtime_id = text(fields.required("runtime_id"), "runtime_id")
    one_of(fields.required("kind"), "kind", RUNTIME_KINDS)
    text(fields.required("display_name"), "display_name")
    one_of(fields.required("version_ordering"), "version_ordering", ORDERING_SCHEMES)
    version_catalog(fields.optional("version_catalog"))
    i64(fields.required("declared_time"), "declared_time")
    if protocol != 1:
        raise Reject("invalid-field-value", "protocol")
    if runtime_id == "":
        raise Reject("invalid-field-value", "runtime_id")


def declared_by(value):
    fields = Fields("DeclaredBy", value).known(["kind", "id"])
    one_of(fields.required("kind"), "kind", {"loader-authority", "project", "tester"})
    if text(fields.required("id"), "id") == "":
        raise Reject("invalid-field-value", "kind")


def parse_loader_object(value):
    from cbor import Map

    if not isinstance(value, Map):
        raise Reject("invalid-field-type", "LoaderObject")
    discriminant = value.get("type")
    if type(discriminant) is not str:
        raise Reject("missing-field", "type")
    if discriminant == "definition":
        loader_definition(value)
        return
    if discriminant == "release":
        loader_release(value)
        return
    if discriminant == "mapping":
        loader_acceptance(value)
        return
    raise Reject("invalid-field-value", "type")


def loader_definition(value):
    fields = Fields("LoaderDef", value).known(
        [
            "protocol",
            "type",
            "loader_id",
            "game_id",
            "display_name",
            "version_ordering",
            "version_catalog",
            "game_versions",
            "bootstrap",
            "accepted_artifacts",
            "declared_time",
        ]
    )
    expect_type(fields, "definition")
    protocol = u32(fields.required("protocol"), "protocol")
    loader_id = text(fields.required("loader_id"), "loader_id")
    text(fields.required("game_id"), "game_id")
    text(fields.required("display_name"), "display_name")
    ordering = one_of(
        fields.required("version_ordering"), "version_ordering", ORDERING_SCHEMES
    )
    version_catalog(fields.optional("version_catalog"))
    if fields.optional("game_versions") is not None:
        predicate(fields.optional("game_versions"))
    if fields.optional("bootstrap") is not None:
        artifact_ref(fields.optional("bootstrap"))
    if fields.optional("accepted_artifacts") is not None:
        text_array(fields.optional("accepted_artifacts"), "accepted_artifacts")
    i64(fields.required("declared_time"), "declared_time")
    if protocol != 1:
        raise Reject("invalid-field-value", "protocol")
    if loader_id == "":
        raise Reject("invalid-field-value", "loader_id")
    if ordering is None:
        raise Reject("invalid-field-value", "version_ordering")


def loader_release(value):
    fields = Fields("LoaderRelease", value).known(
        [
            "protocol",
            "type",
            "loader_id",
            "version_id",
            "game_version_predicate",
            "runtime_id",
            "runtime_predicate",
            "bootstrap",
            "declared_time",
        ]
    )
    expect_type(fields, "release")
    protocol = u32(fields.required("protocol"), "protocol")
    loader_id = text(fields.required("loader_id"), "loader_id")
    version_id = text(fields.required("version_id"), "version_id")
    predicate(fields.required("game_version_predicate"))
    if fields.optional("runtime_id") is not None:
        text(fields.optional("runtime_id"), "runtime_id")
    if fields.optional("runtime_predicate") is not None:
        predicate(fields.optional("runtime_predicate"))
    if fields.optional("bootstrap") is not None:
        artifact_ref(fields.optional("bootstrap"))
    i64(fields.required("declared_time"), "declared_time")
    if protocol != 1:
        raise Reject("invalid-field-value", "protocol")
    if loader_id == "" or version_id == "":
        raise Reject("invalid-field-value", "version_id")


def loader_acceptance(value):
    fields = Fields("LoaderAcceptance", value).known(
        [
            "protocol",
            "type",
            "accepting_loader_id",
            "accepted_loader_id",
            "game_id",
            "game_version_predicate",
            "loader_version_predicate",
            "accepted_version_predicate",
            "qualification",
            "declared_by",
            "evidence_digest",
            "declared_time",
        ]
    )
    expect_type(fields, "mapping")
    protocol = u32(fields.required("protocol"), "protocol")
    accepting = text(fields.required("accepting_loader_id"), "accepting_loader_id")
    accepted = text(fields.required("accepted_loader_id"), "accepted_loader_id")
    text(fields.required("game_id"), "game_id")
    for name in (
        "game_version_predicate",
        "loader_version_predicate",
        "accepted_version_predicate",
    ):
        if fields.optional(name) is not None:
            predicate(fields.optional(name))
    one_of(fields.required("qualification"), "qualification", QUALIFICATIONS)
    declared_by(fields.required("declared_by"))
    if fields.optional("evidence_digest") is not None:
        raw = blob(fields.optional("evidence_digest"), "evidence_digest")
        if len(raw) != 32:
            raise Reject("invalid-field-value", "evidence_digest")
    i64(fields.required("declared_time"), "declared_time")
    if protocol != 1:
        raise Reject("invalid-field-value", "protocol")
    if accepting == "" or accepted == "":
        raise Reject("invalid-field-value", "accepting_loader_id")
