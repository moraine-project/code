import hashlib
import json
import sys

import ed25519
import versions
from objects import feed_follows, parse_feed, parse_object
from cbor import decode
from reject import Reject

DOMAIN_PREFIX = b"GAMEDIST/v1/"
ALL_KINDS = [
    "genesis",
    "delegation",
    "release",
    "feed-entry",
    "profile",
    "changelog",
    "deny-list",
    "modpack",
    "advisory",
    "attestation",
    "game-def",
    "loader-def",
    "runtime-def",
]
OBJECT_KINDS = {
    "genesis",
    "delegation",
    "release",
    "feed-entry",
    "profile",
    "advisory",
    "attestation",
    "game-def",
    "loader-def",
    "runtime-def",
}


class Actual:
    def __init__(self, accepted, reason=None):
        self.accepted = accepted
        self.reason = reason


def accept():
    return Actual(True)


def evaluate(vector):
    kind = vector["kind"]
    if kind == "predicate":
        return evaluate_predicate(vector)
    if kind == "canonical":
        try:
            payload = bytes.fromhex(vector["payload_hex"])
        except ValueError:
            return Actual(False, "invalid-encoding")
        try:
            decode(payload)
        except Reject as error:
            return Actual(False, error.reason)
        return accept()
    if kind not in OBJECT_KINDS:
        return Actual(False, "wrong-object-kind")
    try:
        payload = bytes.fromhex(vector["payload_hex"])
    except ValueError:
        return Actual(False, "invalid-encoding")
    envelope_json = vector.get("envelope")
    if envelope_json is None:
        return Actual(False, "invalid-encoding")
    try:
        envelope = parse_envelope(envelope_json)
        verify_kind = vector.get("verify_kind") or kind
        value = decode(payload)
        parsed = parse_object(kind, value)
        if (
            vector.get("expected_id") is not None
            and object_id_string(kind, payload) != vector["expected_id"]
        ):
            raise Reject("object-id-mismatch", "computed id does not match expected_id")
        verify_object(vector, kind, parsed, payload, envelope, verify_kind)
    except Reject as error:
        return Actual(False, error.reason)
    return accept()


def verify_object(vector, kind, parsed, payload, envelope, verify_kind):
    if kind == "genesis":
        root = root_set_from_genesis(parsed)
        root_verify(root, signed_message(verify_kind, payload), envelope)
        return
    if kind == "delegation":
        trust = require_trust(vector)
        if verify_kind != "delegation":
            root_verify(root_set(trust), signed_message(verify_kind, payload), envelope)
            return
        purpose = parsed[0]
        message = signed_message("delegation", payload)
        if purpose == "key":
            root = root_set(trust)
            if "delegation" not in root["authorized_kinds"]:
                raise Reject(
                    "unauthorized-kind", "genesis does not authorize delegations"
                )
            for granted in parsed[1]["allowed_kinds"]:
                if granted not in root["authorized_kinds"]:
                    raise Reject("unauthorized-kind", f"delegation grants {granted}")
            root_verify(root, message, envelope)
            return
        if purpose == "ownership-transfer":
            valid = root_verify(root_set(trust), message, envelope)
            if valid < 2:
                raise Reject(
                    "transfer-needs-two-signatures",
                    "ownership transfer needs two signatures",
                )
            return
        if purpose == "migration":
            root = root_set(trust)
            valid = root_verify(root, message, envelope)
            if valid < root["threshold"] + 2:
                raise Reject(
                    "cross-signature-required", "migration requires two home signatures"
                )
            return
        root_verify(root_set(trust), message, envelope)
        return
    if kind == "profile":
        trust = require_trust(vector)
        message = signed_message(verify_kind, payload)
        if try_verify(
            envelope,
            message,
            trusted_keys(trust.get("profile_roots", [])),
            trust["threshold"],
        ):
            return
        if try_verify(
            envelope, message, trusted_keys(trust["roots"]), trust["threshold"]
        ):
            raise Reject("profile-authority", "signer holds a release-only delegation")
        if try_verify(
            envelope,
            message,
            trusted_keys(trust.get("delegated_keys", [])),
            trust["threshold"],
        ):
            raise Reject(
                "profile-authority", "signer does not hold a profile delegation"
            )
        raise Reject("bad-signature", "no valid signature under the trusted keys")
    if kind == "feed-entry":
        message = signed_message(verify_kind, payload)
        for prior_hex in vector.get("prior_feed_hex", []):
            try:
                prior_bytes = bytes.fromhex(prior_hex)
            except ValueError:
                raise Reject("invalid-encoding", "prior feed")
            prior = parse_feed(decode(prior_bytes))
            feed_follows(parsed, prior, prior_bytes)
        root_verify(root_set(require_trust(vector)), message, envelope)
        return
    trust = require_trust(vector)
    root_verify(root_set(trust), signed_message(verify_kind, payload), envelope)


def evaluate_predicate(vector):
    case = vector.get("predicate_case")
    if case is None:
        return Actual(False, "invalid-field-value")
    result = versions.evaluate(
        case["ordering"],
        case.get("catalog", []),
        case["scheme"],
        case.get("values", []),
        case["version"],
    )
    if result is None:
        return Actual(False, "invalid-field-value")
    if result == case["expected"]:
        return accept()
    return Actual(False, "invalid-field-value")


def parse_envelope(value):
    try:
        signatures = []
        for item in value["signatures"]:
            key_id = bytes.fromhex(item["key_id"])
            if len(key_id) != 32:
                raise ValueError
            signatures.append((item["alg"], key_id, bytes.fromhex(item["sig"])))
        key_ids = []
        for raw in value["key_ids"]:
            digest = bytes.fromhex(raw)
            if len(digest) != 32:
                raise ValueError
            key_ids.append((value["alg"], digest))
    except (KeyError, ValueError, TypeError):
        raise Reject("invalid-encoding", "malformed signature envelope")
    return {"alg": value["alg"], "signatures": signatures, "key_ids": key_ids}


def require_trust(vector):
    trust = vector.get("trust")
    if trust is None:
        raise Reject("invalid-field-value", "trusted keys are required")
    return trust


def root_set_from_genesis(genesis):
    return {
        "keys": [(_alg(), root["public_key"]) for root in genesis["roots"]],
        "threshold": genesis["threshold"],
        "authorized_kinds": genesis["authorized_kinds"],
    }


def root_set(trust):
    return {
        "keys": trusted_keys(trust["roots"]),
        "threshold": trust["threshold"],
        "authorized_kinds": trust.get("authorized_kinds") or ALL_KINDS,
    }


def trusted_keys(hex_keys):
    keys = []
    for raw in hex_keys:
        try:
            public = bytes.fromhex(raw)
        except ValueError:
            raise Reject("invalid-field-value", "public key hex")
        if len(public) != 32:
            raise Reject("invalid-field-value", "invalid public key")
        keys.append((_alg(), public))
    return keys


def _alg():
    return 0x01


def root_verify(root, message, envelope):
    return verify_envelope(envelope, message, root["keys"], root["threshold"])


def try_verify(envelope, message, keys, threshold):
    try:
        verify_envelope(envelope, message, keys, threshold)
        return True
    except Reject:
        return False


def verify_envelope(envelope, message, trusted, threshold):
    distinct = []
    for alg, key_id, signature in envelope["signatures"]:
        match = None
        for trusted_alg, public in trusted:
            if trusted_alg == alg and derive_key_id(public) == key_id:
                match = public
                break
        if match is None:
            raise Reject("bad-signature", "unknown key")
        if not ed25519.verify(match, message, signature):
            raise Reject("bad-signature", "signature did not verify")
        identity = (alg, key_id)
        if identity in distinct:
            raise Reject("duplicate-signer", "duplicate signer")
        distinct.append(identity)
    if len(envelope["key_ids"]) != len(distinct) or any(
        item not in distinct for item in envelope["key_ids"]
    ):
        raise Reject(
            "invalid-field-value",
            "envelope key_ids do not match the participating signatures",
        )
    if len(distinct) < threshold:
        raise Reject("threshold-not-met", "not enough valid signatures")
    return len(distinct)


def derive_key_id(public):
    return hashlib.sha256(bytes([_alg()]) + public).digest()


def signed_message(kind, payload):
    return DOMAIN_PREFIX + kind.encode("utf-8") + b"\x00" + payload


def object_id_string(kind, payload):
    digest = hashlib.sha256(signed_message(kind, payload)).hexdigest()
    return f"gd:sha256:{digest}"


def check(vector):
    actual = evaluate(vector)
    expected = vector["expected_verdict"] == "accept"
    if actual.accepted != expected:
        return f"expected {vector['expected_verdict']}, got {'accept' if actual.accepted else 'reject'}"
    if (
        not expected
        and vector.get("reason_code") is not None
        and actual.reason != vector["reason_code"]
    ):
        return f"expected reason `{vector['reason_code']}`, got `{actual.reason}`"
    return None


def main(argv):
    path = argv[1] if len(argv) > 1 else "protocol/vectors/vectors.json"
    with open(path, encoding="utf-8") as handle:
        document = json.load(handle)
    failures = []
    for vector in document["vectors"]:
        problem = check(vector)
        if problem is not None:
            failures.append((vector["name"], problem))
    if failures:
        for name, problem in failures:
            print(f"FAIL {name}: {problem}")
        print(f"\n{len(failures)} of {len(document['vectors'])} vectors disagree")
        return 1
    print(f"ok: {len(document['vectors'])} vectors passed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
