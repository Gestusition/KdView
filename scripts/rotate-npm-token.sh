#!/usr/bin/env bash
#
# rotate-npm-token.sh — push NPM_TOKEN from .env into a configured GCP Secret
# Manager project and optionally publish the selected package.
#
# Usage:
#   bash scripts/rotate-npm-token.sh             # rotate only
#   bash scripts/rotate-npm-token.sh --publish   # rotate + npm publish
#
# Env overrides:
#   GCP_PROJECT          (default: active gcloud project)
#   NPM_TOKEN_SECRET     (default: NPM_TOKEN)
#   ENV_FILE             (default: <repo-root>/.env)
#   PUBLISH_PACKAGE_DIR  (default: <repo-root>/tools/ruview-mcp)
#   NPM_REGISTRY         (default: https://registry.npmjs.org/)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${ENV_FILE:-$REPO_ROOT/.env}"
PROJECT="${GCP_PROJECT:-$(gcloud config get-value project 2>/dev/null || true)}"
: "${PROJECT:?Set GCP_PROJECT or configure a default gcloud project}"
SECRET="${NPM_TOKEN_SECRET:-NPM_TOKEN}"
PKG_DIR="${PUBLISH_PACKAGE_DIR:-$REPO_ROOT/tools/ruview-mcp}"
REGISTRY="${NPM_REGISTRY:-https://registry.npmjs.org/}"

[ -f "$ENV_FILE" ] || { echo "ERROR: .env not found at $ENV_FILE" >&2; exit 1; }

TOKEN="$(awk -F= '
  /^[[:space:]]*NPM_TOKEN[[:space:]]*=/ {
    sub(/^[^=]*=[[:space:]]*/, "", $0)
    sub(/^["'\'']/, "", $0)
    sub(/["'\''][[:space:]]*$/, "", $0)
    sub(/[[:space:]]+$/, "", $0)
    print
    exit
  }
' "$ENV_FILE")"

if [ -z "${TOKEN:-}" ]; then
  echo "ERROR: NPM_TOKEN not found in $ENV_FILE" >&2
  exit 1
fi

LEN=${#TOKEN}
echo "Found NPM_TOKEN in .env (length=$LEN)"

echo "Pushing new version to gcloud secret '$SECRET' in project '$PROJECT'..."
if ! gcloud secrets describe "$SECRET" --project="$PROJECT" >/dev/null 2>&1; then
  echo "Secret '$SECRET' not found; creating..."
  printf '%s' "$TOKEN" | gcloud secrets create "$SECRET" \
    --project="$PROJECT" --replication-policy=automatic --data-file=-
else
  printf '%s' "$TOKEN" | gcloud secrets versions add "$SECRET" \
    --project="$PROJECT" --data-file=-
fi

echo "Verifying secret round-trips..."
RETRIEVED="$(gcloud secrets versions access latest --secret="$SECRET" --project="$PROJECT")"
if [ "$RETRIEVED" != "$TOKEN" ]; then
  echo "ERROR: retrieved token does not match the value written to .env" >&2
  exit 1
fi
echo "OK — secret '$SECRET' updated and verified (length=${#RETRIEVED})."

if [ "${1:-}" = "--publish" ]; then
  [ -d "$PKG_DIR" ] || { echo "ERROR: package dir not found at $PKG_DIR" >&2; exit 1; }
  PACKAGE_NAME="$(node -p "require('$PKG_DIR/package.json').name")"
  echo "Publishing $PACKAGE_NAME from $PKG_DIR to $REGISTRY..."
  (
    cd "$PKG_DIR"
    if [ -f package.json ] && grep -q '"build"' package.json; then
      npm run build
    fi
    NODE_AUTH_TOKEN="$RETRIEVED" npm publish --access public --registry "$REGISTRY"
  )
fi

echo "Done."
