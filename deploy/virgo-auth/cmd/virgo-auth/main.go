// Virgo 授权服务：登录 → 校验 Pro → 绑定设备 → 签发短期代理由牌。
//
// 路由（全部挂在 https://virgo-img.apikv.com 下）：
//
//	GET  /auth/login                 → 302 到 Casdoor 授权页
//	GET  /auth/callback?code=&state= → code 换 userinfo，校验 Pro，重定向到客户端
//	POST /v1/activate                → {sub 由服务端 cookie 决定}，上报设备公钥 → 令牌
//	POST /v1/refresh                 → 用设备证明换新令牌
//	POST /v1/unbind                  → 解绑（换机用，带冷却）
//	GET  /v1/entitlements            → 当前账号是否 Pro 与到期时间
//	GET  /healthz
//
// 关键边界：Pro 判定与设备绑定都在服务端；客户端只拿到「绑定到这台设备」的
// 短期令牌，拿不到也改不了这两件事。
package main

import (
	"context"
	"crypto/ed25519"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"virgo-auth/internal/device"
	"virgo-auth/internal/oidc"
	"virgo-auth/internal/token"
)

type config struct {
	Listen       string
	CasdoorURL   string
	ClientID     string
	ClientSecret string
	RedirectURL  string
	StateKey     []byte
	TokenKeyPath string
	ProRole      string // 例：lens/virgo_pro 或 lens/virgo-pro（组/角色名均可）
	ProGroup     string // 例：lens/virgo-pro；命中其一即 Pro
	TokenTTL     time.Duration
	ClientScheme string // 回调后回跳客户端用的 scheme，如 virgo://
}

func loadConfig() config {
	get := func(k, def string) string {
		if v := strings.TrimSpace(os.Getenv(k)); v != "" {
			return v
		}
		return def
	}
	ttl, err := time.ParseDuration(get("TOKEN_TTL", "12h"))
	if err != nil {
		ttl = 12 * time.Hour
	}
	return config{
		Listen:       get("LISTEN", "127.0.0.1:8766"),
		CasdoorURL:   get("CASDOOR_URL", "https://casdoor.apikv.com"),
		ClientID:     os.Getenv("CASDOOR_CLIENT_ID"),
		ClientSecret: os.Getenv("CASDOOR_CLIENT_SECRET"),
		RedirectURL:  get("CASDOOR_REDIRECT_URL", "https://virgo-img.apikv.com/auth/callback"),
		StateKey:     []byte(get("AUTH_STATE_SECRET", "")),
		TokenKeyPath: get("TOKEN_KEY_FILE", "/srv/virgo-auth/token.ed25519"),
		ProRole:      get("PRO_ROLE", ""),
		ProGroup:     get("PRO_GROUP", ""),
		TokenTTL:     ttl,
		ClientScheme: get("CLIENT_SCHEME", ""),
	}
}

// session 保存登录态。用签名 cookie：账号 sub 由服务端签发，客户端改不了。
type server struct {
	cfg    config
	oidc   *oidc.Client
	store  device.Store
	issuer *token.Issuer
}

func main() {
	cfg := loadConfig()
	if len(cfg.StateKey) < 16 {
		log.Fatal("AUTH_STATE_SECRET 未设置或过短（至少 16 字节）")
	}
	if cfg.ClientID == "" || cfg.ClientSecret == "" {
		log.Fatal("CASDOOR_CLIENT_ID / CASDOOR_CLIENT_SECRET 未设置")
	}
	priv, err := loadOrCreateTokenKey(cfg.TokenKeyPath)
	if err != nil {
		log.Fatalf("加载令牌私钥失败：%v", err)
	}
	store, err := openStore(cfg)
	if err != nil {
		log.Fatalf("初始化绑定存储失败：%v", err)
	}
	srv := &server{
		cfg: cfg,
		oidc: oidc.New(oidc.Config{
			Issuer:       cfg.CasdoorURL,
			ClientID:     cfg.ClientID,
			ClientSecret: cfg.ClientSecret,
			RedirectURL:  cfg.RedirectURL,
			StateKey:     cfg.StateKey,
		}),
		store:  store,
		issuer: token.NewIssuer(priv, cfg.TokenTTL),
	}
	srv.install()

	httpSrv := &http.Server{
		Addr:              cfg.Listen,
		Handler:           http.DefaultServeMux,
		ReadHeaderTimeout: 10 * time.Second,
	}
	log.Printf("virgo-auth listening on %s (issuer=%s, proRole=%s, proGroup=%s)", cfg.Listen, cfg.CasdoorURL, cfg.ProRole, cfg.ProGroup)
	if err := httpSrv.ListenAndServe(); err != nil {
		log.Fatal(err)
	}
}

// openStore 按配置选存储：有 DATABASE_URL 就落 PostgreSQL（集群部署），
// 否则退化为内存（本地跑/单测）。集群里绑定必须落盘，否则 Pod 重启即丢。
func openStore(cfg config) (device.Store, error) {
	dsn := strings.TrimSpace(os.Getenv("DATABASE_URL"))
	if dsn == "" {
		log.Print("未设置 DATABASE_URL：绑定表在内存里，进程重启会丢失")
		return device.NewMemory(), nil
	}
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	pg, err := device.NewPG(ctx, dsn)
	if err != nil {
		return nil, err
	}
	log.Print("绑定表落 PostgreSQL")
	return pg, nil
}

// loadOrCreateTokenKey 首次启动生成 Ed25519 私钥并落盘（0600）。私钥只在这台
// 机器上，imgproxy 用导出的公钥验签。
func loadOrCreateTokenKey(path string) (ed25519.PrivateKey, error) {
	if raw, err := os.ReadFile(path); err == nil {
		if len(raw) != ed25519.PrivateKeySize {
			return nil, fmt.Errorf("令牌私钥长度不对（%d 字节）", len(raw))
		}
		return ed25519.PrivateKey(raw), nil
	} else if !errors.Is(err, os.ErrNotExist) {
		return nil, err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return nil, err
	}
	_, priv, err := ed25519.GenerateKey(nil)
	if err != nil {
		return nil, err
	}
	if err := os.WriteFile(path, priv, 0o600); err != nil {
		return nil, err
	}
	return priv, nil
}

func (s *server) install() {
	http.HandleFunc("/healthz", func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusOK, map[string]any{"ok": true})
	})
	http.HandleFunc("/auth/login", s.handleLogin)
	http.HandleFunc("/auth/callback", s.handleCallback)
	http.HandleFunc("/v1/activate", s.handleActivate)
	http.HandleFunc("/v1/refresh", s.handleRefresh)
	http.HandleFunc("/v1/unbind", s.handleUnbind)
	http.HandleFunc("/v1/entitlements", s.handleEntitlements)
}

// ── 登录 ────────────────────────────────────────────────────────────────────

func (s *server) handleLogin(w http.ResponseWriter, r *http.Request) {
	state, err := s.oidc.MakeState()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "state_failed", err.Error())
		return
	}
	target, err := s.oidc.LoginURL(r.Context(), state)
	if err != nil {
		writeErr(w, http.StatusBadGateway, "casdoor_unreachable", err.Error())
		return
	}
	// 客户端回跳目标：桌面 app 用 loopback（RFC 8252 首选，不必注册 OS scheme），
	// 移动端用自定义 scheme；辨识客户端用一条 10 分钟 HttpOnly cookie 带到回调。
	client, err := parseLoginClient(r.URL.Query())
	if err != nil {
		writeErr(w, http.StatusBadRequest, "bad_client", err.Error())
		return
	}
	http.SetCookie(w, &http.Cookie{
		Name:     "virgo_login_client",
		Value:    encodeLoginClient(client),
		Path:     "/",
		HttpOnly: true,
		Secure:   true,
		SameSite: http.SameSiteLaxMode,
		MaxAge:   600,
	})
	http.Redirect(w, r, target, http.StatusFound)
}

// loginClient 描述登录完成后把一次性票据送到哪里。三种形态：
//   - web：普通网页登录，回 JSON；
//   - desktop：自定义 scheme（Android 等移动端），回 virgo://auth?ticket=；
//   - loopback：桌面端本机回环（127.0.0.1:<port>/auth?ticket=&state=），
//     客户端自己起一个短命监听器等这条重定向，无需向操作系统注册 scheme。
//
// port 与 state 由客户端提供，服务端只做白名单校验：端口限 1024–65535，
// state 限 64 个 URL-safe 字符。回跳主机固定 127.0.0.1，客户端无法让它
// 重定向到别的主机。
type loginClient struct {
	kind  string // web | desktop | loopback
	port  int
	state string
}

const (
	loopbackMinPort = 1024
	loopbackMaxPort = 65535
	loginStateMax   = 64
)

func parseLoginClient(q url.Values) (loginClient, error) {
	switch q.Get("client") {
	case "desktop":
		return loginClient{kind: "desktop"}, nil
	case "loopback":
		port, err := strconv.Atoi(q.Get("port"))
		if err != nil || port < loopbackMinPort || port > loopbackMaxPort {
			return loginClient{}, fmt.Errorf("loopback 端口不合法（须在 %d–%d）", loopbackMinPort, loopbackMaxPort)
		}
		state := q.Get("state")
		if !validLoginState(state) {
			return loginClient{}, errors.New("loopback state 不合法（只允许 URL-safe 字符，≤64）")
		}
		return loginClient{kind: "loopback", port: port, state: state}, nil
	default:
		return loginClient{kind: "web"}, nil
	}
}

func validLoginState(state string) bool {
	if len(state) > loginStateMax {
		return false
	}
	for _, r := range state {
		switch {
		case r >= 'a' && r <= 'z', r >= 'A' && r <= 'Z', r >= '0' && r <= '9', r == '-', r == '_':
		default:
			return false
		}
	}
	return true
}

func encodeLoginClient(c loginClient) string {
	if c.kind == "loopback" {
		return "loopback|" + strconv.Itoa(c.port) + "|" + c.state
	}
	if c.kind == "desktop" {
		return "desktop"
	}
	return "web"
}

func decodeLoginClient(raw string) loginClient {
	parts := strings.Split(raw, "|")
	if len(parts) == 3 && parts[0] == "loopback" {
		port, err := strconv.Atoi(parts[1])
		if err == nil && port >= loopbackMinPort && port <= loopbackMaxPort && validLoginState(parts[2]) {
			return loginClient{kind: "loopback", port: port, state: parts[2]}
		}
	}
	if raw == "desktop" {
		return loginClient{kind: "desktop"}
	}
	return loginClient{kind: "web"}
}

// clientCallbackURL 生成登录完成后的回跳目标；false 表示没有客户端回跳，
// 应回 JSON（网页登录）。
func (s *server) clientCallbackURL(c loginClient, ticket string) (string, bool) {
	switch c.kind {
	case "loopback":
		if c.port < loopbackMinPort || c.port > loopbackMaxPort {
			return "", false
		}
		v := url.Values{"ticket": {ticket}, "state": {c.state}}
		return "http://127.0.0.1:" + strconv.Itoa(c.port) + "/auth?" + v.Encode(), true
	case "desktop":
		if s.cfg.ClientScheme == "" {
			return "", false
		}
		v := url.Values{"ticket": {ticket}}
		return s.cfg.ClientScheme + "?" + v.Encode(), true
	}
	return "", false
}

func (s *server) handleCallback(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	code, state := q.Get("code"), q.Get("state")
	if code == "" || !s.oidc.CheckState(state) {
		writeErr(w, http.StatusBadRequest, "bad_state", "登录状态无效或已过期，请重新登录")
		return
	}
	profile, err := s.oidc.Exchange(r.Context(), code)
	if err != nil {
		log.Printf("casdoor exchange failed: %v", err)
		writeErr(w, http.StatusBadGateway, "login_failed", "无法验证登录")
		return
	}
	pro := s.isPro(profile)
	s.setSession(w, profile.Sub, pro)

	// 桌面 / 移动端：把一次性票据交给客户端；网页登录回 JSON。
	// 票据在 5 分钟内有效、绑定 sub 与 Pro 判定，只能换设备令牌，不能直接取图。
	client, _ := r.Cookie("virgo_login_client")
	if client != nil {
		if target, ok := s.clientCallbackURL(decodeLoginClient(client.Value), s.makeTicket(profile.Sub, pro)); ok {
			http.Redirect(w, r, target, http.StatusFound)
			return
		}
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"sub":  profile.Sub,
		"name": profile.DisplayName(),
		"pro":  pro,
	})
}

// isPro 只看 Casdoor 返回的 groups / 角色。判定在服务端，客户端无法伪造。
// 注意：Casdoor 订阅到期后由 cron（每小时）回收组/角色，因此这里只作「当前
// 是否可用」的粗判定；严格到期请配合 /v1/entitlements 读订阅 endTime。
func (s *server) isPro(p *oidc.Profile) bool {
	for _, claim := range append(p.Groups, p.Roles...) {
		if s.cfg.ProGroup != "" && proClaimMatches(claim, s.cfg.ProGroup) {
			return true
		}
		if s.cfg.ProRole != "" && proClaimMatches(claim, s.cfg.ProRole) {
			return true
		}
	}
	return false
}

// proClaimMatches 统一 Casdoor 的两种写法：groups 给 `owner/name` 全名，
// roles 只给短名 `name`，而配置（PRO_GROUP / PRO_ROLE）两种都可能出现。
// 允许「一边全名一边短名」，但两个全名必须同一个 owner，避免跨组织的伪匹配。
func proClaimMatches(claim, want string) bool {
	claim = strings.ToLower(strings.TrimSpace(claim))
	want = strings.ToLower(strings.TrimSpace(want))
	if claim == "" || want == "" {
		return false
	}
	if claim == want {
		return true
	}
	name, full := claimBare(claim)
	wantName, wantFull := claimBare(want)
	if full && wantFull {
		return false
	}
	return name == wantName
}

// claimBare 去掉 owner 前缀，返回短名与「是否带 owner」。
func claimBare(value string) (string, bool) {
	if i := strings.LastIndex(value, "/"); i >= 0 {
		return value[i+1:], true
	}
	return value, false
}

// makeTicket 是短时（5 分钟）一次性票据，登录后桌面端用它换设备绑定。
func (s *server) makeTicket(sub string, pro bool) string {
	raw := fmt.Sprintf("%s|%t|%d", sub, pro, time.Now().Unix())
	sum := sha256.Sum256([]byte(raw + "|" + string(s.cfg.StateKey)))
	return base64.RawURLEncoding.EncodeToString([]byte(raw)) + "." +
		base64.RawURLEncoding.EncodeToString(sum[:16])
}

func (s *server) readTicket(ticket string) (sub string, pro bool, ok bool) {
	parts := strings.Split(ticket, ".")
	if len(parts) != 2 {
		return "", false, false
	}
	rawBytes, err := base64.RawURLEncoding.DecodeString(parts[0])
	if err != nil {
		return "", false, false
	}
	raw := string(rawBytes)
	sum := sha256.Sum256([]byte(raw + "|" + string(s.cfg.StateKey)))
	if subtle.ConstantTimeCompare(
		[]byte(base64.RawURLEncoding.EncodeToString(sum[:16])), []byte(parts[1])) != 1 {
		return "", false, false
	}
	fields := strings.Split(raw, "|")
	if len(fields) != 3 {
		return "", false, false
	}
	var ts int64
	if _, err := fmt.Sscan(fields[2], &ts); err != nil {
		return "", false, false
	}
	if time.Now().Unix()-ts > 300 {
		return "", false, false
	}
	return fields[0], fields[1] == "true", true
}

func (s *server) setSession(w http.ResponseWriter, sub string, pro bool) {
	http.SetCookie(w, &http.Cookie{
		Name:     "virgo_sid",
		Value:    s.makeTicket(sub, pro),
		Path:     "/",
		HttpOnly: true,
		Secure:   true,
		SameSite: http.SameSiteLaxMode,
		MaxAge:   300,
	})
}

func (s *server) session(w http.ResponseWriter, r *http.Request) (string, bool, bool) {
	c, err := r.Cookie("virgo_sid")
	if err != nil {
		return "", false, false
	}
	sub, pro, ok := s.readTicket(c.Value)
	return sub, pro, ok
}

// ── 设备与令牌 ──────────────────────────────────────────────────────────────

type activateRequest struct {
	PublicKey  string `json:"publicKey"`
	DeviceName string `json:"deviceName"`
	Ticket     string `json:"ticket"` // 桌面端用登录票据；网页端走 cookie
}

func (s *server) handleActivate(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		writeErr(w, http.StatusMethodNotAllowed, "method", "只接受 POST")
		return
	}
	var req activateRequest
	if err := json.NewDecoder(http.MaxBytesReader(w, r.Body, 8<<10)).Decode(&req); err != nil {
		writeErr(w, http.StatusBadRequest, "bad_json", err.Error())
		return
	}
	sub, pro, ok := "", false, false
	if req.Ticket != "" {
		sub, pro, ok = s.readTicket(req.Ticket)
	} else {
		sub, pro, ok = s.session(w, r)
	}
	if !ok {
		writeErr(w, http.StatusUnauthorized, "not_signed_in", "请先登录")
		return
	}
	if !pro {
		writeErr(w, http.StatusForbidden, "not_pro", "当前账号不是 Pro 订阅")
		return
	}
	binding, err := s.store.Activate(r.Context(), sub, req.PublicKey, req.DeviceName)
	if errors.Is(err, device.ErrAlreadyBound) {
		writeErr(w, http.StatusConflict, "device_bound", "该账号已绑定另一台设备")
		return
	}
	if err != nil {
		writeErr(w, http.StatusBadRequest, "activate_failed", err.Error())
		return
	}
	keyFP, err := device.Fingerprint(req.PublicKey)
	if err != nil {
		writeErr(w, http.StatusBadRequest, "bad_key", err.Error())
		return
	}
	raw, claims, err := s.issuer.Issue(sub, keyFP, req.PublicKey, "pro")
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "issue_failed", err.Error())
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"token":     raw,
		"expiresAt": claims.ExpiresAt,
		"device":    binding,
	})
}

type refreshRequest struct {
	Sub       string `json:"sub"`
	PublicKey string `json:"publicKey"`
	Nonce     string `json:"nonce"`
	Timestamp int64  `json:"timestamp"`
	Signature string `json:"signature"`
}

func (s *server) handleRefresh(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		writeErr(w, http.StatusMethodNotAllowed, "method", "只接受 POST")
		return
	}
	var req refreshRequest
	if err := json.NewDecoder(http.MaxBytesReader(w, r.Body, 8<<10)).Decode(&req); err != nil {
		writeErr(w, http.StatusBadRequest, "bad_json", err.Error())
		return
	}
	proof := device.Proof{
		Sub:       req.Sub,
		Method:    http.MethodPost,
		Path:      "/v1/refresh",
		Timestamp: req.Timestamp,
		Nonce:     req.Nonce,
		Signature: req.Signature,
	}
	if err := s.store.Verify(r.Context(), proof); err != nil {
		writeErr(w, http.StatusUnauthorized, "bad_proof", err.Error())
		return
	}
	keyFP, err := device.Fingerprint(req.PublicKey)
	if err != nil {
		writeErr(w, http.StatusBadRequest, "bad_key", err.Error())
		return
	}
	raw, claims, err := s.issuer.Issue(req.Sub, keyFP, req.PublicKey, "pro")
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "issue_failed", err.Error())
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"token": raw, "expiresAt": claims.ExpiresAt})
}

type unbindRequest struct {
	Sub       string `json:"sub"`
	PublicKey string `json:"publicKey"`
}

func (s *server) handleUnbind(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		writeErr(w, http.StatusMethodNotAllowed, "method", "只接受 POST")
		return
	}
	var req unbindRequest
	if err := json.NewDecoder(http.MaxBytesReader(w, r.Body, 8<<10)).Decode(&req); err != nil {
		writeErr(w, http.StatusBadRequest, "bad_json", err.Error())
		return
	}
	err := s.store.Unbind(r.Context(), req.Sub, req.PublicKey)
	switch {
	case errors.Is(err, device.ErrUnbindCooldown):
		writeErr(w, http.StatusTooManyRequests, "cooldown", "24 小时内只能解绑一次")
	case errors.Is(err, device.ErrNoBinding):
		writeErr(w, http.StatusNotFound, "no_binding", "没有绑定记录")
	case err != nil:
		writeErr(w, http.StatusBadRequest, "unbind_failed", err.Error())
	default:
		writeJSON(w, http.StatusOK, map[string]any{"ok": true})
	}
}

func (s *server) handleEntitlements(w http.ResponseWriter, r *http.Request) {
	sub, pro, ok := s.session(w, r)
	if !ok {
		writeErr(w, http.StatusUnauthorized, "not_signed_in", "请先登录")
		return
	}
	_, bound := s.store.Binding(r.Context(), sub)
	writeJSON(w, http.StatusOK, map[string]any{"sub": sub, "pro": pro, "deviceBound": bound})
}

// ── 工具 ────────────────────────────────────────────────────────────────────

func writeJSON(w http.ResponseWriter, status int, payload any) {
	body, err := json.Marshal(payload)
	if err != nil {
		http.Error(w, "encode failed", http.StatusInternalServerError)
		return
	}
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(status)
	_, _ = w.Write(body)
}

func writeErr(w http.ResponseWriter, status int, code, message string) {
	writeJSON(w, status, map[string]any{"error": code, "message": message})
}

var _ = context.Background
