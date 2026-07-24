SHELL := /bin/sh

CARGO ?= cargo
PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin
TARGET_DIR ?= $(CURDIR)/codex-rs/target

CLANKER_BIN := $(TARGET_DIR)/release/clanker
MANIFEST := $(CURDIR)/codex-rs/Cargo.toml

.PHONY: all build install

all: install

build:
	$(CARGO) build --locked --release \
		--manifest-path "$(MANIFEST)" \
		--target-dir "$(TARGET_DIR)" \
		--package codex-cli \
		--bin clanker

install: build
	mkdir -p "$(BINDIR)"
	install -m 755 "$(CLANKER_BIN)" "$(BINDIR)/clanker"
	@case ":$$PATH:" in \
		*":$(BINDIR):"*) printf 'Installed clanker to %s\n' "$(BINDIR)/clanker" ;; \
		*) printf 'Installed clanker to %s\nAdd %s to PATH to run it directly.\n' \
			"$(BINDIR)/clanker" "$(BINDIR)" ;; \
	esac
