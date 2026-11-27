package device

import (
	"context"
	"database/sql"
	"errors"
	"time"

	_ "github.com/jackc/pgx/v5/stdlib" // 注册 database/sql 的 "pgx" 驱动
)

// schema 是落盘结构。幂等：每次启动跑一遍。
//
//   - device_bindings：账号 → 设备公钥。主键 sub 保证「一账号一活跃设备」。
//   - device_unbinds： 解绑冷却记录。
//   - proof_nonces：   (sub, nonce) 主键即天然的去重，INSERT 冲突就是重放。
const schema = `
CREATE TABLE IF NOT EXISTS device_bindings (
    sub         TEXT PRIMARY KEY,
    public_key  TEXT NOT NULL,
    device_name TEXT NOT NULL DEFAULT '',
    bound_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS device_unbinds (
    sub        TEXT PRIMARY KEY,
    unbound_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS proof_nonces (
    sub     TEXT NOT NULL,
    nonce   TEXT NOT NULL,
    seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (sub, nonce)
);
CREATE INDEX IF NOT EXISTS proof_nonces_seen_at ON proof_nonces (seen_at);
`

// PG 是 PostgreSQL 实现：绑定与 nonce 都落盘，Pod 重启不丢。
type PG struct {
	db    *sql.DB
	nowFn func() time.Time
}

// NewPG 打开连接并建表。dsn 形如
// postgres://user:pass@host:5432/dbname?sslmode=verify-full&sslrootcert=...
func NewPG(ctx context.Context, dsn string) (*PG, error) {
	db, err := sql.Open("pgx", dsn)
	if err != nil {
		return nil, err
	}
	// CNPG 单实例、连接数有限：给这个服务留一个小池子即可。
	db.SetMaxOpenConns(8)
	db.SetMaxIdleConns(4)
	db.SetConnMaxLifetime(30 * time.Minute)
	if err := db.PingContext(ctx); err != nil {
		_ = db.Close()
		return nil, err
	}
	if _, err := db.ExecContext(ctx, schema); err != nil {
		_ = db.Close()
		return nil, err
	}
	return &PG{db: db, nowFn: time.Now}, nil
}

func (p *PG) Close() error { return p.db.Close() }

func (p *PG) Activate(ctx context.Context, sub, publicKey, deviceName string) (*Binding, error) {
	if sub == "" || publicKey == "" {
		return nil, ErrNoBinding
	}
	if err := ValidatePublicKey(publicKey); err != nil {
		return nil, err
	}
	tx, err := p.db.BeginTx(ctx, nil)
	if err != nil {
		return nil, err
	}
	defer func() { _ = tx.Rollback() }()

	// 行锁住该账号，避免两个并发激活都通过「是否已绑定」的判断。
	var existingKey string
	var boundAt time.Time
	err = tx.QueryRowContext(ctx,
		"SELECT public_key, bound_at FROM device_bindings WHERE sub = $1 FOR UPDATE", sub).
		Scan(&existingKey, &boundAt)
	switch {
	case errors.Is(err, sql.ErrNoRows):
		boundAt = p.nowFn()
		_, err = tx.ExecContext(ctx,
			"INSERT INTO device_bindings (sub, public_key, device_name, bound_at, last_seen) VALUES ($1,$2,$3,$4,$4)",
			sub, publicKey, deviceName, boundAt)
		if err != nil {
			return nil, err
		}
	case err != nil:
		return nil, err
	case existingKey != publicKey:
		// 换设备：直接覆盖绑定，旧设备的证明随即失效（last login wins）。
		boundAt = p.nowFn()
		_, err = tx.ExecContext(ctx,
			"UPDATE device_bindings SET public_key = $2, device_name = $3, bound_at = $4, last_seen = $4 WHERE sub = $1",
			sub, publicKey, deviceName, boundAt)
		if err != nil {
			return nil, err
		}
	default:
		// 同一设备重复激活：幂等，只刷新 last_seen 与设备名。
		_, err = tx.ExecContext(ctx,
			"UPDATE device_bindings SET last_seen = $2, device_name = $3 WHERE sub = $1",
			sub, p.nowFn(), deviceName)
		if err != nil {
			return nil, err
		}
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return &Binding{Sub: sub, PublicKey: publicKey, DeviceName: deviceName,
		BoundAt: boundAt, LastSeen: p.nowFn()}, nil
}

func (p *PG) Unbind(ctx context.Context, sub, publicKey string) error {
	now := p.nowFn()
	tx, err := p.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer func() { _ = tx.Rollback() }()

	var unboundAt time.Time
	err = tx.QueryRowContext(ctx,
		"SELECT unbound_at FROM device_unbinds WHERE sub = $1", sub).Scan(&unboundAt)
	if err == nil && now.Sub(unboundAt) < UnbindCooldown {
		return ErrUnbindCooldown
	}
	if err != nil && !errors.Is(err, sql.ErrNoRows) {
		return err
	}

	var existingKey string
	err = tx.QueryRowContext(ctx,
		"SELECT public_key FROM device_bindings WHERE sub = $1 FOR UPDATE", sub).Scan(&existingKey)
	if errors.Is(err, sql.ErrNoRows) || (err == nil && existingKey != publicKey) {
		return ErrNoBinding
	}
	if err != nil {
		return err
	}
	if _, err := tx.ExecContext(ctx, "DELETE FROM device_bindings WHERE sub = $1", sub); err != nil {
		return err
	}
	if _, err := tx.ExecContext(ctx,
		`INSERT INTO device_unbinds (sub, unbound_at) VALUES ($1,$2)
		 ON CONFLICT (sub) DO UPDATE SET unbound_at = EXCLUDED.unbound_at`, sub, now); err != nil {
		return err
	}
	return tx.Commit()
}

func (p *PG) Binding(ctx context.Context, sub string) (*Binding, bool) {
	var b Binding
	err := p.db.QueryRowContext(ctx,
		"SELECT sub, public_key, device_name, bound_at, last_seen FROM device_bindings WHERE sub = $1", sub).
		Scan(&b.Sub, &b.PublicKey, &b.DeviceName, &b.BoundAt, &b.LastSeen)
	if err != nil {
		return nil, false
	}
	return &b, true
}

func (p *PG) Verify(ctx context.Context, proof Proof) error {
	now := p.nowFn()
	if proof.Sub == "" || proof.Signature == "" || proof.Nonce == "" {
		return ErrBadProof
	}
	if err := checkTimestamp(proof.Timestamp, now); err != nil {
		return err
	}
	var publicKey string
	err := p.db.QueryRowContext(ctx,
		"SELECT public_key FROM device_bindings WHERE sub = $1", proof.Sub).Scan(&publicKey)
	if errors.Is(err, sql.ErrNoRows) {
		return ErrNoBinding
	}
	if err != nil {
		return err
	}
	if err := verifySignature(publicKey, proof); err != nil {
		return err
	}
	// 主键冲突 = 这个 nonce 用过；把「去重」交给数据库而不是内存。
	res, err := p.db.ExecContext(ctx,
		"INSERT INTO proof_nonces (sub, nonce, seen_at) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING",
		proof.Sub, proof.Nonce, now)
	if err != nil {
		return err
	}
	if n, _ := res.RowsAffected(); n == 0 {
		return ErrBadProof
	}
	if _, err := p.db.ExecContext(ctx,
		"UPDATE device_bindings SET last_seen = $2 WHERE sub = $1", proof.Sub, now); err != nil {
		return err
	}
	// 顺手清理过期 nonce；量很小，不需要单独任务。
	_, _ = p.db.ExecContext(ctx,
		"DELETE FROM proof_nonces WHERE seen_at < $1", now.Add(-NonceTTL))
	return nil
}
