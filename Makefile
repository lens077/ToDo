SHELL := /bin/bash
.DEFAULT_GOAL := help

PNPM ?= pnpm
CARGO ?= cargo
OUTPUT_DIR ?= target/release-local

# 官网三端：Cloudflare Pages（海外）、EdgeOne Pages（大陆镜像）、
# virgo.apikv.com（k1 集群，经 Pangolin 转 10.10.31.240:443）。
CF_PROJECT ?= virgo-overseas
EO_PROJECT ?= virgo-site
K8S_IMAGE ?= ccr.ccs.tencentyun.com/sumery/virgo-site
K8S_TAG ?= $(shell git rev-parse --short HEAD)
CHART_DIR ?= web/deploy/chart/virgo

.PHONY: help install dev icon check build bundle-macos package-macos package-macos-signed \
        site-build pages-deploy pages-deploy-eo site-image k8s-deploy argo-apply site-deploy

help: ## List available build and packaging targets
	@printf '%s\n' \
	  'make install               Install locked frontend dependencies' \
	  'make dev                   Start the Tauri desktop app' \
	  'make icon                  Regenerate desktop and PWA icons' \
	  'make check                 Run the complete repository verification gate' \
	  'make build                 Build release CLI and production web assets' \
	  'make bundle-macos          Build Todo.app without creating archives' \
	  'make package-macos         Build local unsigned archives and checksums' \
	  'make package-macos-signed  Sign, notarize, and package release artifacts' \
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

check: ## Run format, tests, clippy, builds, e2e, and diff checks
	./scripts/verify.sh

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

# Landing site ---------------------------------------------------------------

site-build:
	cd web && npm run build

# 海外站。需要本机已登录 Wrangler（`npx wrangler login`）。
pages-deploy: site-build ## Publish the landing site to Cloudflare Pages
	cd web && npx --yes wrangler@4.131.1 pages deploy dist --project-name=$(CF_PROJECT) --branch=main

# 大陆镜像。需要本机已登录 EdgeOne CLI（`edgeone login --site china`）。
pages-deploy-eo: site-build ## Publish the landing site to EdgeOne Pages
	@command -v edgeone >/dev/null || { echo '未安装 EdgeOne CLI：npm i -g --prefix ~/.local edgeone@latest && edgeone login --site china'; exit 1; }
	cd web && edgeone makers deploy dist -n $(EO_PROJECT) -e production

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
