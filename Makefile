.DEFAULT_GOAL := help

export TMPDIR := $(CURDIR)/.local-cache/go-tmp
export GOTMPDIR := $(TMPDIR)
export GOCACHE := $(CURDIR)/.local-cache/go-build

.PHONY: help install dev build dev-web build-web prepare-media typecheck \
	test test-web test-rust test-agent lint fmt fmt-check check \
	agent-build agent-run clean prepare-cache

help:
	@printf '%s\n' \
		'make install        安装前端依赖并准备 FFmpeg' \
		'make dev            启动桌面应用开发环境' \
		'make build          构建桌面应用安装包' \
		'make dev-web        启动前端开发服务' \
		'make build-web      构建前端并检查 TypeScript 类型' \
		'make prepare-media  准备 FFmpeg 和许可证文件' \
		'make typecheck      检查 TypeScript 类型' \
		'make test           运行前端、Rust 和 Edge Agent 测试' \
		'make test-web       运行前端测试' \
		'make test-rust      运行 Rust 测试' \
		'make test-agent     运行 Edge Agent 测试和数据竞争检查' \
		'make lint           运行 Rust Clippy 和 Go vet' \
		'make fmt            格式化 Rust 和 Go 代码' \
		'make fmt-check      检查 Rust 和 Go 代码格式' \
		'make check          检查格式、类型、代码并运行全部测试' \
		'make agent-build    编译 Edge Agent 到 .local-cache/edge-agent' \
		'make agent-run      编译并启动 Edge Agent' \
		'make clean          清理前端和 Rust 构建产物'

prepare-cache:
	@mkdir -p "$(TMPDIR)" "$(GOCACHE)"

install: | prepare-cache
	pnpm install --frozen-lockfile

dev: | prepare-cache
	pnpm tauri dev $(ARGS)

build: | prepare-cache
	pnpm tauri build $(ARGS)

dev-web: | prepare-cache
	pnpm dev

build-web: | prepare-cache
	pnpm build

prepare-media: | prepare-cache
	pnpm prepare:media

typecheck: | prepare-cache
	pnpm typecheck

test: test-web test-rust test-agent

test-web: | prepare-cache
	pnpm test

test-rust: | prepare-cache
	cargo test --manifest-path src-tauri/Cargo.toml

test-agent: | prepare-cache
	cd apps/edge-agent && go test -race ./...

lint: | prepare-cache
	cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
	cd apps/edge-agent && go vet ./...

fmt: | prepare-cache
	cargo fmt --manifest-path src-tauri/Cargo.toml
	cd apps/edge-agent && go fmt ./...

fmt-check: | prepare-cache
	cargo fmt --manifest-path src-tauri/Cargo.toml --check
	gofmt -d apps/edge-agent
	test -z "$$(gofmt -l apps/edge-agent)"

check: fmt-check typecheck lint test

agent-build: | prepare-cache
	cd apps/edge-agent && go build -o ../../.local-cache/edge-agent ./cmd/edge-agent

agent-run: agent-build
	.local-cache/edge-agent

clean: | prepare-cache
	cargo clean --manifest-path src-tauri/Cargo.toml
	rm -rf dist .tanstack
