# cog-ha-matter Release Checklist

## 1. Validate locally

```sh
cargo test -p cog-ha-matter --no-default-features --lib
cargo check -p cog-ha-matter --no-default-features
```

## 2. Tag the release

```sh
version=$(cargo pkgid -p cog-ha-matter | sed -E 's/.*#//')
git tag "cog-ha-matter-v$version"
git push origin "cog-ha-matter-v$version"
```

The tag starts `.github/workflows/cog-ha-matter-release.yml`, which:

- reruns the library tests and package check;
- builds `cog-ha-matter-x86_64` and `cog-ha-matter-arm`;
- emits SHA-256 sidecars;
- optionally emits Ed25519 signatures when the repository's
  `RELEASE_SIGNING_KEY` secret is configured;
- retains per-architecture workflow artifacts for 14 days; and
- creates or updates the matching GitHub Release with every generated asset.

A manual workflow dispatch validates and builds the same artifacts but does not
publish a GitHub Release because it is not associated with a version tag.

## 3. Verify the published release

```sh
tag="cog-ha-matter-v$(cargo pkgid -p cog-ha-matter | sed -E 's/.*#//')"
gh release view "$tag"

tmp=$(mktemp -d)
gh release download "$tag" --dir "$tmp"
(
  cd "$tmp"
  for checksum in *.sha256; do
    binary="${checksum%.sha256}"
    printf '%s  %s\n' "$(cat "$checksum")" "$binary" | sha256sum --check -
  done
)
```

## Replacing a bad artifact

Fix the build at the same commit, delete the failed workflow run's release
assets if necessary, and rerun the tag workflow. The publish job uploads assets
with `--clobber`, so a deterministic rebuild replaces the matching filenames.
If code changed, publish a new patch version instead of mutating an existing
release.
