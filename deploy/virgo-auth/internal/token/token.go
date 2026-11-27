// Package token 签发并校验 imgproxy 用的短期令牌。
//
// 令牌是紧凑的 `base64(payload).base64(sig)`（非 JWT：只需要能被我们自己的
// imgproxy 验，不需要互操作）。签名算法是 Ed25519——公钥可以只放 imgproxy，
// 私钥只在 auth 服务，两边不共享密钥。
//
// 令牌里绑定了 sub 与设备公钥指纹：即使令牌被拷到别的设备，imgproxy 还会
// 要求请求带上设备证明（见 device.Verify），两道闸门叠加。
package token

import (
	"crypto/ed25519"
	"encoding/base64"
	"encoding/json"
	"errors"
	"time"
)

var (
	ErrMalformed = errors.New("token: malformed")
	ErrBadSig    = errors.New("token: bad signature")
	ErrExpired   = errors.New("token: expired")
)

// Claims 是令牌载荷。字段刻意少：只放鉴权与计费需要的。
type Claims struct {
	Sub        string `json:"sub"`   // Casdoor sub（账号唯一标识）
	DeviceKey  string `json:"dk"`    // 设备公钥指纹（sha256 前 16 字节 hex）
	Plan       string `json:"plan"`  // "pro"
	ExpiresAt  int64  `json:"exp"`   // unix 秒
	IssuedAt   int64  `json:"iat"`   // unix 秒
	ProxyHosts string `json:"hosts"` // 允许回源的域名白名单（逗号分隔），空=全部
	// DevicePub 是设备 P-256 公钥（SPKI base64）。imgproxy 见到它就会要求
	// 每个取图请求附带设备签名（时间戳 + nonce + DER 签名），令牌被拷走也
	// 用不了；为空表示只验令牌（老令牌兼容）。
	DevicePub string `json:"dpk,omitempty"`
}

// Issuer 持有私钥，只在 auth 服务里。
type Issuer struct {
	priv ed25519.PrivateKey
	ttl  time.Duration
}

func NewIssuer(priv ed25519.PrivateKey, ttl time.Duration) *Issuer {
	return &Issuer{priv: priv, ttl: ttl}
}

// Issue 为一次成功激活签发短期令牌。默认 12 小时：够用户读完一篇长文，
// 又不至于长期可用。
func (i *Issuer) Issue(sub, deviceKey, devicePub, plan string) (string, Claims, error) {
	now := time.Now()
	claims := Claims{
		Sub:       sub,
		DeviceKey: deviceKey,
		DevicePub: devicePub,
		Plan:      plan,
		IssuedAt:  now.Unix(),
		ExpiresAt: now.Add(i.ttl).Unix(),
	}
	payload, err := json.Marshal(claims)
	if err != nil {
		return "", Claims{}, err
	}
	sig := ed25519.Sign(i.priv, payload)
	enc := base64.RawURLEncoding
	return enc.EncodeToString(payload) + "." + enc.EncodeToString(sig), claims, nil
}

// Verifier 持有公钥，imgproxy 侧用。
type Verifier struct {
	pub ed25519.PublicKey
}

func NewVerifier(pub ed25519.PublicKey) *Verifier {
	return &Verifier{pub: pub}
}

// Verify 校验签名与有效期，返回 claims。
func (v *Verifier) Verify(raw string) (Claims, error) {
	parts := splitToken(raw)
	if len(parts) != 2 {
		return Claims{}, ErrMalformed
	}
	enc := base64.RawURLEncoding
	payload, err := enc.DecodeString(parts[0])
	if err != nil {
		return Claims{}, ErrMalformed
	}
	sig, err := enc.DecodeString(parts[1])
	if err != nil {
		return Claims{}, ErrMalformed
	}
	if !ed25519.Verify(v.pub, payload, sig) {
		return Claims{}, ErrBadSig
	}
	var claims Claims
	if err := json.Unmarshal(payload, &claims); err != nil {
		return Claims{}, ErrMalformed
	}
	if time.Now().Unix() > claims.ExpiresAt {
		return Claims{}, ErrExpired
	}
	return claims, nil
}

func splitToken(raw string) []string {
	out := make([]string, 0, 2)
	start := 0
	for i := 0; i < len(raw); i++ {
		if raw[i] == '.' {
			out = append(out, raw[start:i])
			start = i + 1
		}
	}
	out = append(out, raw[start:])
	return out
}
