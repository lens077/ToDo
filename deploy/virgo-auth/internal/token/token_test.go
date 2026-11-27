package token

import (
	"crypto/ed25519"
	"errors"
	"testing"
	"time"
)

func TestIssueVerifyRoundTrip(t *testing.T) {
	_, priv, _ := ed25519.GenerateKey(nil)
	issuer := NewIssuer(priv, time.Hour)
	verifier := NewVerifier(priv.Public().(ed25519.PublicKey))

	raw, claims, err := issuer.Issue("user-sub", "device-fp", "dev-pub", "pro")
	if err != nil {
		t.Fatalf("issue: %v", err)
	}
	got, err := verifier.Verify(raw)
	if err != nil {
		t.Fatalf("verify: %v", err)
	}
	if got.Sub != "user-sub" || got.Plan != "pro" || got.DeviceKey != "device-fp" {
		t.Fatalf("claims mismatch: %+v", got)
	}
	if got.ExpiresAt != claims.ExpiresAt {
		t.Fatalf("expiry drifted: %d vs %d", got.ExpiresAt, claims.ExpiresAt)
	}
}

func TestVerifyRejectsTamperedPayload(t *testing.T) {
	_, priv, _ := ed25519.GenerateKey(nil)
	issuer := NewIssuer(priv, time.Hour)
	verifier := NewVerifier(priv.Public().(ed25519.PublicKey))

	raw, _, err := issuer.Issue("user-sub", "fp", "dev-pub", "pro")
	if err != nil {
		t.Fatalf("issue: %v", err)
	}
	// 换掉 payload 里的 plan，签名就对不上——客户端无法把自己改成更高级别。
	forged := []byte(raw)
	forged[0] = 'X'
	if _, err := verifier.Verify(string(forged)); !errors.Is(err, ErrBadSig) && !errors.Is(err, ErrMalformed) {
		t.Fatalf("tampered token should fail, got %v", err)
	}
}

func TestVerifyRejectsExpired(t *testing.T) {
	_, priv, _ := ed25519.GenerateKey(nil)
	// TTL 为负等于签发即过期。
	issuer := NewIssuer(priv, -time.Minute)
	verifier := NewVerifier(priv.Public().(ed25519.PublicKey))
	raw, _, err := issuer.Issue("s", "fp", "dev-pub", "pro")
	if err != nil {
		t.Fatalf("issue: %v", err)
	}
	if _, err := verifier.Verify(raw); !errors.Is(err, ErrExpired) {
		t.Fatalf("expired token should fail, got %v", err)
	}
}

func TestOtherKeyCannotForge(t *testing.T) {
	_, priv, _ := ed25519.GenerateKey(nil)
	otherPub, _, _ := ed25519.GenerateKey(nil)
	issuer := NewIssuer(priv, time.Hour)
	// imgproxy 只认自己的公钥：拿别的公钥验签必须失败。
	raw, _, err := issuer.Issue("s", "fp", "dev-pub", "pro")
	if err != nil {
		t.Fatalf("issue: %v", err)
	}
	if _, err := NewVerifier(otherPub).Verify(raw); !errors.Is(err, ErrBadSig) {
		t.Fatalf("foreign public key must reject, got %v", err)
	}
}
