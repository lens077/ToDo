SHELL := /bin/bash
.DEFAULT_GOAL := help

PNPM ?= pnpm
CARGO ?= cargo
OUTPUT_DIR ?= target/release-local

.PHONY: help install dev icon check build bundle-macos package-macos package-macos-signed

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

bundle-macos: ## Build target/release/bundle/macos/Todo.app
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
