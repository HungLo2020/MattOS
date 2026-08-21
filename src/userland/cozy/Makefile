INSTALL_DIR := $(HOME)/.local/bin
BINARY := cozy

.PHONY: install build

build:
	cargo build --release

install: build
	cp target/release/cozy $(INSTALL_DIR)/$(BINARY)
