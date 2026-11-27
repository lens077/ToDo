# virgo-auth 是静态编译的 Go 二进制，只依赖 CA 证书（调 Casdoor 走 HTTPS）。
FROM alpine:3.21
RUN apk add --no-cache ca-certificates
COPY virgo-auth /usr/local/bin/virgo-auth
# 非 root 运行：读挂载进来的密钥、绑一个非特权端口。
RUN adduser -D -u 10001 virgo
USER 10001:10001
EXPOSE 8766
ENTRYPOINT ["/usr/local/bin/virgo-auth"]
