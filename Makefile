# ==============================================================================
# Window Hop - Makefile
# ==============================================================================
# Commands for building development executables, release packages, installer .exe,
# running quality checks, and managing project versioning.
# ==============================================================================

SHELL := /bin/bash
PYTHON := python3

# Extract current project version
VERSION ?= $(shell $(PYTHON) scripts/version.py get 2>/dev/null || sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)

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
ZIP_NAME := window-hop-v$(VERSION)-windows-x64.zip

.PHONY: all help dev build-dev release build-release installer package package-dev \
        test check fmt clean version version-full set-version bump-patch bump-minor bump-major tag

all: help

help:
	@echo "Window Hop Build and Versioning Helper"
	@echo "Current version: $(VERSION)"
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
	@echo "Versioning targets:"
	@echo "  make version         Print current version"
	@echo "  make version-full    Print current version with git commit hash"
	@echo "  make set-version VERSION=x.y.z  Update version in Cargo.toml and Cargo.lock"
	@echo "  make bump-patch      Increment patch version (e.g. 0.1.0 -> 0.1.1)"
	@echo "  make bump-minor      Increment minor version (e.g. 0.1.0 -> 0.2.0)"
	@echo "  make bump-major      Increment major version (e.g. 0.1.0 -> 1.0.0)"
	@echo "  make tag             Create git annotated tag for current version (v$(VERSION))"
	@echo ""
	@echo "Options:"
	@echo "  TARGET=<triple>      Optional cross-compilation target (e.g. x86_64-pc-windows-msvc)"

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
	@echo "==> Building release executables (profile: release)..."
	cargo build --release $(CARGO_TARGET_FLAG) --bin window-hop --bin window-hop-installer
	@echo "Release binaries built at $(RELEASE_DIR)/"

installer:
	@echo "==> Building release installer executable..."
	cargo build --release $(CARGO_TARGET_FLAG) --bin window-hop-installer
	@echo "Release installer built at $(RELEASE_DIR)/window-hop-installer$(EXE_EXT)"

# ------------------------------------------------------------------------------
# Packaging
# ------------------------------------------------------------------------------

package: release
	@echo "==> Packaging release artifacts into $(DIST_DIR)/..."
	@mkdir -p $(DIST_DIR)
	@cp $(RELEASE_DIR)/window-hop$(EXE_EXT) $(DIST_DIR)/
	@cp $(RELEASE_DIR)/window-hop-installer$(EXE_EXT) $(DIST_DIR)/
	@cp docs/install-guide.md $(DIST_DIR)/INSTALL-GUIDE.txt
	@echo "==> Creating distribution archive $(DIST_DIR)/$(ZIP_NAME)..."
	@cd $(DIST_DIR) && \
	if command -v zip >/dev/null 2>&1; then \
		zip -q -r $(ZIP_NAME) window-hop$(EXE_EXT) window-hop-installer$(EXE_EXT) INSTALL-GUIDE.txt; \
	elif command -v powershell.exe >/dev/null 2>&1; then \
		powershell.exe -NoProfile -Command "Compress-Archive -Path window-hop$(EXE_EXT),window-hop-installer$(EXE_EXT),INSTALL-GUIDE.txt -DestinationPath $(ZIP_NAME) -Force"; \
	elif command -v pwsh >/dev/null 2>&1; then \
		pwsh -NoProfile -Command "Compress-Archive -Path window-hop$(EXE_EXT),window-hop-installer$(EXE_EXT),INSTALL-GUIDE.txt -DestinationPath $(ZIP_NAME) -Force"; \
	elif command -v 7z >/dev/null 2>&1; then \
		7z a -bd -bso0 $(ZIP_NAME) window-hop$(EXE_EXT) window-hop-installer$(EXE_EXT) INSTALL-GUIDE.txt; \
	else \
		tar -czf window-hop-v$(VERSION)-windows-x64.tar.gz window-hop$(EXE_EXT) window-hop-installer$(EXE_EXT) INSTALL-GUIDE.txt; \
	fi
	@echo "==> Package ready:"
	@ls -lh $(DIST_DIR)

package-dev: build-dev
	@echo "==> Packaging development artifacts into $(DIST_DIR)/..."
	@mkdir -p $(DIST_DIR)
	@cp $(DEBUG_DIR)/window-hop$(EXE_EXT) $(DIST_DIR)/
	@cp $(DEBUG_DIR)/window-hop-installer$(EXE_EXT) $(DIST_DIR)/
	@cp docs/install-guide.md $(DIST_DIR)/INSTALL-GUIDE.txt
	@echo "==> Creating development archive $(DIST_DIR)/$(ZIP_NAME)..."
	@cd $(DIST_DIR) && \
	if command -v zip >/dev/null 2>&1; then \
		zip -q -r $(ZIP_NAME) window-hop$(EXE_EXT) window-hop-installer$(EXE_EXT) INSTALL-GUIDE.txt; \
	elif command -v powershell.exe >/dev/null 2>&1; then \
		powershell.exe -NoProfile -Command "Compress-Archive -Path window-hop$(EXE_EXT),window-hop-installer$(EXE_EXT),INSTALL-GUIDE.txt -DestinationPath $(ZIP_NAME) -Force"; \
	elif command -v pwsh >/dev/null 2>&1; then \
		pwsh -NoProfile -Command "Compress-Archive -Path window-hop$(EXE_EXT),window-hop-installer$(EXE_EXT),INSTALL-GUIDE.txt -DestinationPath $(ZIP_NAME) -Force"; \
	elif command -v 7z >/dev/null 2>&1; then \
		7z a -bd -bso0 $(ZIP_NAME) window-hop$(EXE_EXT) window-hop-installer$(EXE_EXT) INSTALL-GUIDE.txt; \
	else \
		tar -czf window-hop-v$(VERSION)-windows-x64.tar.gz window-hop$(EXE_EXT) window-hop-installer$(EXE_EXT) INSTALL-GUIDE.txt; \
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
	@$(PYTHON) -c "import shutil, os; os.path.exists('$(DIST_DIR)') and shutil.rmtree('$(DIST_DIR)')"
	@echo "Clean completed."

# ------------------------------------------------------------------------------
# Versioning
# ------------------------------------------------------------------------------

version:
	@$(PYTHON) scripts/version.py get

version-full:
	@$(PYTHON) scripts/version.py full

set-version:
ifndef VERSION
	$(error VERSION is not set. Usage: make set-version VERSION=x.y.z)
endif
	@$(PYTHON) scripts/version.py set $(VERSION)

bump-patch:
	@$(PYTHON) scripts/version.py bump patch

bump-minor:
	@$(PYTHON) scripts/version.py bump minor

bump-major:
	@$(PYTHON) scripts/version.py bump major

tag:
	@echo "==> Creating Git tag v$(VERSION)..."
	git tag -a v$(VERSION) -m "Release v$(VERSION)"
	@echo "Tag v$(VERSION) created. Push with: git push origin v$(VERSION)"
