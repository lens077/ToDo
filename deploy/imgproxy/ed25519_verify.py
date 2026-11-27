"""
Ed25519 验签（RFC 8032 参考实现的精简版，只做 verify）。

为什么自己写而不是装库：node4 上 imgproxy 是零第三方依赖的 Python 标准库
服务，装 pynacl 会引入编译依赖。这里只需要「用公钥验一个签名」，不碰私钥，
按 RFC 8032 §5.1.7 的判定流程实现，并用 Go 的 crypto/ed25519 产物做交叉验证
（见 tests/test_ed25519.py）。
"""

import hashlib

# Curve25519 参数（RFC 8032 §5.1）
P = 2**255 - 19
L = 2**252 + 27742317777372353535851937790883648493
D = -121665 * pow(121666, P - 2, P) % P
I = pow(2, (P - 1) // 4, P)


def _inv(x: int) -> int:
    return pow(x, P - 2, P)


def _recover_x(y: int, sign: int):
    if y >= P:
        return None
    x2 = (y * y - 1) * _inv(D * y * y + 1) % P
    if x2 == 0:
        return None if sign else 0
    x = pow(x2, (P + 3) // 8, P)
    if (x * x - x2) % P != 0:
        x = x * I % P
    if (x * x - x2) % P != 0:
        return None
    if (x & 1) != sign:
        x = P - x
    return x


_GY = 4 * _inv(5) % P
_GX = _recover_x(_GY, 0)
G = (_GX, _GY, 1, _GX * _GY % P)


def _add(p1, p2):
    x1, y1, z1, t1 = p1
    x2, y2, z2, t2 = p2
    a = (y1 - x1) * (y2 - x2) % P
    b = (y1 + x1) * (y2 + x2) % P
    c = t1 * 2 * D * t2 % P
    dd = z1 * 2 * z2 % P
    e, f, g, h = b - a, dd - c, dd + c, b + a
    return (e * f % P, g * h % P, f * g % P, e * h % P)


def _mul(s: int, point):
    if s == 0:
        return (0, 1, 1, 0)
    q = _mul(s // 2, point)
    q = _add(q, q)
    if s & 1:
        q = _add(q, point)
    return q


def _compress(point) -> bytes:
    x, y, z, _ = point
    zinv = _inv(z)
    x, y = x * zinv % P, y * zinv % P
    return int.to_bytes(y | ((x & 1) << 255), 32, "little")


def _decompress(raw: bytes):
    if len(raw) != 32:
        return None
    y = int.from_bytes(raw, "little")
    sign = y >> 255
    y &= (1 << 255) - 1
    x = _recover_x(y, sign)
    if x is None:
        return None
    return (x, y, 1, x * y % P)


def _check_point(point) -> bool:
    x, y, z, t = point
    if x == 0 and y == 0:
        return False
    if (y * y - x * x - z * z - D * t * t) % P != 0:
        return False
    if (t * z - x * y) % P != 0:
        return False
    return True


def _hash_int(data: bytes) -> int:
    return int.from_bytes(hashlib.sha512(data).digest(), "little")


def verify(public_key: bytes, message: bytes, signature: bytes) -> bool:
    """RFC 8032 §5.1.7：签名必须 64 字节，公钥 32 字节。"""
    if len(signature) != 64 or len(public_key) != 32:
        return False
    a = _decompress(public_key)
    if a is None:
        return False
    r_bytes = signature[:32]
    r = _decompress(r_bytes)
    if r is None:
        return False
    s = int.from_bytes(signature[32:], "little")
    if s >= L:
        return False
    # S·B = R + H(R‖A‖M)·A
    left = _compress(_mul(s, G))
    h = _hash_int(r_bytes + public_key + message) % L
    right = _compress(_add(r, _mul(h, a)))
    return left == right