// 生成 Ed25519 测试向量：公钥 + 一个有效令牌 + 一个过期令牌。
//
//	go run tests/gen_vectors.go > /tmp/edtest/tokens.txt
//
// 令牌格式必须与 virgo-auth/internal/token 保持一致（base64url 无填充）。
package main

import (
	"crypto/ed25519"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"time"
)

func main() {
	pub, priv, _ := ed25519.GenerateKey(nil)
	now := time.Now().Unix()
	issue := func(exp int64) string {
		payload, _ := json.Marshal(map[string]any{
			"sub": "user-1", "dk": "fp", "plan": "pro", "exp": exp, "iat": now,
		})
		sig := ed25519.Sign(priv, payload)
		enc := base64.RawURLEncoding
		return enc.EncodeToString(payload) + "." + enc.EncodeToString(sig)
	}
	enc := base64.RawURLEncoding
	fmt.Printf("PUB=%s\n", enc.EncodeToString(pub))
	fmt.Printf("TOKEN=%s\n", issue(now+3600))
	fmt.Printf("EXPIRED=%s\n", issue(now-3600))
}
