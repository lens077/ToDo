SHELL := /bin/bash
.DEFAULT_GOAL := help

PNPM ?= pnpm
CARGO ?= cargo
OUTPUT_DIR ?= target/release-local
VERSION ?= $(shell node -p "require('./apps/desktop/src-tauri/tauri.conf.json').version")
BUILD_TARGET ?= ./scripts/build-target.sh

# 官网三端：Cloudflare Pages（海外）、EdgeOne Pages（大陆镜像）、
# virgo.apikv.com（k1 集群，经 Pangolin 转 10.10.31.240:443）。
CF_PROJECT ?= virgo-overseas
EO_PROJECT ?= virgo-site
K8S_IMAGE ?= ccr.ccs.tencentyun.com/sumery/virgo-site
K8S_TAG ?= $(shell git rev-parse --short HEAD)
CHART_DIR ?= web/deploy/chart/virgo

.PHONY: help install dev icon check check-quick hooks build bundle-macos package-macos package-macos-signed \
        comm-build other-build build-mac-arm64 build-mac-x64 build-win-x64 build-win-arm64 build-linux-x64 \
        site-build pages-deploy pages-deploy-eo site-image k8s-deploy argo-apply site-deploy

help: ## List available build and packaging targets
	@printf '%s\n' \
	  'make install               Install locked frontend dependencies' \
	  'make dev                   Start the Tauri desktop app' \
	  'make icon                  Regenerate desktop and PWA icons' \
	  'make check                 Run the complete repository verification gate (manual, slow)' \
	  'make check-quick           Run the pre-push gate (target < 2 min)' \
	  'make hooks                 Install the tracked pre-push hook' \
	  'make build                 Build release CLI and production web assets' \
	  'make bundle-macos          Build Todo.app without creating archives' \
	  'make package-macos         Build local unsigned archives and checksums' \
	  'make package-macos-signed  Sign, notarize, and package release artifacts' \
	  'make comm-build            常用集合（按宿主：mac arm64 / win x64）' \
	  'make other-build           其余集合（按宿主：linux x64 / mac intel / win arm64）' \
	  '' \
	  '官网（三端）：' \
	  'make site-deploy          构建并发布到全部三端' \
	  'make pages-deploy         Cloudflare Pages（virgo-overseas）' \
	  'make pages-deploy-eo      EdgeOne Pages（virgo-site）' \
	  'make k8s-deploy           virgo.apikv.com（k1 集群，立即 helm 部署）' \
	  'make argo-apply           注册 ArgoCD Application（GitOps）' \
	  '' \
	  'Override the local package output directory with:' \
	  '  make package-macos OUTPUT_DIR=target/my-release'

install: ## Install dependencies exactly as recorded in pnpm-lock.yaml
	$(PNPM) install --frozen-lockfile

# Development ---------------------------------------------------------------

dev: ## Start Vite and the native Tauri shell
	$(PNPM) --filter @todo/desktop tauri dev

icon: ## Regenerate all native and PWA icon sizes
	$(PNPM) --filter @todo/desktop icon

check: ## Run format, tests, clippy, builds, e2e, and diff checks (manual, slow)
	./scripts/verify.sh

check-quick: ## Run the pre-push gate (Rust + web, target under 2 minutes)
	./scripts/verify-quick.sh

hooks: ## Install the tracked pre-push hook
	git config core.hooksPath .githooks
	@echo 'pre-push hook installed: .githooks/pre-push'

# Build and package ----------------------------------------------------------

build: ## Build the release CLI and production web assets
	$(CARGO) build --locked --release -p todo-cli
	$(PNPM) --filter @todo/desktop build

bundle-macos: ## Build "target/release/bundle/macos/Sumery Virgo.app"
	env \
	  -u APPLE_SIGNING_IDENTITY \
	  -u APPLE_ID \
	  -u APPLE_PASSWORD \
	  -u APPLE_TEAM_ID \
	  -u APPLE_API_ISSUER \
	  -u APPLE_API_KEY \
	  -u APPLE_API_KEY_PATH \
	  $(PNPM) --filter @todo/desktop tauri build --bundles app

package-macos: ## Build ad-hoc signed local archives; do not distribute them
	OUTPUT_DIR="$(abspath $(OUTPUT_DIR))" SIGN_AND_NOTARIZE=0 ./scripts/package-macos.sh

package-macos-signed: ## Build Developer ID signed and notarized release archives
	OUTPUT_DIR="$(abspath $(OUTPUT_DIR))" SIGN_AND_NOTARIZE=1 ./scripts/package-macos.sh

# 桌面端多平台构建。Tauri 安装包只能在对应宿主上生成：
# mac-* 在 macOS，win-* 在 Windows，linux-* 在 Linux。
# 在错误宿主上会明确报错，不假装成功。
# comm-build / other-build 按宿主选目标：一条命令在当前机器上永远可执行，
# 对应集合里的另一个平台请在它的宿主或发布 runner 上跑。
comm-build: ## 常用集合（按宿主：mac arm64 / win x64）
	@case "$$(uname -s)" in \
	  Darwin) $(MAKE) --no-print-directory build-mac-arm64 ;; \
	  MINGW*|MSYS*|CYGWIN*|Windows_NT) $(MAKE) --no-print-directory build-win-x64 ;; \
	  *) echo "comm-build: 请在 macOS 或 Windows 上运行" >&2; exit 1 ;; \
	esac
other-build: ## 其余集合（按宿主：linux x64 / mac intel / win arm64）
	@case "$$(uname -s)" in \
	  Darwin) $(MAKE) --no-print-directory build-mac-x64 ;; \
	  Linux) $(MAKE) --no-print-directory build-linux-x64 ;; \
	  MINGW*|MSYS*|CYGWIN*|Windows_NT) $(MAKE) --no-print-directory build-win-arm64 ;; \
	  *) echo "other-build: 无法识别宿主" >&2; exit 1 ;; \
	esac
build-mac-arm64: ## macOS Apple Silicon
	$(BUILD_TARGET) mac-arm64
build-mac-x64: ## macOS Intel
	$(BUILD_TARGET) mac-x64
build-win-x64: ## Windows x64
	$(BUILD_TARGET) win-x64
build-win-arm64: ## Windows ARM64
	$(BUILD_TARGET) win-arm64
build-linux-x64: ## Linux x64
	$(BUILD_TARGET) linux-x64

# Landing site ---------------------------------------------------------------

site-build:
	cd web && pnpm run build

# 海外站。需要本机已登录 Wrangler（`pnpm dlx wrangler@4.152.0 login`）。
pages-deploy: site-build ## Publish the landing site to Cloudflare Pages
	cd web && pnpm dlx wrangler@4.152.0 pages deploy dist --project-name=$(CF_PROJECT) --branch=main

# 大陆镜像。需要本机已登录 EdgeOne CLI（`edgeone login --site china`）。
pages-deploy-eo: site-build ## Publish the landing site to EdgeOne Pages
	cd web && pnpm dlx edgeone@1.6.46 makers deploy dist -n $(EO_PROJECT) -e production

# virgo.apikv.com：把静态站打进镜像，部署到 k1 集群，经 default/cilium-gateway
# 的 https 监听提供；Pangolin 资源把该域名转到 10.10.31.240:443（保留 Host）。
site-image: site-build
	docker buildx build --platform linux/amd64 -f web/deploy/k8s/Dockerfile \
	  -t $(K8S_IMAGE):$(K8S_TAG) --push web

# 立即部署（bootstrap / 回滚用）。日常发布走 CI：CI 回写 values-prod.yaml，
# ArgoCD 从 Git 同步。手工 helm 升级后，ArgoCD 下次同步会以 Git 里的 tag 为准。
k8s-deploy: site-image ## Deploy the landing site to the k1 cluster now
	kubectl apply -f deploy/argocd/namespace.yaml
	helm upgrade --install virgo $(CHART_DIR) -n virgo \
	  -f $(CHART_DIR)/values.yaml -f $(CHART_DIR)/values-prod.yaml \
	  --set web.image.tag=$(K8S_TAG) --wait --timeout 5m
	kubectl -n virgo rollout status deploy/virgo --timeout=180s

# 注册 ArgoCD Application；之后 main 分支的 values 变化由 ArgoCD 自动同步。
argo-apply: ## Register the ArgoCD project and application
	kubectl apply -f deploy/argocd/namespace.yaml
	kubectl apply -f deploy/argocd/project.yaml
	kubectl apply -f deploy/argocd/application.yaml

site-deploy: pages-deploy pages-deploy-eo k8s-deploy ## Deploy the landing site to all three origins
