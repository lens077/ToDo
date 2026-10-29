# sponsor-api 镜像：Node 22+（node:sqlite 内置，无需原生编译）。
# 构建上下文是 apps/sponsor-api/：
#   docker build -f deploy/Dockerfile -t sponsor-api .
FROM node:22-alpine AS deps
WORKDIR /app
COPY package.json ./
# 沙箱密钥等凭据走环境变量注入，不进镜像；镜像里只有代码与依赖。
RUN npm install --omit=dev --no-audit --no-fund

FROM node:22-alpine
# node 镜像自带的非 root 用户（uid 1000）
RUN mkdir -p /data && chown node:node /data
WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY src/ ./src/
ENV NODE_ENV=production \
    SPONSOR_DB_PATH=/data/sponsor.sqlite \
    SPONSOR_PORT=8791
VOLUME ["/data"]
EXPOSE 8791
USER node:node
# 只允许沙箱/生产两个官方网关地址，启动时会再校验一次。
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD wget -qO- http://127.0.0.1:8791/api/healthz || exit 1
ENTRYPOINT ["node", "src/server.mjs"]
