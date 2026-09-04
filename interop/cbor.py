import unicodedata

from reject import Reject

MAX_DEPTH = 64

MAJOR_UNSIGNED = 0
MAJOR_NEGATIVE = 1
MAJOR_BYTES = 2
MAJOR_TEXT = 3
MAJOR_ARRAY = 4
MAJOR_MAP = 5
MAJOR_TAG = 6
MAJOR_SIMPLE = 7


class Map:
    def __init__(self, pairs):
        self.pairs = pairs

    def get(self, key):
        for name, value in self.pairs:
            if type(name) is str and name == key:
                return value
        return None


def decode(data):
    parser = _Parser(data)
    value = parser.parse(0)
    if parser.position != len(data):
        raise Reject("invalid-encoding", "trailing bytes after the top-level item")
    return value


class _Parser:
    def __init__(self, data):
        self.data = data
        self.position = 0

    def parse(self, depth):
        if depth > MAX_DEPTH:
            raise Reject("invalid-encoding", "max nesting depth exceeded")
        initial = self.byte()
        major = initial >> 5
        additional = initial & 0x1F
        if major == MAJOR_UNSIGNED:
            argument = self.argument(additional)
            if argument > 0x7FFFFFFFFFFFFFFF:
                raise Reject(
                    "invalid-encoding", "integer is outside the signed 64-bit range"
                )
            return argument
        if major == MAJOR_NEGATIVE:
            argument = self.argument(additional)
            if argument > 0x7FFFFFFFFFFFFFFF:
                raise Reject(
                    "invalid-encoding", "integer is outside the signed 64-bit range"
                )
            return -1 - argument
        if major == MAJOR_BYTES:
            length = self.length(additional)
            return self.exact(length)
        if major == MAJOR_TEXT:
            length = self.length(additional)
            raw = self.exact(length)
            try:
                return raw.decode("utf-8")
            except UnicodeDecodeError:
                raise Reject("invalid-encoding", "text string is not valid UTF-8")
        if major == MAJOR_ARRAY:
            length = self.length(additional)
            return [self.parse(depth + 1) for _ in range(length)]
        if major == MAJOR_MAP:
            return self.map(additional, depth)
        if major == MAJOR_TAG:
            raise Reject("invalid-encoding", "CBOR tags are not used by this profile")
        return self.simple(additional)

    def map(self, additional, depth):
        length = self.length(additional)
        pairs = []
        previous = None
        normalized = set()
        for _ in range(length):
            start = self.position
            key = self.parse(depth + 1)
            encoded = self.data[start : self.position]
            if previous is not None:
                if previous == encoded:
                    raise Reject("duplicate-key", "duplicate map key")
                if not _sorted_before(previous, encoded):
                    raise Reject(
                        "non-canonical-encoding",
                        "map keys are not in deterministic order",
                    )
            if type(key) is str:
                folded = unicodedata.normalize("NFC", key)
                if folded in normalized:
                    raise Reject(
                        "non-canonical-encoding",
                        "text keys differ only by Unicode normalization",
                    )
                normalized.add(folded)
            previous = encoded
            pairs.append((key, self.parse(depth + 1)))
        return Map(pairs)

    def simple(self, additional):
        if additional == 20:
            return False
        if additional == 21:
            return True
        if additional == 22:
            return None
        if additional in (25, 26, 27):
            raise Reject(
                "invalid-encoding", "floating-point values are forbidden by the profile"
            )
        if additional == 31:
            raise Reject("invalid-encoding", "indefinite-length items are forbidden")
        raise Reject("invalid-encoding", "unsupported simple value")

    def argument(self, additional):
        if additional <= 23:
            return additional
        if additional == 24:
            value = self.byte()
            if value < 24:
                raise Reject(
                    "non-canonical-encoding",
                    "integer is not encoded in its minimal width",
                )
            return value
        if additional == 25:
            value = self.u16()
            if value <= 0xFF:
                raise Reject(
                    "non-canonical-encoding",
                    "integer is not encoded in its minimal width",
                )
            return value
        if additional == 26:
            value = self.u32()
            if value <= 0xFFFF:
                raise Reject(
                    "non-canonical-encoding",
                    "integer is not encoded in its minimal width",
                )
            return value
        if additional == 27:
            value = self.u64()
            if value <= 0xFFFFFFFF:
                raise Reject(
                    "non-canonical-encoding",
                    "integer is not encoded in its minimal width",
                )
            return value
        if additional == 31:
            raise Reject("invalid-encoding", "indefinite-length items are forbidden")
        raise Reject("invalid-encoding", "unsupported simple value")

    def length(self, additional):
        if additional <= 23:
            return additional
        if additional == 24:
            value = self.byte()
            if value < 24:
                raise Reject(
                    "non-canonical-encoding",
                    "length is not encoded in its minimal width",
                )
            return value
        if additional == 25:
            value = self.u16()
            if value <= 0xFF:
                raise Reject(
                    "non-canonical-encoding",
                    "length is not encoded in its minimal width",
                )
            return value
        if additional == 26:
            value = self.u32()
            if value <= 0xFFFF:
                raise Reject(
                    "non-canonical-encoding",
                    "length is not encoded in its minimal width",
                )
            return value
        if additional == 27:
            value = self.u64()
            if value <= 0xFFFFFFFF:
                raise Reject(
                    "non-canonical-encoding",
                    "length is not encoded in its minimal width",
                )
            return value
        if additional == 31:
            raise Reject("invalid-encoding", "indefinite-length items are forbidden")
        raise Reject("invalid-encoding", "unsupported simple value")

    def byte(self):
        if self.position >= len(self.data):
            raise Reject("invalid-encoding", "unexpected end of input")
        value = self.data[self.position]
        self.position += 1
        return value

    def exact(self, length):
        end = self.position + length
        if end > len(self.data):
            raise Reject("invalid-encoding", "unexpected end of input")
        chunk = self.data[self.position : end]
        self.position = end
        return chunk

    def u16(self):
        return int.from_bytes(self.exact(2), "big")

    def u32(self):
        return int.from_bytes(self.exact(4), "big")

    def u64(self):
        return int.from_bytes(self.exact(8), "big")


def _sorted_before(previous, following):
    return len(previous) < len(following) or (
        len(previous) == len(following) and previous < following
    )
