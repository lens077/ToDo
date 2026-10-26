# 静态站镜像：构建产物由 web/build.mjs 生成，构建上下文是 web/。
# nginx-unprivileged 以非 root 运行、默认监听 8080；k8s 里由 chart 的
# ConfigMap 覆盖站点配置，下面这份是直接 docker run 时的默认站。
FROM nginxinc/nginx-unprivileged:1.27-alpine
COPY dist/ /usr/share/nginx/html/
EXPOSE 8080
