#!/usr/bin/env bash
# Build signed + notarized macOS DMG.
#
# Prerequisites:
#   1. Developer ID Application cert installed (signingIdentity in tauri.conf.json).
#   2. App-specific password stored via:
#        xcrun notarytool store-credentials "my-translator" \
#          --apple-id "<your Apple ID>" --team-id "<your Team ID>"
#
# Usage:  ./scripts/build-notarized.sh
#
# Reads the app-specific password from your terminal env. If not set, prompts.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Load secrets from .env (gitignored). Format:
#   APPLE_PASSWORD=xxxx-xxxx-xxxx-xxxx
if [[ -f "$REPO_ROOT/.env" ]]; then
  set -a
  # shellcheck disable=SC1091
  source "$REPO_ROOT/.env"
  set +a
fi

for v in APPLE_ID APPLE_TEAM_ID APPLE_PASSWORD APPLE_SIGNING_IDENTITY; do
  if [[ -z "${!v:-}" ]]; then
    echo "ERROR: $v not set." >&2
    echo "Put these in .env (gitignored):" >&2
    echo "  APPLE_ID=you@example.com" >&2
    echo "  APPLE_TEAM_ID=XXXXXXXXXX" >&2
    echo "  APPLE_PASSWORD=xxxx-xxxx-xxxx-xxxx   # app-specific password" >&2
    echo '  APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (XXXXXXXXXX)"' >&2
    exit 1
  fi
done

export APPLE_ID APPLE_TEAM_ID APPLE_PASSWORD APPLE_SIGNING_IDENTITY

echo "Building with notarization..."
echo "  APPLE_ID=$APPLE_ID"
echo "  APPLE_TEAM_ID=$APPLE_TEAM_ID"
echo "  APPLE_SIGNING_IDENTITY=$APPLE_SIGNING_IDENTITY"
echo "  APPLE_PASSWORD=*** (${#APPLE_PASSWORD} chars)"
echo

npm run tauri build
