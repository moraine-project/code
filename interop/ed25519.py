import hashlib

P = 2**255 - 19
L = 2**252 + 27742317777372353535851937790883648493
D = (-121665 * pow(121666, P - 2, P)) % P
I = pow(2, (P - 1) // 4, P)


def _recover_x(y, sign):
    if y >= P:
        return None
    xx = (y * y - 1) * pow(D * y * y + 1, P - 2, P) % P
    if xx == 0:
        return None if sign else 0
    x = pow(xx, (P + 3) // 8, P)
    if (x * x - xx) % P != 0:
        x = x * I % P
    if (x * x - xx) % P != 0:
        return None
    if x % 2 != sign:
        x = P - x
    return x


def _decode_point(encoded):
    if len(encoded) != 32:
        return None
    y = int.from_bytes(encoded, "little") & ((1 << 255) - 1)
    x = _recover_x(y, encoded[31] >> 7)
    if x is None:
        return None
    if (-x * x + y * y - 1 - D * x * x * y * y) % P != 0:
        return None
    return (x, y)


def _add(left, right):
    x1, y1 = left
    x2, y2 = right
    product = D * x1 * x2 * y1 * y2 % P
    x3 = (x1 * y2 + x2 * y1) * pow(1 + product, P - 2, P) % P
    y3 = (y1 * y2 + x1 * x2) * pow(1 - product, P - 2, P) % P
    return (x3, y3)


def _scalar_mult(point, scalar):
    result = (0, 1)
    addend = point
    while scalar > 0:
        if scalar & 1:
            result = _add(result, addend)
        addend = _add(addend, addend)
        scalar >>= 1
    return result


_BASE = (_recover_x(4 * pow(5, P - 2, P) % P, 0), 4 * pow(5, P - 2, P) % P)


def verify(public_key, message, signature):
    if len(public_key) != 32 or len(signature) != 64:
        return False
    public = _decode_point(public_key)
    nonce = _decode_point(signature[:32])
    if public is None or nonce is None:
        return False
    scalar = int.from_bytes(signature[32:], "little")
    if scalar >= L:
        return False
    challenge = (
        int.from_bytes(
            hashlib.sha512(signature[:32] + public_key + message).digest(), "little"
        )
        % L
    )
    return _scalar_mult(_BASE, scalar) == _add(nonce, _scalar_mult(public, challenge))


def _self_test():
    secret = bytes.fromhex(
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60"
    )
    public = bytes.fromhex(
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
    )
    signature = bytes.fromhex(
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155"
        "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
    )
    assert verify(public, b"", signature), "RFC 8032 vector 1 must verify"
    assert not verify(public, b"x", signature), "a changed message must not verify"
    assert not verify(public, b"", signature[:63] + bytes([signature[63] ^ 1])), (
        "a changed signature must not verify"
    )
    assert secret


if __name__ == "__main__":
    _self_test()
    print("ed25519 self-test ok")
