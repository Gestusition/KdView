# PyPI release runbook — `wifi-densepose` + `ruview`

Operations doc for the `.github/workflows/pip-release.yml` CI workflow.

## Auth

The workflow uses one GitHub Actions secret named `PYPI_API_TOKEN`.
The published `wifi-densepose` and `ruview` names are stable compatibility
identifiers. A maintainer must use a PyPI project token authorized for both.

## Refreshing the token

Store the token in a maintainer-controlled secret manager. Set or rotate the
GitHub Actions secret only in the canonical fork:

```bash
gh secret set PYPI_API_TOKEN --repo Gestusition/KdView
```

Paste the token through the interactive prompt. Do not pipe it through logs,
shell history, repository files, or CI output.

## Triggering a release

Two paths:

- **Tag push** — `git tag v2.X.Y-pip && git push origin v2.X.Y-pip` —
  publishes the v2 wheel matrix. `v1.99.0-pip` triggers the tombstone
  job instead.
- **Manual dispatch** — `gh workflow run pip-release.yml --ref <branch>
  -f target=v2-wheels -f publish_to=pypi`. Use `publish_to=testpypi`
  for a dry-run target if a TestPyPI token is also set as
  `TESTPYPI_API_TOKEN`.

## Release-day sequence

Per ADR-117 §7.3, the tombstone publishes first so it claims the
"current" slot in pip's resolver:

1. `git tag v1.99.0-pip && git push origin v1.99.0-pip` →
   tombstone live at `https://pypi.org/project/wifi-densepose/1.99.0/`
2. Verify: `pip install wifi-densepose==1.99.0; python -c "import
   wifi_densepose"` → ImportError with migration URL.
3. `git tag v2.0.0-pip && git push origin v2.0.0-pip` → v2 wheel
   matrix live at `https://pypi.org/project/wifi-densepose/2.0.0/`.
4. (Optional, in lock-step) build + publish a matching `ruview`
   release from `python/ruview-meta/` so the meta-package version
   stays pinned to the same wifi-densepose version.

## Off-loop manual gates

- **Q3** (ADR-117 §11.3) — generate `expected_features_v2.sha256`
  from the v2 Rust pipeline before any v2 publish.
- **OIDC Trusted Publisher** — not used. The workflow is token-based.
  If the project migrates to OIDC later, remove `password:`
  from `pypa/gh-action-pypi-publish` calls and add the publisher
  registration on pypi.org.
