# HA-Matter Service Packaging

Build, checksum, sign, and release packaging for `cog-ha-matter`.

See [ADR-100 — Cog Packaging Specification](../../../../docs/adr/ADR-100-cog-packaging-specification.md)
and [ADR-116 — HA-Matter Cog](../../../../docs/adr/ADR-116-cog-ha-matter-seed.md)
for the related architecture decisions.

## What is packaged

The binary retains the complete sensing integration:

- mDNS auto-discovery (`_ruview-ha._tcp`)
- Home Assistant and Matter sensing integration
- Ed25519-signed witness chain for tamper-evident audit logs
- Privacy mode that emits semantic primitives instead of biometrics

## Files

| File | Purpose |
|---|---|
| `manifest.template.json` | Runtime manifest template with version, architecture, checksum, and signature slots |
| `Makefile` | Cross-target build, signing, manifest, verification, release, and cleanup targets |
| `dist/` | Generated binaries and release metadata (gitignored) |

## Local packaging

Run commands from this directory:

```sh
# Build one target, or use `make build` for both.
make build-x86_64
make build-arm

# Generate checksums and optional signatures.
make sign-x86_64
make sign-arm

# Verify generated checksums and render the manifest.
make verify
make manifest
```

The ARM build requires the configured `aarch64-unknown-linux-gnu` Rust target
and cross-linker. `make release` runs `build`, `sign`, and `manifest`; use
`make clean` to remove generated artifacts.

### Release signing key

Set `RELEASE_SIGNING_KEY` to a PEM-encoded private key in the environment when
detached signatures are required. The Makefile passes the key to OpenSSL over
standard input and writes a base64-encoded `.sig` beside the binary and its
`.sha256` file. When the variable is unset, packaging still produces and
verifies the SHA-256 checksum, but no signature file is created.

Never commit a private key or include it in command-line arguments.

## GitHub Actions artifacts and releases

The `Cog HA-Matter Release` workflow validates the crate and builds both Linux
architectures. Manual workflow runs upload build artifacts for 14 days but do
not publish a release.

Publishing is tag-gated. Pushing a tag matching
`cog-ha-matter-v<VERSION>` builds the same artifacts and creates or updates the
matching GitHub Release. Keep `<VERSION>` aligned with the crate's workspace
version. Each architecture contributes its binary, `.sha256`, and optional
`.sig` file.

Release assets are referenced from URLs such as:

```text
https://github.com/Gestusition/KdView/releases/download/cog-ha-matter-v<VERSION>/cog-ha-matter-<ARCH>
```
