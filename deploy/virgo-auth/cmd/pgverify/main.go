// pgverify 是部署自检：连上 DATABASE_URL 指定的库、建表、读写一条绑定，
// 用来在集群里确认「绑定表真的落盘了」。
package main

import (
	"context"
	"fmt"
	"os"
	"time"

	"virgo-auth/internal/device"
)

// 一个固定的合法 P-256 公钥（SPKI base64），只用于自检。
const pubKey = "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEMWNfiagRD05vu0UovdQ+hzgNNP25L2LfBPEl2NFmJBxQIEbkh79EGAKg5Xh1C3Bg1SQ4BILDnnkmwN5GZFzEew=="

func main() {
	dsn := os.Getenv("DATABASE_URL")
	if dsn == "" {
		fmt.Println("FAIL: DATABASE_URL 未设置")
		os.Exit(1)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	pg, err := device.NewPG(ctx, dsn)
	if err != nil {
		fmt.Println("FAIL 连接/建表:", err)
		os.Exit(1)
	}
	defer pg.Close()
	fmt.Println("OK 连接成功，表就绪")

	existing, ok := pg.Binding(ctx, "verify-user")
	fmt.Printf("读现有绑定: ok=%v %+v\n", ok, existing)

	if _, err := pg.Activate(ctx, "verify-user", pubKey, "verify-pod"); err != nil {
		fmt.Println("FAIL activate:", err)
		os.Exit(1)
	}
	got, ok := pg.Binding(ctx, "verify-user")
	if !ok {
		fmt.Println("FAIL 绑定写入后读不到")
		os.Exit(1)
	}
	fmt.Printf("OK 绑定落盘: sub=%s device=%s\n", got.Sub, got.DeviceName)
}
