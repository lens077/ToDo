package main

import (
	"net/url"
	"strings"
	"testing"

	"virgo-auth/internal/oidc"
)

func TestParseLoginClient(t *testing.T) {
	cases := []struct {
		name    string
		query   string
		want    loginClient
		wantErr bool
	}{
		{name: "web 默认", query: "", want: loginClient{kind: "web"}},
		{name: "未知 client 退化为 web", query: "client=web", want: loginClient{kind: "web"}},
		{name: "desktop", query: "client=desktop", want: loginClient{kind: "desktop"}},
		{
			name:  "loopback 正常",
			query: "client=loopback&port=53001&state=abc_123-XYZ",
			want:  loginClient{kind: "loopback", port: 53001, state: "abc_123-XYZ"},
		},
		{name: "loopback 端口过小", query: "client=loopback&port=80&state=n", wantErr: true},
		{name: "loopback 端口过大", query: "client=loopback&port=70000&state=n", wantErr: true},
		{name: "loopback 端口缺失", query: "client=loopback&state=n", wantErr: true},
		{name: "loopback 端口非数字", query: "client=loopback&port=abc&state=n", wantErr: true},
		{name: "loopback state 含分隔符", query: "client=loopback&port=53001&state=a|b", wantErr: true},
		{name: "loopback state 过长", query: "client=loopback&port=53001&state=" + strings.Repeat("a", 65), wantErr: true},
		{name: "loopback state 可为空", query: "client=loopback&port=53001", want: loginClient{kind: "loopback", port: 53001}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			q, err := url.ParseQuery(tc.query)
			if err != nil {
				t.Fatalf("查询串无法解析：%v", err)
			}
			got, err := parseLoginClient(q)
			if tc.wantErr {
				if err == nil {
					t.Fatalf("期望报错，实际得到 %+v", got)
				}
				return
			}
			if err != nil {
				t.Fatalf("未预期的错误：%v", err)
			}
			if got != tc.want {
				t.Fatalf("得到 %+v，期望 %+v", got, tc.want)
			}
		})
	}
}

// cookie 里的编码必须能不带网络地往返：回调只能看到 cookie，而解析一旦
// 放宽就会把别的客户端的回跳目标带错。
func TestEncodeDecodeLoginClient(t *testing.T) {
	cases := []loginClient{
		{kind: "web"},
		{kind: "desktop"},
		{kind: "loopback", port: 1024},
		{kind: "loopback", port: 65535, state: "abcdefghijklmnopqrstuvwxyz0123456789_-"},
	}
	for _, want := range cases {
		encoded := encodeLoginClient(want)
		if got := decodeLoginClient(encoded); got != want {
			t.Fatalf("往返失败：%q 解出 %+v，期望 %+v", encoded, got, want)
		}
	}
	// 手工改坏的 cookie 不能变成 loopback 回跳：退回网页登录而不是接受任意端口。
	for _, raw := range []string{
		"loopback|80|x",
		"loopback|70000|x",
		"loopback|53001|a|b",
		"desktop|x",
		"loopback",
	} {
		if got := decodeLoginClient(raw); got.kind == "loopback" {
			t.Fatalf("损坏的 cookie %q 不该被解析成 loopback：%+v", raw, got)
		}
	}
}

func TestClientCallbackURL(t *testing.T) {
	const ticket = "c3Vi.inRydWU"
	s := &server{cfg: config{ClientScheme: "virgo://auth"}}

	t.Run("loopback 主机固定 127.0.0.1 且回送 state", func(t *testing.T) {
		target, ok := s.clientCallbackURL(loginClient{kind: "loopback", port: 53001, state: "XYZ"}, ticket)
		if !ok {
			t.Fatal("期望有回跳目标")
		}
		u, err := url.Parse(target)
		if err != nil {
			t.Fatalf("回跳地址无法解析：%v", err)
		}
		if u.Scheme != "http" || u.Host != "127.0.0.1:53001" || u.Path != "/auth" {
			t.Fatalf("回跳地址形态不对：%s", target)
		}
		if got := u.Query().Get("ticket"); got != ticket {
			t.Fatalf("ticket 丢失：%q", got)
		}
		if got := u.Query().Get("state"); got != "XYZ" {
			t.Fatalf("state 未回送：%q", got)
		}
	})

	t.Run("loopback 不接受任意端口", func(t *testing.T) {
		for _, port := range []int{0, 22, 1023, 65536} {
			if target, ok := s.clientCallbackURL(loginClient{kind: "loopback", port: port, state: "s"}, ticket); ok {
				t.Fatalf("端口 %d 不该生成回跳：%s", port, target)
			}
		}
	})

	t.Run("desktop 走 scheme", func(t *testing.T) {
		target, ok := s.clientCallbackURL(loginClient{kind: "desktop"}, ticket)
		if !ok {
			t.Fatal("期望有回跳目标")
		}
		if !strings.HasPrefix(target, "virgo://auth?ticket=") {
			t.Fatalf("scheme 回跳形态不对：%s", target)
		}
	})

	t.Run("desktop 未配置 scheme 时回 JSON", func(t *testing.T) {
		bare := &server{}
		if target, ok := bare.clientCallbackURL(loginClient{kind: "desktop"}, ticket); ok {
			t.Fatalf("未配置 scheme 不该回跳：%s", target)
		}
	})

	t.Run("web 回 JSON", func(t *testing.T) {
		if target, ok := s.clientCallbackURL(loginClient{kind: "web"}, ticket); ok {
			t.Fatalf("网页登录不该回跳：%s", target)
		}
	})
}

func TestProClaimMatches(t *testing.T) {
	cases := []struct {
		claim string
		want  string
		ok    bool
	}{
		{"lens/virgo_pro", "lens/virgo_pro", true},
		{"virgo_pro", "lens/virgo_pro", true}, // Casdoor roles 只给短名
		{"lens/virgo_pro", "virgo_pro", true}, // 配置写短名也能命中组
		{"Lens/Virgo_Pro", "lens/virgo_pro", true},
		{"lens/virgo_pro", "other/virgo_pro", false}, // 跨组织不命中
		{"lens/beta-pro", "lens/virgo-pro", false},
		{"", "virgo_pro", false},
		{"virgo_pro", "", false},
	}
	for _, tc := range cases {
		if got := proClaimMatches(tc.claim, tc.want); got != tc.ok {
			t.Fatalf("proClaimMatches(%q, %q) = %v，期望 %v", tc.claim, tc.want, got, tc.ok)
		}
	}
}

func TestIsProReadsGroupsAndRoles(t *testing.T) {
	s := &server{cfg: config{ProGroup: "lens/virgo-pro", ProRole: "lens/virgo_pro"}}
	if !s.isPro(&oidc.Profile{Roles: []string{"admin", "customer", "virgo_pro"}}) {
		t.Fatal("roles 里的 virgo_pro 应判为 Pro")
	}
	if !s.isPro(&oidc.Profile{Groups: []string{"lens/virgo-pro"}}) {
		t.Fatal("groups 里的 virgo-pro 应判为 Pro")
	}
	if s.isPro(&oidc.Profile{Groups: []string{"lens/beta-pro"}, Roles: []string{"customer"}}) {
		t.Fatal("beta-pro / customer 不应判为 Pro")
	}
	if s.isPro(&oidc.Profile{}) {
		t.Fatal("空 profile 不应判为 Pro")
	}
}
