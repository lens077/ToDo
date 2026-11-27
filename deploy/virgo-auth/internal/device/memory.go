package device

import (
	"context"
	"sync"
	"time"
)

// Memory 是进程内实现：单测与「没有数据库」的降级场景用。
// 重启即丢绑定，生产走 PG。
type Memory struct {
	mu      sync.Mutex
	bySub   map[string]*Binding
	unbinds map[string]time.Time // sub → 最近解绑时间
	nonces  map[string]time.Time // "sub|nonce" → 首次见到的时间
	nowFn   func() time.Time
}

func NewMemory() *Memory {
	return &Memory{
		bySub:   map[string]*Binding{},
		unbinds: map[string]time.Time{},
		nonces:  map[string]time.Time{},
		nowFn:   time.Now,
	}
}

func (m *Memory) Activate(_ context.Context, sub, publicKey, deviceName string) (*Binding, error) {
	if sub == "" || publicKey == "" {
		return nil, ErrNoBinding
	}
	if err := ValidatePublicKey(publicKey); err != nil {
		return nil, err
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	now := m.nowFn()
	boundAt := now
	if old, ok := m.bySub[sub]; ok && old.PublicKey == publicKey {
		// 同一台设备重复激活：保留最初绑定时间，只刷新心跳与设备名。
		boundAt = old.BoundAt
	}
	// 不同公钥视为换设备：直接覆盖，旧设备在下一次证明校验时就会失败。
	b := &Binding{Sub: sub, PublicKey: publicKey, DeviceName: deviceName, BoundAt: boundAt, LastSeen: now}
	m.bySub[sub] = b
	out := *b
	return &out, nil
}

func (m *Memory) Unbind(_ context.Context, sub, publicKey string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	now := m.nowFn()
	if last, ok := m.unbinds[sub]; ok && now.Sub(last) < UnbindCooldown {
		return ErrUnbindCooldown
	}
	existing, ok := m.bySub[sub]
	if !ok || existing.PublicKey != publicKey {
		return ErrNoBinding
	}
	delete(m.bySub, sub)
	m.unbinds[sub] = now
	return nil
}

func (m *Memory) Binding(_ context.Context, sub string) (*Binding, bool) {
	m.mu.Lock()
	defer m.mu.Unlock()
	b, ok := m.bySub[sub]
	if !ok {
		return nil, false
	}
	out := *b
	return &out, true
}

func (m *Memory) Verify(_ context.Context, p Proof) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	now := m.nowFn()
	if p.Sub == "" || p.Signature == "" || p.Nonce == "" {
		return ErrBadProof
	}
	if err := checkTimestamp(p.Timestamp, now); err != nil {
		return err
	}
	b, ok := m.bySub[p.Sub]
	if !ok {
		return ErrNoBinding
	}
	if err := verifySignature(b.PublicKey, p); err != nil {
		return err
	}
	nonceKey := p.Sub + "|" + p.Nonce
	if seen, ok := m.nonces[nonceKey]; ok && now.Sub(seen) < NonceTTL {
		return ErrBadProof
	}
	m.nonces[nonceKey] = now
	for k, seenAt := range m.nonces {
		if now.Sub(seenAt) > NonceTTL {
			delete(m.nonces, k)
		}
	}
	b.LastSeen = now
	return nil
}
