package device

import (
	"context"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/sha256"
	"crypto/x509"
	"encoding/base64"
	"errors"
	"os"
	"testing"
	"time"
)

// newKey 生成一台设备的 P-256 密钥对，返回公钥 base64(SPKI) 与签名函数。
func newKey(t *testing.T) (string, func(sub, method, path string, ts int64, nonce string) string) {
	t.Helper()
	priv, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatalf("generate key: %v", err)
	}
	der, err := x509.MarshalPKIXPublicKey(&priv.PublicKey)
	if err != nil {
		t.Fatalf("marshal public key: %v", err)
	}
	sign := func(sub, method, path string, ts int64, nonce string) string {
		digest := sha256.Sum256(proofMessage(sub, method, path, ts, nonce))
		sig, err := ecdsa.SignASN1(rand.Reader, priv, digest[:])
		if err != nil {
			t.Fatalf("sign: %v", err)
		}
		return base64.StdEncoding.EncodeToString(sig)
	}
	return base64.StdEncoding.EncodeToString(der), sign
}

// forEachStore 让同一组用例对 Memory 与 PG 都跑。语义必须完全一致，
// PG 才能在生产里替掉内存版。
func forEachStore(t *testing.T, run func(t *testing.T, s Store)) {
	t.Helper()
	t.Run("memory", func(t *testing.T) { run(t, NewMemory()) })
	if dsn := os.Getenv("VIRGO_TEST_PG_DSN"); dsn != "" {
		t.Run("pg", func(t *testing.T) {
			pg, err := NewPG(context.Background(), dsn)
			if err != nil {
				t.Fatalf("connect pg: %v", err)
			}
			t.Cleanup(func() { _ = pg.Close() })
			if _, err := pg.db.Exec("TRUNCATE device_bindings, device_unbinds, proof_nonces"); err != nil {
				t.Fatalf("truncate: %v", err)
			}
			run(t, pg)
		})
	}
}

func TestActivateBindsOneDevicePerAccount(t *testing.T) {
	forEachStore(t, func(t *testing.T, s Store) {
		ctx := context.Background()
		keyA, signA := newKey(t)
		keyB, _ := newKey(t)

		bindingA, err := s.Activate(ctx, "user-1", keyA, "Pixel")
		if err != nil {
			t.Fatalf("first activate: %v", err)
		}
		// 第二台设备激活直接挤掉第一台：绑定换成 B，A 的证明立刻失效。
		bindingB, err := s.Activate(ctx, "user-1", keyB, "iPad")
		if err != nil {
			t.Fatalf("second device should take over, got %v", err)
		}
		if bindingB.PublicKey != keyB {
			t.Fatalf("binding should now be device B, got %q", bindingB.PublicKey)
		}
		stored, ok := s.Binding(ctx, "user-1")
		if !ok || stored == nil {
			t.Fatalf("binding lookup: missing")
		}
		if stored.PublicKey != keyB {
			t.Fatalf("stored binding = %q, want device B", stored.PublicKey)
		}
		if bindingA.PublicKey == stored.PublicKey {
			t.Fatalf("device A should have been replaced")
		}
		// 旧设备再拿自己的私钥签证明应被拒（绑定已换成 B 的公钥）。
		proof := Proof{
			Sub:       "user-1",
			Method:    "GET",
			Path:      "/img?url=x",
			Timestamp: 1,
			Nonce:     "nonce-evicted",
			Signature: signA("user-1", "GET", "/img?url=x", 1, "nonce-evicted"),
		}
		if err := s.Verify(ctx, proof); !errors.Is(err, ErrBadProof) {
			t.Fatalf("evicted device proof should fail, got %v", err)
		}
		// 同一台设备重复激活是幂等的（应用重装/重登不该报错）。
		if _, err := s.Activate(ctx, "user-1", keyB, "iPad"); err != nil {
			t.Fatalf("same device re-activate should succeed: %v", err)
		}
		// 交换回来同样直接生效：last login wins。
		if _, err := s.Activate(ctx, "user-1", keyA, "Pixel"); err != nil {
			t.Fatalf("switch back should succeed: %v", err)
		}
	})
}

func TestActivateRejectsMalformedKeys(t *testing.T) {
	forEachStore(t, func(t *testing.T, s Store) {
		ctx := context.Background()
		if _, err := s.Activate(ctx, "u", "not-base64!!", ""); err == nil {
			t.Fatal("malformed key must be rejected")
		}
		if _, err := s.Activate(ctx, "u", "", "phone"); err == nil {
			t.Fatal("empty key must be rejected")
		}
	})
}

func TestProofVerifiesAndIsBoundToRequest(t *testing.T) {
	forEachStore(t, func(t *testing.T, s Store) {
		ctx := context.Background()
		key, sign := newKey(t)
		if _, err := s.Activate(ctx, "user-1", key, "Pixel"); err != nil {
			t.Fatalf("activate: %v", err)
		}
		now := time.Now().Unix()

		good := Proof{Sub: "user-1", Method: "POST", Path: "/v1/refresh", Timestamp: now, Nonce: "n1"}
		good.Signature = sign(good.Sub, good.Method, good.Path, good.Timestamp, good.Nonce)
		if err := s.Verify(ctx, good); err != nil {
			t.Fatalf("valid proof rejected: %v", err)
		}

		// 改路径 = 换用途，签名不再成立。
		tampered := good
		tampered.Nonce = "n2"
		tampered.Signature = sign(good.Sub, good.Method, "/v1/other", good.Timestamp, tampered.Nonce)
		if err := s.Verify(ctx, tampered); !errors.Is(err, ErrBadProof) {
			t.Fatal("proof for another path must be rejected")
		}
	})
}

func TestProofRejectsReplayAndStaleTimestamps(t *testing.T) {
	forEachStore(t, func(t *testing.T, s Store) {
		ctx := context.Background()
		key, sign := newKey(t)
		if _, err := s.Activate(ctx, "user-1", key, "Pixel"); err != nil {
			t.Fatalf("activate: %v", err)
		}
		now := time.Now().Unix()
		p := Proof{Sub: "user-1", Method: "POST", Path: "/v1/refresh", Timestamp: now, Nonce: "replay"}
		p.Signature = sign(p.Sub, p.Method, p.Path, p.Timestamp, p.Nonce)
		if err := s.Verify(ctx, p); err != nil {
			t.Fatalf("first use should pass: %v", err)
		}
		// 同一 nonce 重放必须被拒。
		if err := s.Verify(ctx, p); !errors.Is(err, ErrBadProof) {
			t.Fatal("replayed nonce must be rejected")
		}

		stale := Proof{Sub: "user-1", Method: "POST", Path: "/v1/refresh", Timestamp: now - 3600, Nonce: "old"}
		stale.Signature = sign(stale.Sub, stale.Method, stale.Path, stale.Timestamp, stale.Nonce)
		if err := s.Verify(ctx, stale); !errors.Is(err, ErrBadProof) {
			t.Fatal("stale timestamp must be rejected")
		}
	})
}

func TestProofFromAnotherKeyCannotImpersonate(t *testing.T) {
	forEachStore(t, func(t *testing.T, s Store) {
		ctx := context.Background()
		boundKey, _ := newKey(t)
		_, attackerSign := newKey(t)
		if _, err := s.Activate(ctx, "user-1", boundKey, "Pixel"); err != nil {
			t.Fatalf("activate: %v", err)
		}
		now := time.Now().Unix()
		p := Proof{Sub: "user-1", Method: "POST", Path: "/v1/refresh", Timestamp: now, Nonce: "evil"}
		p.Signature = attackerSign(p.Sub, p.Method, p.Path, p.Timestamp, p.Nonce)
		// 这正是「令牌被拷到别的设备」的场景：没有绑定私钥就签不出可用请求。
		if err := s.Verify(ctx, p); !errors.Is(err, ErrBadProof) {
			t.Fatal("proof signed by a different key must be rejected")
		}
	})
}

func TestUnbindEnforcesCooldown(t *testing.T) {
	forEachStore(t, func(t *testing.T, s Store) {
		ctx := context.Background()
		key, _ := newKey(t)
		if _, err := s.Activate(ctx, "user-1", key, "Pixel"); err != nil {
			t.Fatalf("activate: %v", err)
		}
		if err := s.Unbind(ctx, "user-1", key); err != nil {
			t.Fatalf("first unbind: %v", err)
		}
		key2, _ := newKey(t)
		if _, err := s.Activate(ctx, "user-1", key2, "iPad"); err != nil {
			t.Fatalf("activate new device after unbind: %v", err)
		}
		// 冷却期内再解绑要被拒，防止两台设备来回挤。
		if err := s.Unbind(ctx, "user-1", key2); !errors.Is(err, ErrUnbindCooldown) {
			t.Fatalf("second unbind inside cooldown should be rejected, got %v", err)
		}
	})
}

func TestFingerprintStable(t *testing.T) {
	key, _ := newKey(t)
	a, err := Fingerprint(key)
	if err != nil {
		t.Fatalf("fingerprint: %v", err)
	}
	b, _ := Fingerprint(key)
	if a != b || len(a) != 32 {
		t.Fatalf("fingerprint must be stable 32-hex, got %q vs %q", a, b)
	}
}
