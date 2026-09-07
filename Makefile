# qreview. `make check` is the gate; everything else is a shortcut.
#
# The interface is always built before the binary: the binary embeds web/dist.

CARGO ?= cargo
NPM ?= npm
PREFIX ?= $(HOME)/.local
MUSL_TARGET := x86_64-unknown-linux-musl
# What the release file is called. No version in it: the release page and
# the package registry both carry that, and a name that never changes is
# what a permanent link can point at.
DIST_NAME := qreview-linux-x86_64.gz
DEV_PORT := 7420
# What the recording of `make demo` is turned into. An animated WebP: a
# README plays it like an image, where a video file needs a player the page
# may not have, and it weighs a quarter of the same seconds in a GIF.
DEMO_FPS := 10
DEMO_WIDTH := 900
DEMO_QUALITY := 60

.PHONY: all setup web build check test e2e shots demo fmt lint install dist musl-target dev clean

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

## The browser tests alone. They drive the browser already on the machine.
e2e: build
	cd web && $(NPM) run e2e

## Screenshots of the interface, into web/e2e/.shots.
shots: build
	cd web && $(NPM) run shots

## The recording the README shows. `make demo REPO=~/dev/myproject`.
##
## It drives a tour of a real repository and writes docs/demo.gif. Add
## arguments for qreview with DEMO_ARGS, for example DEMO_ARGS=--base=HEAD~5.
##
## It needs two things the gate does not: `npx playwright install ffmpeg`,
## once, for the browser to write a video, and an ffmpeg on the PATH that can
## write WebP.
demo: build
	@test -n "$(REPO)" || { echo "usage: make demo REPO=<a repository>"; exit 1; }
	@ffmpeg -hide_banner -encoders 2>/dev/null | grep -q libwebp_anim || { \
		echo "ffmpeg with the libwebp encoder is missing. It writes docs/demo.webp."; \
		exit 1; \
	}
	cd web && $(NPM) run demo -- $(REPO) $(DEMO_ARGS)
	@mkdir -p docs
	@ffmpeg -v error -y -i web/e2e/.demo/demo.webm \
		-vf "fps=$(DEMO_FPS),scale=$(DEMO_WIDTH):-1:flags=lanczos" \
		-c:v libwebp_anim -lossless 0 -q:v $(DEMO_QUALITY) -compression_level 6 \
		-loop 0 -an docs/demo.webp
	@ls -lh docs/demo.webp

## Correct the format of everything.
fmt:
	$(CARGO) fmt --all
	cd web && $(NPM) run format

lint:
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	cd web && $(NPM) run lint

## The binary into $(PREFIX)/bin.
install: build
	$(CARGO) install --path crates/qreview --root $(PREFIX) --force

## One static binary, compressed for a release. It adds the target if it is
## missing.
dist: web musl-target
	$(CARGO) build --release --target $(MUSL_TARGET)
	@rm -rf dist && mkdir dist
	@cp target/$(MUSL_TARGET)/release/qreview dist/
	@gzip -9 -c dist/qreview > dist/$(DIST_NAME)
	@ls -lh dist/

# A target belongs to rustup, not to asdf: `.tool-versions` pins a Rust
# version and knows nothing about targets. asdf installs Rust through rustup
# and points RUSTUP_HOME at its own directory, so this works under asdf too,
# and the target lands inside the asdf install.
#
# Only `dist` does this. `build` never touches the toolchain.
musl-target:
	@command -v rustup >/dev/null 2>&1 || { \
		echo "rustup is missing. It installs the $(MUSL_TARGET) target."; \
		exit 1; \
	}
	@rustup target list --installed | grep -qx '$(MUSL_TARGET)' || { \
		echo "adding the $(MUSL_TARGET) target"; \
		rustup target add $(MUSL_TARGET); \
	}

## The server and Vite together, with hot reload on the interface.
## Open the URL that Vite prints, not the one the server prints.
dev:
	@trap 'kill 0' EXIT INT TERM; \
	$(CARGO) run -- --port $(DEV_PORT) & \
	cd web && $(NPM) run dev; \
	wait

clean:
	rm -rf dist
	$(CARGO) clean
	rm -rf web/dist web/node_modules
