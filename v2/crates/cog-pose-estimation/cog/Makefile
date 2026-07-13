# Build, checksum, sign, verify, and publish cog-pose-estimation artifacts.

CRATE              := cog-pose-estimation
VERSION            := $(shell cargo pkgid -p $(CRATE) 2>/dev/null | sed -E 's/.*#([0-9.]+).*/\1/')
TARGET_DIR         := ../../../target
RELEASE_REPOSITORY ?= Gestusition/KdView
RELEASE_TAG        ?= $(CRATE)-v$(VERSION)

# RELEASE_SIGNING_KEY is canonical. Keep the old variable as an input-only
# compatibility alias so existing private automation does not break abruptly.
RELEASE_SIGNING_KEY ?= $(COGNITUM_OWNER_SIGNING_KEY)
export RELEASE_SIGNING_KEY

ARCHES := arm x86_64

.PHONY: build build-arm build-x86_64

build: build-arm build-x86_64

build-arm:
	mkdir -p dist
	cargo build -p $(CRATE) --release --target aarch64-unknown-linux-gnu
	cp $(TARGET_DIR)/aarch64-unknown-linux-gnu/release/$(CRATE) dist/$(CRATE)-arm

build-x86_64:
	mkdir -p dist
	cargo build -p $(CRATE) --release --target x86_64-unknown-linux-gnu
	cp $(TARGET_DIR)/x86_64-unknown-linux-gnu/release/$(CRATE) dist/$(CRATE)-x86_64

.PHONY: sign sign-arm sign-x86_64

sign: sign-arm sign-x86_64

sign-arm: dist/$(CRATE)-arm
	sha256sum $< | cut -d' ' -f1 > $<.sha256
	@rm -f $<.sig
	@if [ -n "$$RELEASE_SIGNING_KEY" ]; then \
		printf '%s' "$$RELEASE_SIGNING_KEY" \
			| openssl pkeyutl -sign -inkey /dev/stdin -rawin -in $<.sha256 \
			| openssl base64 -A > $<.sig; \
		echo "Signed $<.sha256"; \
	else \
		echo "RELEASE_SIGNING_KEY not set; emitted checksum only for arm"; \
	fi

sign-x86_64: dist/$(CRATE)-x86_64
	sha256sum $< | cut -d' ' -f1 > $<.sha256
	@rm -f $<.sig
	@if [ -n "$$RELEASE_SIGNING_KEY" ]; then \
		printf '%s' "$$RELEASE_SIGNING_KEY" \
			| openssl pkeyutl -sign -inkey /dev/stdin -rawin -in $<.sha256 \
			| openssl base64 -A > $<.sig; \
		echo "Signed $<.sha256"; \
	else \
		echo "RELEASE_SIGNING_KEY not set; emitted checksum only for x86_64"; \
	fi

.PHONY: upload upload-arm upload-x86_64

upload: upload-arm upload-x86_64

upload-arm: sign-arm
	@gh release view $(RELEASE_TAG) --repo $(RELEASE_REPOSITORY) >/dev/null 2>&1 \
		|| gh release create $(RELEASE_TAG) --repo $(RELEASE_REPOSITORY) --verify-tag \
			--title "$(CRATE) $(VERSION)" --generate-notes
	gh release upload $(RELEASE_TAG) --repo $(RELEASE_REPOSITORY) --clobber \
		dist/$(CRATE)-arm dist/$(CRATE)-arm.sha256 $$(test ! -f dist/$(CRATE)-arm.sig || echo dist/$(CRATE)-arm.sig)

upload-x86_64: sign-x86_64
	@gh release view $(RELEASE_TAG) --repo $(RELEASE_REPOSITORY) >/dev/null 2>&1 \
		|| gh release create $(RELEASE_TAG) --repo $(RELEASE_REPOSITORY) --verify-tag \
			--title "$(CRATE) $(VERSION)" --generate-notes
	gh release upload $(RELEASE_TAG) --repo $(RELEASE_REPOSITORY) --clobber \
		dist/$(CRATE)-x86_64 dist/$(CRATE)-x86_64.sha256 $$(test ! -f dist/$(CRATE)-x86_64.sig || echo dist/$(CRATE)-x86_64.sig)

.PHONY: manifest release verify clean

manifest:
	@./scripts/render-manifest.sh $(VERSION)

release: build sign manifest
	@echo "Release assets ready for $(RELEASE_REPOSITORY) tag $(RELEASE_TAG)"

verify:
	@for arch in $(ARCHES); do \
		f=dist/$(CRATE)-$$arch; \
		if [ ! -f $$f ]; then echo "  MISSING $$f"; continue; fi; \
		actual=$$(sha256sum $$f | cut -d' ' -f1); \
		expected=$$(cat $$f.sha256 2>/dev/null); \
		if [ "$$actual" = "$$expected" ]; then echo "  OK     $$f ($$actual)"; \
		else echo "  FAIL   $$f (expected $$expected, got $$actual)"; exit 1; fi; \
	done

clean:
	rm -rf dist/$(CRATE)-*
