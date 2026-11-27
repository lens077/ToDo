// Package oidc 只实现 Virgo 需要的三件事：拼授权地址、code 换 token、取
// userinfo。照抄 Scorpius `server/internal/auth/casdoor.go` 的形状——client
// secret 只在服务端，永不下发客户端。
package oidc

import (
	"context"
	"crypto/hmac"
	crand "crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"sync"
	"time"
)

var ErrBadState = errors.New("oidc: bad or expired state")

// Profile 是业务需要的 userinfo 子集。Groups 用来判断 Pro：Casdoor 的
// userinfo 在 scope 含 profile 时会带 groups，值是 `lens/beta-pro` 这种全名。
type Profile struct {
	Sub           string   `json:"sub"`
	Email         string   `json:"email"`
	EmailVerified bool     `json:"email_verified"`
	Name          string   `json:"name"`
	PreferredName string   `json:"preferred_username"`
	Groups        []string `json:"groups"`
	// Casdoor 的 userinfo 还会返回 roles，且只给短名（如 `virgo_pro`）；
	// 订阅权限在 Casdoor 里是角色而不是组，所以必须一起判。
	Roles []string `json:"roles"`
}

func (p *Profile) DisplayName() string {
	if p.Name != "" {
		return p.Name
	}
	if p.PreferredName != "" {
		return p.PreferredName
	}
	return p.Sub
}

type Config struct {
	Issuer       string
	ClientID     string
	ClientSecret string
	RedirectURL  string
	// StateKey 用来签 state，防止 CSRF；由调用方从环境变量注入。
	StateKey []byte
	HTTP     *http.Client
}

type Client struct {
	cfg  Config
	http *http.Client

	once sync.Once
	disc *discovery
	derr error
}

type discovery struct {
	AuthorizationEndpoint string `json:"authorization_endpoint"`
	TokenEndpoint         string `json:"token_endpoint"`
	UserinfoEndpoint      string `json:"userinfo_endpoint"`
	JWKSURI               string `json:"jwks_uri"`
}

func New(cfg Config) *Client {
	hc := cfg.HTTP
	if hc == nil {
		hc = &http.Client{Timeout: 15 * time.Second}
	}
	return &Client{cfg: cfg, http: hc}
}

func (c *Client) discover(ctx context.Context) (*discovery, error) {
	c.once.Do(func() {
		req, err := http.NewRequestWithContext(ctx, http.MethodGet,
			strings.TrimRight(c.cfg.Issuer, "/")+"/.well-known/openid-configuration", nil)
		if err != nil {
			c.derr = err
			return
		}
		resp, err := c.http.Do(req)
		if err != nil {
			c.derr = err
			return
		}
		defer resp.Body.Close()
		if resp.StatusCode != http.StatusOK {
			c.derr = fmt.Errorf("oidc: discovery %s", resp.Status)
			return
		}
		var d discovery
		if err := json.NewDecoder(io.LimitReader(resp.Body, 1<<20)).Decode(&d); err != nil {
			c.derr = err
			return
		}
		if d.AuthorizationEndpoint == "" || d.TokenEndpoint == "" || d.UserinfoEndpoint == "" {
			c.derr = errors.New("oidc: discovery lacks endpoints")
			return
		}
		c.disc = &d
	})
	return c.disc, c.derr
}

// MakeState 生成 `<随机串>.<时间戳>.<签名>`：state 不落库，靠 HMAC 自证，
// 十分钟内有效。
func (c *Client) MakeState() (string, error) {
	nonce := make([]byte, 24)
	if _, err := crand.Read(nonce); err != nil {
		return "", err
	}
	payload := fmt.Sprintf("%x.%d", nonce, time.Now().Unix())
	return payload + "." + c.sign(payload), nil
}

// CheckState 校验 state 的签名与时间窗。
func (c *Client) CheckState(state string) bool {
	parts := strings.Split(state, ".")
	if len(parts) != 3 {
		return false
	}
	payload := parts[0] + "." + parts[1]
	if !hmac.Equal([]byte(c.sign(payload)), []byte(parts[2])) {
		return false
	}
	sec, err := strconv.ParseInt(parts[1], 10, 64)
	if err != nil {
		return false
	}
	return time.Now().Before(time.Unix(sec, 0).Add(10 * time.Minute))
}

func (c *Client) sign(payload string) string {
	mac := hmac.New(sha256.New, c.cfg.StateKey)
	mac.Write([]byte(payload))
	return base64.RawURLEncoding.EncodeToString(mac.Sum(nil))
}

func (c *Client) LoginURL(ctx context.Context, state string) (string, error) {
	d, err := c.discover(ctx)
	if err != nil {
		return "", err
	}
	q := url.Values{}
	q.Set("client_id", c.cfg.ClientID)
	q.Set("response_type", "code")
	q.Set("redirect_uri", c.cfg.RedirectURL)
	// groups 需要随 userinfo 返回，Pro 判定依赖它。
	q.Set("scope", "openid profile email")
	q.Set("state", state)
	return d.AuthorizationEndpoint + "?" + q.Encode(), nil
}

// Exchange 用 code 换 token 并取 userinfo。access token 用完即弃，不存。
func (c *Client) Exchange(ctx context.Context, code string) (*Profile, error) {
	d, err := c.discover(ctx)
	if err != nil {
		return nil, err
	}
	form := url.Values{}
	form.Set("grant_type", "authorization_code")
	form.Set("code", code)
	form.Set("redirect_uri", c.cfg.RedirectURL)
	form.Set("client_id", c.cfg.ClientID)
	form.Set("client_secret", c.cfg.ClientSecret)
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, d.TokenEndpoint, strings.NewReader(form.Encode()))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	req.Header.Set("Accept", "application/json")
	resp, err := c.http.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("oidc: token exchange %s: %s", resp.Status, truncate(body, 200))
	}
	var tok struct {
		AccessToken string `json:"access_token"`
	}
	if err := json.Unmarshal(body, &tok); err != nil {
		return nil, fmt.Errorf("oidc: bad token response: %w", err)
	}
	if tok.AccessToken == "" {
		return nil, errors.New("oidc: empty access_token")
	}
	return c.userinfo(ctx, d.UserinfoEndpoint, tok.AccessToken)
}

func (c *Client) userinfo(ctx context.Context, endpoint, accessToken string) (*Profile, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Authorization", "Bearer "+accessToken)
	req.Header.Set("Accept", "application/json")
	resp, err := c.http.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("oidc: userinfo %s: %s", resp.Status, truncate(body, 200))
	}
	var p Profile
	if err := json.Unmarshal(body, &p); err != nil {
		return nil, fmt.Errorf("oidc: bad userinfo: %w", err)
	}
	if p.Sub == "" {
		return nil, errors.New("oidc: userinfo has no sub")
	}
	return &p, nil
}

func truncate(b []byte, n int) string {
	s := strings.TrimSpace(string(b))
	if len(s) > n {
		return s[:n] + "…"
	}
	return s
}
