# qreview. `make check` is the gate; everything else is a shortcut.
#
# The interface is always built before the binary: the binary embeds web/dist.

CARGO ?= cargo
NPM ?= npm
PREFIX ?= $(HOME)/.local
MUSL_TARGET := x86_64-unknown-linux-musl
DEV_PORT := 7420

.PHONY: all setup web build check test fmt lint install dist dev clean

all: build

## Install the interface dependencies. Needed once, and after a lockfile change.
setup:
	cd web && $(NPM) ci

## Build the interface into web/dist.
web:
	cd web && $(NPM) run build

## Build the release binary, interface included.
build: web
	$(CARGO) build --release

## The whole gate. Run this before every commit. `make check V=1` streams it.
check:
	@V=$(V) ./scripts/check.sh

## The tests alone.
test: web
	$(CARGO) test --workspace
	cd web && $(NPM) run test:run

## Correct the format of everything.
fmt:
	$(CARGO) fmt --all
	cd web && $(NPM) run format

lint:
	$(CARGO) clippy --all-targets -- -D warnings
	cd web && $(NPM) run lint

## The binary into $(PREFIX)/bin.
install: build
	$(CARGO) install --path crates/qreview --root $(PREFIX) --force

## One static binary, for a colleague. Needs: rustup target add $(MUSL_TARGET)
dist: web
	$(CARGO) build --release --target $(MUSL_TARGET)
	@ls -lh target/$(MUSL_TARGET)/release/qreview

## The server and Vite together, with hot reload on the interface.
## Open the URL that Vite prints, not the one the server prints.
dev:
	@trap 'kill 0' EXIT INT TERM; \
	$(CARGO) run -- --port $(DEV_PORT) & \
	cd web && $(NPM) run dev; \
	wait

clean:
	$(CARGO) clean
	rm -rf web/dist web/node_modules
