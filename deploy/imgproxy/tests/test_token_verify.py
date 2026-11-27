"""Pro 令牌验签的跨实现测试：令牌由 Go 的 crypto/ed25519 生成，
这里用 Python 标准库实现验签。两边必须一致，否则线上会出现
「auth 签得出、imgproxy 验不过」的静默失配。
"""

import base64
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import ed25519_verify  # noqa: E402
from imgproxy import TokenVerifier  # noqa: E402

# 测试向量由 Go 的 crypto/ed25519 生成（gen_vectors.go），避免用 Python 自签
# 自验——那只能证明实现自洽，证明不了与 virgo-auth 一致。

def _read_vectors() -> tuple[str, str, str]:
    """读取 gen_vectors 产出的向量；没有就跳过（CI 未跑生成器时）。"""
    path = os.environ.get("VECTOR_FILE", "/tmp/edtest/tokens.txt")
    if not os.path.exists(path):
        return "", "", ""
    vals = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            if "=" in line:
                key, value = line.strip().split("=", 1)
                vals[key] = value
    return vals.get("PUB", ""), vals.get("TOKEN", ""), vals.get("EXPIRED", "")


class Ed25519VerifyTest(unittest.TestCase):
    def test_rejects_garbage(self):
        self.assertFalse(ed25519_verify.verify(b"", b"m", b"s"))
        self.assertFalse(ed25519_verify.verify(b"x" * 32, b"m", b"y" * 64))


class TokenVerifierTest(unittest.TestCase):
    def setUp(self):
        pub, token, expired = _read_vectors()
        if not pub:
            self.skipTest("缺少 Go 生成的测试向量（/tmp/edtest/tokens.txt）")
        self.token, self.expired = token, expired
        self.dir = tempfile.mkdtemp()
        self.pub_path = os.path.join(self.dir, "pub.b64")
        with open(self.pub_path, "w", encoding="utf-8") as fh:
            fh.write(pub)

    def test_valid_token_passes(self):
        verifier = TokenVerifier(self.pub_path)
        self.assertTrue(verifier.enabled)
        claims = verifier.verify(self.token)
        self.assertIsNotNone(claims)
        self.assertEqual(claims["sub"], "user-1")
        self.assertEqual(claims["plan"], "pro")

    def test_expired_token_rejected(self):
        verifier = TokenVerifier(self.pub_path)
        self.assertIsNone(verifier.verify(self.expired))

    def test_tampered_token_rejected(self):
        verifier = TokenVerifier(self.pub_path)
        # 翻转签名里的一个字符：Ed25519 验签必须失败。
        payload, sig = self.token.split(".", 1)
        flipped = sig[:5] + ("A" if sig[5] != "A" else "B") + sig[6:]
        self.assertIsNone(verifier.verify(payload + "." + flipped))

    def test_no_public_key_disables_and_returns_none(self):
        verifier = TokenVerifier("")
        self.assertFalse(verifier.enabled)
        self.assertIsNone(verifier.verify(self.token))


if __name__ == "__main__":
    unittest.main(verbosity=2)