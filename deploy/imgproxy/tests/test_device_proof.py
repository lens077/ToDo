"""设备证明校验的单元测试。

令牌里的 `dpk` 是设备 P-256 公钥（SPKI DER base64）。带 dpk 的令牌必须再附
`X-Virgo-Ts` / `X-Virgo-Nonce` / `X-Virgo-Sig`，签的是
`GET\n{path}\n{ts}\n{nonce}`；缺证明、过期、重放、签名不对、公钥坏都要拒绝。
"""

import base64
import os
import sys
import time
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import imgproxy  # noqa: E402

from cryptography.hazmat.primitives import hashes, serialization  # noqa: E402
from cryptography.hazmat.primitives.asymmetric import ec  # noqa: E402


def _new_device() -> tuple[ec.EllipticCurvePrivateKey, str]:
    key = ec.generate_private_key(ec.SECP256R1())
    spki = key.public_key().public_bytes(
        serialization.Encoding.DER,
        serialization.PublicFormat.SubjectPublicKeyInfo,
    )
    return key, base64.b64encode(spki).decode()


def _headers_for(key, path: str, *, ts: int | None = None, nonce: str = "n0nc3") -> dict[str, str]:
    ts = int(time.time()) if ts is None else ts
    message = f"GET\n{path}\n{ts}\n{nonce}".encode()
    sig = key.sign(message, ec.ECDSA(hashes.SHA256()))
    return {
        "X-Virgo-Ts": str(ts),
        "X-Virgo-Nonce": nonce,
        "X-Virgo-Sig": base64.b64encode(sig).decode(),
    }


class DeviceProofTest(unittest.TestCase):
    def setUp(self) -> None:
        imgproxy._PROOF_NONCES.clear()
        self.key, self.pub = _new_device()
        self.path = "/img?url=https%3A%2F%2Fpbs.twimg.com%2Fmedia%2Fa.jpg"
        self.claims = {"sub": "u1", "dpk": self.pub, "hosts": "pbs.twimg.com"}

    def test_valid_proof_passes(self) -> None:
        headers = _headers_for(self.key, self.path)
        self.assertIsNone(imgproxy.verify_device_proof(self.claims, "GET", self.path, headers))

    def test_replay_is_rejected(self) -> None:
        headers = _headers_for(self.key, self.path)
        self.assertIsNone(imgproxy.verify_device_proof(self.claims, "GET", self.path, headers))
        error = imgproxy.verify_device_proof(self.claims, "GET", self.path, headers)
        self.assertEqual(error, "设备证明重复使用")

    def test_signature_for_other_path_is_rejected(self) -> None:
        headers = _headers_for(self.key, "/img?url=https%3A%2F%2Fpbs.twimg.com%2Fb.jpg")
        error = imgproxy.verify_device_proof(self.claims, "GET", self.path, headers)
        self.assertEqual(error, "设备证明签名不对")

    def test_expired_timestamp_is_rejected(self) -> None:
        headers = _headers_for(self.key, self.path, ts=int(time.time()) - 400)
        error = imgproxy.verify_device_proof(self.claims, "GET", self.path, headers)
        self.assertEqual(error, "设备证明已过期")

    def test_missing_headers_is_rejected(self) -> None:
        self.assertEqual(
            imgproxy.verify_device_proof(self.claims, "GET", self.path, {}),
            "缺少设备证明",
        )

    def test_garbage_public_key_is_rejected(self) -> None:
        claims = {"sub": "u1", "dpk": "bm90LWEta2V5"}
        headers = _headers_for(self.key, self.path)
        self.assertEqual(
            imgproxy.verify_device_proof(claims, "GET", self.path, headers),
            "设备公钥无效",
        )

    def test_token_without_dpk_skips_proof(self) -> None:
        self.assertIsNone(
            imgproxy.verify_device_proof({"sub": "u1"}, "GET", self.path, {})
        )


if __name__ == "__main__":
    unittest.main()
