# ==============================================================================
# Window Hop - Makefile
# ==============================================================================
# Commands for building development executables, release packages, installer .exe,
# running quality checks, and managing project versioning.
# Requires only Cargo and Rust toolchain (no Python required).
# ==============================================================================

SHELL := /bin/bash

# Extract current project version directly from Cargo.toml (pure shell, instant)
VERSION ?= $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)

# Target triple configuration (e.g. x86_64-pc-windows-msvc or empty for host)
TARGET ?=

# Detect executable extension based on host OS or specified target
ifeq ($(OS),Windows_NT)
    HOST_OS := Windows
    EXE_EXT := .exe
else
    HOST_OS := $(shell uname -s)
    ifneq (,$(findstring windows,$(TARGET)))
        EXE_EXT := .exe
    else
        EXE_EXT :=
    endif
endif

# Cargo flags
ifdef TARGET
    CARGO_TARGET_FLAG := --target $(TARGET)
    DEBUG_DIR := target/$(TARGET)/debug
    RELEASE_DIR := target/$(TARGET)/release
else
    CARGO_TARGET_FLAG :=
    DEBUG_DIR := target/debug
    RELEASE_DIR := target/release
endif

DIST_DIR := dist
ZIP_NAME := window-hop-v$(VERSION)-portable.zip
PORTABLE_ZIP := window-hop-v$(VERSION)-portable.zip
SETUP_EXE := WindowHop-Setup$(EXE_EXT)
SETUP_VERSIONED_EXE := WindowHop-Setup-v$(VERSION)$(EXE_EXT)
NSIS_BIN := $(shell which makensis 2>/dev/null || echo "")
VERSION_TOOL := $(DEBUG_DIR)/version-tool$(EXE_EXT)

.PHONY: all help run run-dev run-installer run-release dev build-dev release build-release installer package package-dev \
        test check fmt clean version version-full set-version bump-patch bump-minor bump-major tag

all: help

help:
	@echo "Window Hop Build and Versioning Helper"
	@echo "Current version: $(VERSION)"
	@echo ""
	@echo "Run targets (Development):"
	@echo "  make run             Run Window Hop GUI application in dev mode (alias: make run-dev)"
	@echo "  make run-installer   Run installer in dev mode (e.g. make run-installer ARGS=\"--help\")"
	@echo "  make run-release     Run Window Hop application in release mode"
	@echo ""
	@echo "Build targets:"
	@echo "  make dev             Build development executables (window-hop$(EXE_EXT), window-hop-installer$(EXE_EXT))"
	@echo "  make dev-installer   Build development installer executable only"
	@echo "  make release         Build optimized release executables"
	@echo "  make installer       Build optimized installer executable only"
	@echo "  make package         Package release executables and guide into $(DIST_DIR)/ and create .zip"
	@echo "  make package-dev     Package development executables into $(DIST_DIR)/"
	@echo ""
	@echo "Testing and verification:"
	@echo "  make test            Run all Cargo unit and integration tests"
	@echo "  make check           Run cargo check, cargo fmt check, and cargo clippy (-D warnings)"
	@echo "  make fmt             Format code using cargo fmt"
	@echo "  make clean           Clean build artifacts and $(DIST_DIR)/"
	@echo ""
	@echo "Versioning targets (powered by pure Rust / Node helper):"
	@echo "  make version         Print current version"
	@echo "  make version-full    Print current version with git commit hash"
	@echo "  make set-version VERSION=x.y.z  Update version in Cargo.toml and Cargo.lock"
	@echo "  make bump-patch      Increment patch version (e.g. 0.1.0 -> 0.1.1)"
	@echo "  make bump-minor      Increment minor version (e.g. 0.1.0 -> 0.2.0)"
	@echo "  make bump-major      Increment major version (e.g. 0.1.0 -> 1.0.0)"
	@echo "  make tag             Create git annotated tag for current version (v$(VERSION))"
	@echo ""
	@echo "Options:"
	@echo "  ARGS=<flags>         Pass custom arguments (e.g. make run ARGS=\"--startup\")"
	@echo "  TARGET=<triple>      Optional cross-compilation target (e.g. x86_64-pc-windows-msvc)"

# ------------------------------------------------------------------------------
# Run targets (Development)
# ------------------------------------------------------------------------------

run: run-dev

run-dev:
	@echo "==> Running Window Hop (profile: dev)..."
	cargo run $(CARGO_TARGET_FLAG) --bin window-hop -- $(ARGS)

run-installer:
	@echo "==> Running Window Hop Installer (profile: dev)..."
	cargo run $(CARGO_TARGET_FLAG) --bin window-hop-installer -- $(ARGS)

run-release:
	@echo "==> Running Window Hop (profile: release)..."
	cargo run --release $(CARGO_TARGET_FLAG) --bin window-hop -- $(ARGS)

# ------------------------------------------------------------------------------
# Build targets
# ------------------------------------------------------------------------------

dev: build-dev

build-dev:
	@echo "==> Building development executables (profile: dev)..."
	cargo build $(CARGO_TARGET_FLAG) --bin window-hop --bin window-hop-installer
	@echo "Development binaries built at $(DEBUG_DIR)/"

dev-installer:
	@echo "==> Building development installer executable..."
	cargo build $(CARGO_TARGET_FLAG) --bin window-hop-installer
	@echo "Development installer built at $(DEBUG_DIR)/window-hop-installer$(EXE_EXT)"

release: build-release

build-release:
	@echo "==> Building release application executable..."
	cargo build --release $(CARGO_TARGET_FLAG) --bin window-hop
	@echo "==> Building self-contained bundled installer with embedded payload..."
	WINDOW_HOP_PAYLOAD_PATH=$(RELEASE_DIR)/window-hop$(EXE_EXT) cargo build --release $(CARGO_TARGET_FLAG) --bin window-hop-installer
	@echo "Release binaries built at $(RELEASE_DIR)/"

installer:
	@echo "==> Building release application for installer payload..."
	cargo build --release $(CARGO_TARGET_FLAG) --bin window-hop
	@echo "==> Building self-contained release installer executable..."
	WINDOW_HOP_PAYLOAD_PATH=$(RELEASE_DIR)/window-hop$(EXE_EXT) cargo build --release $(CARGO_TARGET_FLAG) --bin window-hop-installer
	@echo "Release installer built at $(RELEASE_DIR)/window-hop-installer$(EXE_EXT)"
	@if [ -n "$(NSIS_BIN)" ]; then \
		mkdir -p $(DIST_DIR); \
		cp $(RELEASE_DIR)/window-hop$(EXE_EXT) $(DIST_DIR)/; \
		cp docs/install-guide.md $(DIST_DIR)/INSTALL-GUIDE.txt; \
		echo "==> Compiling NSIS installer $(DIST_DIR)/$(SETUP_EXE)..."; \
		$(NSIS_BIN) -DVERSION=$(VERSION) -DOUTFILE=../$(DIST_DIR)/$(SETUP_EXE) installer/installer.nsi; \
		cp $(DIST_DIR)/$(SETUP_EXE) $(DIST_DIR)/$(SETUP_VERSIONED_EXE); \
	fi

# ------------------------------------------------------------------------------
# Packaging
# ------------------------------------------------------------------------------

package: release
	@echo "==> Packaging release artifacts into $(DIST_DIR)/..."
	@mkdir -p $(DIST_DIR)
	@cp $(RELEASE_DIR)/window-hop$(EXE_EXT) $(DIST_DIR)/
	@cp $(RELEASE_DIR)/window-hop-installer$(EXE_EXT) $(DIST_DIR)/
	@cp docs/install-guide.md $(DIST_DIR)/INSTALL-GUIDE.txt
	@if [ -n "$(NSIS_BIN)" ]; then \
		echo "==> Compiling NSIS installer $(DIST_DIR)/$(SETUP_EXE)..."; \
		$(NSIS_BIN) -DVERSION=$(VERSION) -DOUTFILE=../$(DIST_DIR)/$(SETUP_EXE) installer/installer.nsi; \
		cp $(DIST_DIR)/$(SETUP_EXE) $(DIST_DIR)/$(SETUP_VERSIONED_EXE); \
	else \
		echo "==> makensis not found, skipping NSIS setup compiler. Bundled window-hop-installer$(EXE_EXT) is available."; \
	fi
	@echo "==> Creating portable distribution archive $(DIST_DIR)/$(PORTABLE_ZIP)..."
	@cd $(DIST_DIR) && \
	if command -v zip >/dev/null 2>&1; then \
		zip -q $(PORTABLE_ZIP) window-hop$(EXE_EXT) INSTALL-GUIDE.txt; \
	elif command -v powershell.exe >/dev/null 2>&1; then \
		powershell.exe -NoProfile -Command "Compress-Archive -Path window-hop$(EXE_EXT),INSTALL-GUIDE.txt -DestinationPath $(PORTABLE_ZIP) -Force"; \
	elif command -v pwsh >/dev/null 2>&1; then \
		pwsh -NoProfile -Command "Compress-Archive -Path window-hop$(EXE_EXT),INSTALL-GUIDE.txt -DestinationPath $(PORTABLE_ZIP) -Force"; \
	elif command -v 7z >/dev/null 2>&1; then \
		7z a -bd -bso0 $(PORTABLE_ZIP) window-hop$(EXE_EXT) INSTALL-GUIDE.txt; \
	else \
		tar -czf window-hop-v$(VERSION)-portable.tar.gz window-hop$(EXE_EXT) INSTALL-GUIDE.txt; \
	fi
	@echo "==> Package ready:"
	@ls -lh $(DIST_DIR)

package-dev: build-dev
	@echo "==> Packaging development artifacts into $(DIST_DIR)/..."
	@mkdir -p $(DIST_DIR)
	@cp $(DEBUG_DIR)/window-hop$(EXE_EXT) $(DIST_DIR)/
	@cp $(DEBUG_DIR)/window-hop-installer$(EXE_EXT) $(DIST_DIR)/
	@cp docs/install-guide.md $(DIST_DIR)/INSTALL-GUIDE.txt
	@echo "==> Creating development archive $(DIST_DIR)/$(PORTABLE_ZIP)..."
	@cd $(DIST_DIR) && \
	if command -v zip >/dev/null 2>&1; then \
		zip -q $(PORTABLE_ZIP) window-hop$(EXE_EXT) INSTALL-GUIDE.txt; \
	elif command -v powershell.exe >/dev/null 2>&1; then \
		powershell.exe -NoProfile -Command "Compress-Archive -Path window-hop$(EXE_EXT),INSTALL-GUIDE.txt -DestinationPath $(PORTABLE_ZIP) -Force"; \
	elif command -v pwsh >/dev/null 2>&1; then \
		pwsh -NoProfile -Command "Compress-Archive -Path window-hop$(EXE_EXT),INSTALL-GUIDE.txt -DestinationPath $(PORTABLE_ZIP) -Force"; \
	elif command -v 7z >/dev/null 2>&1; then \
		7z a -bd -bso0 $(PORTABLE_ZIP) window-hop$(EXE_EXT) INSTALL-GUIDE.txt; \
	else \
		tar -czf window-hop-v$(VERSION)-portable.tar.gz window-hop$(EXE_EXT) INSTALL-GUIDE.txt; \
	fi
	@echo "==> Development package ready:"
	@ls -lh $(DIST_DIR)

# ------------------------------------------------------------------------------
# Testing and Verification
# ------------------------------------------------------------------------------

test:
	@echo "==> Running Cargo tests..."
	cargo test --all-targets

check:
	@echo "==> Running Cargo check..."
	cargo check $(CARGO_TARGET_FLAG)
	@echo "==> Checking code format..."
	cargo fmt --all -- --check
	@echo "==> Running Clippy..."
	cargo clippy --all-targets $(CARGO_TARGET_FLAG) -- -D warnings
	@echo "All quality checks passed!"

fmt:
	@echo "==> Formatting code..."
	cargo fmt --all

clean:
	@echo "==> Cleaning build artifacts..."
	cargo clean
	@rm -rf $(DIST_DIR)
	@echo "Clean completed."

# ------------------------------------------------------------------------------
# Versioning Helper (Rust & Node)
# ------------------------------------------------------------------------------

$(VERSION_TOOL): scripts/version.rs
	@mkdir -p $(DEBUG_DIR)
	@rustc -O scripts/version.rs -o $(VERSION_TOOL)

version:
	@echo $(VERSION)

version-full: $(VERSION_TOOL)
	@$(VERSION_TOOL) full

set-version: $(VERSION_TOOL)
ifndef VERSION
	$(error VERSION is not set. Usage: make set-version VERSION=x.y.z)
endif
	@$(VERSION_TOOL) set $(VERSION)

bump-patch: $(VERSION_TOOL)
	@$(VERSION_TOOL) bump patch

bump-minor: $(VERSION_TOOL)
	@$(VERSION_TOOL) bump minor

bump-major: $(VERSION_TOOL)
	@$(VERSION_TOOL) bump major

tag:
	@echo "==> Creating Git tag v$(VERSION)..."
	git tag -a v$(VERSION) -m "Release v$(VERSION)"
	@echo "Tag v$(VERSION) created. Push with: git push origin v$(VERSION)"
