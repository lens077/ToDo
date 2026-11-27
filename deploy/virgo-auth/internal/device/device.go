// Package device 实现「登录后自动下发凭据 + 单设备绑定」。
//
// 设计要点：
//   - 客户端在平台密钥库（Android Keystore / macOS 钥匙串）里生成 EC 私钥，
//     只把公钥上报。私钥永不出设备，服务端也永远拿不到。
//   - 激活时服务端把「账号 sub → 设备公钥」写进绑定表，同一账号只保留一个
//     活跃设备：新设备激活即自动挤掉旧设备（last login wins），旧设备的证明
//     立刻失效；不再需要用户先去旧设备解绑。
//   - 业务请求（imgproxy）带设备证明：对 sub+method+path+时间戳+nonce 用私钥
//     签名，服务端验签 + 校验时间窗 + nonce 去重。令牌被拷到别的设备也没用
//     ——没有对应私钥签不出可用请求。
//
// 两种存储实现：Memory（测试）与 PG（集群落盘，重启不丢绑定）。两者语义必须
// 一致，device_test.go 的用例对二者都跑。
package device

import (
	"context"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/sha256"
	"crypto/x509"
	"encoding/base64"
	"errors"
	"fmt"
	"time"
)

var (
	ErrAlreadyBound   = errors.New("device: account already bound to another device")
	ErrUnbindCooldown = errors.New("device: unbind attempted too recently")
	ErrNoBinding      = errors.New("device: account has no active binding")
	ErrBadProof       = errors.New("device: proof rejected")
)

// Binding 是账号与设备的绑定记录。
type Binding struct {
	Sub        string    `json:"sub"`
	PublicKey  string    `json:"publicKey"` // base64(SPKI DER)
	DeviceName string    `json:"deviceName"`
	BoundAt    time.Time `json:"boundAt"`
	LastSeen   time.Time `json:"lastSeen"`
}

const (
	// UnbindCooldown 是解绑冷却：防止「两台设备来回挤」绕过单设备限制。
	UnbindCooldown = 24 * time.Hour
	// ProofWindow 是证明时间窗：容忍设备时钟偏差。
	ProofWindow = 5 * time.Minute
	// NonceTTL 是 nonce 保留时长：略大于时间窗即可。
	NonceTTL = 10 * time.Minute
)

// Store 是绑定与证明校验的存储抽象。
type Store interface {
	// Activate 把账号绑到这台设备。同账号已有其他设备时**直接替换**（新设备
	// 挤掉旧设备），并返回新绑定；同一设备重复激活是幂等的。
	Activate(ctx context.Context, sub, publicKey, deviceName string) (*Binding, error)
	// Unbind 解绑当前设备（换机用），24 小时内只能一次。
	Unbind(ctx context.Context, sub, publicKey string) error
	// Binding 取当前绑定；没有则 ok=false。
	Binding(ctx context.Context, sub string) (*Binding, bool)
	// Verify 校验设备证明（签名 + 时间窗 + nonce 重放）。
	Verify(ctx context.Context, p Proof) error
}

// Proof 是客户端提交的请求证明。签名原文由服务端按同一规则重建：
//
//	sub \n method \n path \n unix秒 \n nonce
type Proof struct {
	Sub       string
	Method    string
	Path      string
	Timestamp int64
	Nonce     string
	Signature string // base64(ECDSA ASN.1)
}

func proofMessage(sub, method, path string, ts int64, nonce string) []byte {
	return []byte(fmt.Sprintf("%s\n%s\n%s\n%d\n%s", sub, method, path, ts, nonce))
}

// verifySignature 是两种实现共用的密码学校验：曲线限定 + 签名有效性。
func verifySignature(publicKeyB64 string, p Proof) error {
	key, err := parsePublicKey(publicKeyB64)
	if err != nil {
		return ErrBadProof
	}
	sig, err := base64.StdEncoding.DecodeString(p.Signature)
	if err != nil {
		return ErrBadProof
	}
	digest := sha256.Sum256(proofMessage(p.Sub, p.Method, p.Path, p.Timestamp, p.Nonce))
	if !ecdsa.VerifyASN1(key, digest[:], sig) {
		return ErrBadProof
	}
	return nil
}

// checkTimestamp 校验证明时间在 ±ProofWindow 内。
func checkTimestamp(ts int64, now time.Time) error {
	if d := now.Sub(time.Unix(ts, 0)); d > ProofWindow || d < -ProofWindow {
		return ErrBadProof
	}
	return nil
}

func parsePublicKey(b64 string) (*ecdsa.PublicKey, error) {
	der, err := base64.StdEncoding.DecodeString(b64)
	if err != nil {
		return nil, errors.New("device: public key is not valid base64")
	}
	pub, err := x509.ParsePKIXPublicKey(der)
	if err != nil {
		return nil, errors.New("device: public key is not valid SPKI")
	}
	key, ok := pub.(*ecdsa.PublicKey)
	if !ok || key.Curve != elliptic.P256() {
		return nil, errors.New("device: only P-256 ECDSA keys are accepted")
	}
	return key, nil
}

// ValidatePublicKey 供 HTTP 层在入库前拒绝明显错误的公钥。
func ValidatePublicKey(b64 string) error {
	_, err := parsePublicKey(b64)
	return err
}

// Fingerprint 返回公钥指纹（sha256 前 16 字节的 hex），写进令牌里绑定设备。
func Fingerprint(publicKeyB64 string) (string, error) {
	der, err := base64.StdEncoding.DecodeString(publicKeyB64)
	if err != nil {
		return "", errors.New("device: public key is not valid base64")
	}
	sum := sha256.Sum256(der)
	return fmt.Sprintf("%x", sum[:16]), nil
}
