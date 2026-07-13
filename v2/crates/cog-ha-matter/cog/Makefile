# Build, checksum, sign, and verify pipeline for cog-ha-matter.
# GitHub Actions publishes tagged artifacts to this repository's Releases page.

CRATE      := cog-ha-matter
VERSION    := $(shell cargo pkgid -p $(CRATE) 2>/dev/null | sed -E 's/.*#([0-9.]+).*/\1/')
TARGET_DIR := ../../../target

ARCHES := arm x86_64

# --- Build targets ---

.PHONY: build build-arm build-x86_64

build: build-arm build-x86_64

build-arm:
	mkdir -p dist
	cargo build -p $(CRATE) --release --target aarch64-unknown-linux-gnu
	cp $(TARGET_DIR)/aarch64-unknown-linux-gnu/release/$(CRATE) ./dist/$(CRATE)-arm

build-x86_64:
	mkdir -p dist
	cargo build -p $(CRATE) --release --target x86_64-unknown-linux-gnu
	cp $(TARGET_DIR)/x86_64-unknown-linux-gnu/release/$(CRATE) ./dist/$(CRATE)-x86_64

# --- Checksums and optional Ed25519 signatures ---

.PHONY: sign sign-arm sign-x86_64

sign: sign-arm sign-x86_64

sign-arm: dist/$(CRATE)-arm
	sha256sum dist/$(CRATE)-arm | cut -d' ' -f1 > dist/$(CRATE)-arm.sha256
	@rm -f dist/$(CRATE)-arm.sig
	@if [ -n "$$RELEASE_SIGNING_KEY" ]; then \
		printf '%s' "$$RELEASE_SIGNING_KEY" \
			| openssl pkeyutl -sign -inkey /dev/stdin -rawin \
				-in dist/$(CRATE)-arm.sha256 \
			| openssl base64 -A > dist/$(CRATE)-arm.sig; \
		echo "Signed dist/$(CRATE)-arm.sha256"; \
	else \
		echo "RELEASE_SIGNING_KEY not set; emitted checksum only for arm"; \
	fi

sign-x86_64: dist/$(CRATE)-x86_64
	sha256sum dist/$(CRATE)-x86_64 | cut -d' ' -f1 > dist/$(CRATE)-x86_64.sha256
	@rm -f dist/$(CRATE)-x86_64.sig
	@if [ -n "$$RELEASE_SIGNING_KEY" ]; then \
		printf '%s' "$$RELEASE_SIGNING_KEY" \
			| openssl pkeyutl -sign -inkey /dev/stdin -rawin \
				-in dist/$(CRATE)-x86_64.sha256 \
			| openssl base64 -A > dist/$(CRATE)-x86_64.sig; \
		echo "Signed dist/$(CRATE)-x86_64.sha256"; \
	else \
		echo "RELEASE_SIGNING_KEY not set; emitted checksum only for x86_64"; \
	fi

# --- Manifest ---

.PHONY: manifest

manifest:
	@cargo run -p $(CRATE) --release -- --print-manifest

# --- Convenience ---

.PHONY: release verify clean

release: build sign manifest
	@echo "Release assets ready for $(CRATE) v$(VERSION)"

verify:
	@for arch in $(ARCHES); do \
		f=dist/$(CRATE)-$$arch; \
		if [ ! -f $$f ]; then echo "  MISSING $$f"; continue; fi; \
		actual=$$(sha256sum $$f | cut -d' ' -f1); \
		expected=$$(cat $$f.sha256 2>/dev/null); \
		if [ "$$actual" = "$$expected" ]; then echo "  OK     $$f ($$actual)"; \
		else echo "  FAIL   $$f (expected $$expected, got $$actual)"; fi; \
	done

clean:
	rm -rf dist/$(CRATE)-*
