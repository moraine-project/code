"""Field access and shared sub-structures for the vector corpus."""

import hashlib

from cbor import Map
from reject import Reject

SIDES = {"client", "server", "both"}
TARGET_KINDS = {"project", "loader", "runtime"}
DEPENDENCY_KINDS = {"required", "optional", "embedded", "incompatible", "recommended"}
PERMISSIONS = {"yes", "no", "ask"}
LOCATION_KINDS = {"origin", "mirror", "external"}


def derive_key_id(public_key):
    return hashlib.sha256(bytes([0x01]) + public_key).digest()


class Fields:
    def __init__(self, label, value):
        if not isinstance(value, Map):
            raise Reject("invalid-field-type", label)
        pairs = []
        for key, item in value.pairs:
            if type(key) is not str:
                raise Reject("invalid-field-type", "map key is not text")
            pairs.append((key, item))
        self.pairs = pairs
        self.value = Map(pairs)

    def known(self, allowed):
        for name, _ in self.pairs:
            if name not in allowed:
                raise Reject("unknown-field", name)
        return self

    def required(self, name):
        item = self.value.get(name)
        if item is None:
            raise Reject("missing-field", name)
        return item

    def optional(self, name):
        return self.value.get(name)


def text(value, name):
    if type(value) is not str:
        raise Reject("invalid-field-type", name)
    return value


def blob(value, name):
    if type(value) is not bytes:
        raise Reject("invalid-field-type", name)
    return value


def boolean(value, name):
    if type(value) is not bool:
        raise Reject("invalid-field-type", name)
    return value


def integer(value, name, low, high):
    if type(value) is not int:
        raise Reject("invalid-field-type", name)
    if value < low or value > high:
        raise Reject("invalid-field-value", name)
    return value


def u32(value, name):
    return integer(value, name, 0, 0xFFFFFFFF)


def u64(value, name):
    return integer(value, name, 0, 0x7FFFFFFFFFFFFFFF)


def i64(value, name):
    return integer(value, name, -(2**63), 2**63 - 1)


def array(value, name):
    if type(value) is not list:
        raise Reject("invalid-field-type", name)
    return value


def text_array(value, name):
    return [text(item, name) for item in array(value, name)]


def one_of(value, name, allowed):
    found = text(value, name)
    if found not in allowed:
        raise Reject("invalid-field-value", name)
    return found


def expect_type(fields, expected):
    found = text(fields.required("type"), "type")
    if found != expected:
        raise Reject("invalid-field-value", "type")


def key_id32(value, name):
    raw = blob(value, name)
    if len(raw) != 32:
        raise Reject("invalid-field-value", name)
    return raw


def root_key(value):
    fields = Fields("RootKey", value).known(["key_id", "public_key"])
    return {
        "key_id": key_id32(fields.required("key_id"), "key_id"),
        "public_key": blob(fields.required("public_key"), "public_key"),
    }


def predicate(value):
    fields = Fields("Predicate", value).known(["scheme", "values"])
    return {
        "scheme": text(fields.required("scheme"), "scheme"),
        "values": text_array(fields.required("values"), "values"),
    }


def artifact_ref(value):
    fields = Fields("ArtifactRef", value).known(["digest", "size", "media_type"])
    if len(blob(fields.required("digest"), "digest")) != 32:
        raise Reject("invalid-field-value", "digest")
    return {"size": u64(fields.required("size"), "size")}


def rights(value):
    fields = Fields("Rights", value).known(
        [
            "redistribution",
            "modpack_inclusion",
            "commercial_use",
            "server_use",
            "mirroring",
            "attribution_required",
            "notes",
        ]
    )
    for name in (
        "redistribution",
        "modpack_inclusion",
        "commercial_use",
        "server_use",
        "mirroring",
    ):
        one_of(fields.required(name), name, PERMISSIONS)
    boolean(fields.required("attribution_required"), "attribution_required")
    if fields.optional("notes") is not None:
        text(fields.optional("notes"), "notes")


def compatibility(value):
    fields = Fields("Compatibility", value).known(
        [
            "game_version_predicate",
            "loader_id",
            "loader_version_predicate",
            "side",
            "runtime_predicate",
            "os_predicate",
            "arch_predicate",
        ]
    )
    one_of(fields.required("side"), "side", SIDES)
    predicate(fields.required("game_version_predicate"))
    if fields.optional("loader_id") is not None:
        text(fields.optional("loader_id"), "loader_id")
    if fields.optional("loader_version_predicate") is not None:
        predicate(fields.optional("loader_version_predicate"))
    if fields.optional("runtime_predicate") is not None:
        predicate(fields.optional("runtime_predicate"))
    for name in ("os_predicate", "arch_predicate"):
        if fields.optional(name) is not None:
            text_array(fields.optional(name), name)


def artifact(value):
    fields = Fields("Artifact", value).known(
        [
            "digest",
            "size",
            "media_type",
            "filename",
            "is_primary",
            "os_predicate",
            "arch_predicate",
        ]
    )
    if len(blob(fields.required("digest"), "digest")) != 32:
        raise Reject("invalid-field-value", "digest")
    u64(fields.required("size"), "size")
    text(fields.required("media_type"), "media_type")
    if text(fields.required("filename"), "filename") == "":
        raise Reject("invalid-field-value", "filename")
    primary = boolean(fields.required("is_primary"), "is_primary")
    os_values = (
        text_array(fields.optional("os_predicate"), "os_predicate")
        if fields.optional("os_predicate") is not None
        else []
    )
    arch_values = (
        text_array(fields.optional("arch_predicate"), "arch_predicate")
        if fields.optional("arch_predicate") is not None
        else []
    )
    return {"is_primary": primary, "os": sorted(os_values), "arch": sorted(arch_values)}


def dependency(value):
    fields = Fields("Dependency", value).known(
        ["target_kind", "target_id", "game_id", "predicate", "kind", "applies_to"]
    )
    one_of(fields.required("target_kind"), "target_kind", TARGET_KINDS)
    text(fields.required("target_id"), "target_id")
    text(fields.required("game_id"), "game_id")
    predicate(fields.required("predicate"))
    one_of(fields.required("kind"), "kind", DEPENDENCY_KINDS)
    one_of(fields.required("applies_to"), "applies_to", SIDES)


def location(value):
    fields = Fields("Location", value).known(["url", "kind", "operator_id"])
    if text(fields.required("url"), "url") == "":
        raise Reject("invalid-field-value", "url")
    one_of(fields.required("kind"), "kind", LOCATION_KINDS)
    if fields.optional("operator_id") is not None:
        text(fields.optional("operator_id"), "operator_id")


def validate_primary(artifacts):
    slots = {}
    for item in artifacts:
        slot = slots.setdefault((tuple(item["os"]), tuple(item["arch"])), 0)
        if item["is_primary"]:
            slots[(tuple(item["os"]), tuple(item["arch"]))] = slot + 1
    for count in slots.values():
        if count != 1:
            raise Reject(
                "primary-artifact-ambiguous",
                "exactly one primary artifact is required per class",
            )
