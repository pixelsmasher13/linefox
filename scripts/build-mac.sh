#!/usr/bin/env bash
# Build, sign, notarize, and publish the Linefox (OSS) macOS .dmg.
#
# Adapted from heelix_notes/scripts/build-mac.sh. The OSS build:
#   - Uses bundle id `com.openlinefox.dev` (set in tauri.conf.json), so it
#     lives alongside the closed Linefox build (`com.linefox.dev`) without
#     conflict.
#   - Does NOT publish updater artifacts (`createUpdaterArtifacts: false`),
#     so no Tauri signing key is required.
#   - Uploads the signed .dmg to a GitHub Release instead of S3.
#
# Required env vars (for signing + notarization):
#   APPLE_API_ISSUER         (App Store Connect API issuer ID)
#   APPLE_API_KEY            (App Store Connect API key ID)
#   APPLE_API_KEY_PATH       (absolute path to the AuthKey_*.p8 file)
#   APPLE_SIGNING_IDENTITY   (e.g. "Developer ID Application: Your Co (TEAMID)")
#
# Optional:
#   GH_REPO                  (default: pixelsmasher13/linefox)
#   RELEASE_TAG              (default: v$(version from src-tauri/Cargo.toml))
#   RELEASE_TITLE            (default: "Linefox $RELEASE_TAG")
#   RELEASE_NOTES_FILE       (default: docs/release-notes.md if present, else autogen)
#   PRERELEASE               (set =1 to mark as pre-release)
#
# Usage:
#   ./scripts/build-mac.sh                   # build + create/update GitHub release
#   ./scripts/build-mac.sh --no-upload       # build only (skip GitHub publish)
#   ./scripts/build-mac.sh --tag v0.1.0      # override release tag
#
# Prereqs:
#   - gh CLI authenticated (run `gh auth status` to verify; should show the
#     account that owns the GH_REPO)
#   - Apple Developer credentials in .env.build (gitignored)

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# ── Auto-load credentials from .env.build if present (gitignored) ──
if [[ -f "$ROOT_DIR/.env.build" ]]; then
  echo "==> Loading credentials from .env.build"
  set -a
  # shellcheck disable=SC1090
  source "$ROOT_DIR/.env.build"
  set +a
fi

# ── Parse CLI flags ──
SKIP_UPLOAD=0
CLI_TAG=""
PRERELEASE_FLAG="${PRERELEASE:-0}"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-upload) SKIP_UPLOAD=1; shift ;;
    --tag)       CLI_TAG="$2"; shift 2 ;;
    --prerelease) PRERELEASE_FLAG=1; shift ;;
    *) echo "Unknown arg: $1"; exit 1 ;;
  esac
done

# ── Require signing env vars ──
: "${APPLE_API_ISSUER:?APPLE_API_ISSUER must be set (see .env.build.example)}"
: "${APPLE_API_KEY:?APPLE_API_KEY must be set}"
: "${APPLE_API_KEY_PATH:?APPLE_API_KEY_PATH must be set}"
: "${APPLE_SIGNING_IDENTITY:?APPLE_SIGNING_IDENTITY must be set}"

if [[ ! -f "$APPLE_API_KEY_PATH" ]]; then
  echo "❌ APPLE_API_KEY_PATH does not exist: $APPLE_API_KEY_PATH"
  exit 1
fi

GH_REPO="${GH_REPO:-pixelsmasher13/linefox}"

# ── Verify gh CLI is available + authed (unless skipping upload) ──
if [[ "$SKIP_UPLOAD" -eq 0 ]]; then
  if ! command -v gh >/dev/null 2>&1; then
    echo "❌ gh CLI not found. Install with: brew install gh"
    exit 1
  fi
  if ! gh auth status >/dev/null 2>&1; then
    echo "❌ gh CLI not authenticated. Run: gh auth login"
    exit 1
  fi
fi

# ── Clean stale dmg artifacts from prior runs ──
# Tauri's bundle_dmg.sh calls `hdiutil convert ... -o <target>` without `-ov`,
# so a leftover Linefox_*.dmg aborts bundling before producing a new image.
rm -f src-tauri/target/release/bundle/macos/*.dmg \
      src-tauri/target/release/bundle/macos/.DS_Store \
      src-tauri/target/release/bundle/dmg/*.dmg \
      src-tauri/target/release/bundle/dmg/rw.*.dmg

# ── Build ──
echo "==> Building Linefox (OSS) for macOS (signed + notarized)..."
npm run tauri build

# ── Locate the DMG ──
DMG_PATH=$(find src-tauri/target/release/bundle/dmg -maxdepth 1 -name "*.dmg" | head -1)
if [[ -z "$DMG_PATH" ]]; then
  echo "❌ No .dmg produced under src-tauri/target/release/bundle/dmg"
  exit 1
fi

DMG_FILE=$(basename "$DMG_PATH")
DMG_SIZE=$(du -h "$DMG_PATH" | cut -f1)
echo "==> Built: $DMG_PATH ($DMG_SIZE)"

# ── Verify signature + notarization ──
echo "==> Verifying signature..."
codesign --verify --deep --strict --verbose=2 "$DMG_PATH" 2>&1 | tail -3 || true

# spctl checks Gatekeeper acceptance (= signed AND notarized + stapled). Non-fatal
# on local dev where the ticket might not yet be stapled; just informational.
echo "==> Checking Gatekeeper acceptance..."
spctl --assess --type install --verbose "$DMG_PATH" 2>&1 | tail -2 || true

if [[ "$SKIP_UPLOAD" -eq 1 ]]; then
  echo "==> Skipping GitHub upload (--no-upload)"
  exit 0
fi

# ── Derive release tag ──
# Priority: --tag flag > $RELEASE_TAG env > "v" + Cargo.toml version
if [[ -n "$CLI_TAG" ]]; then
  TAG="$CLI_TAG"
elif [[ -n "${RELEASE_TAG:-}" ]]; then
  TAG="$RELEASE_TAG"
else
  VERSION=$(grep -m1 '^version' src-tauri/Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
  TAG="v$VERSION"
fi
TITLE="${RELEASE_TITLE:-Linefox $TAG}"

echo "==> Publishing to GitHub Releases: $GH_REPO @ $TAG"

# ── Create or update the release ──
RELEASE_ARGS=("--repo" "$GH_REPO" "--title" "$TITLE")
[[ "$PRERELEASE_FLAG" == "1" ]] && RELEASE_ARGS+=("--prerelease")

if [[ -n "${RELEASE_NOTES_FILE:-}" && -f "$RELEASE_NOTES_FILE" ]]; then
  RELEASE_ARGS+=("--notes-file" "$RELEASE_NOTES_FILE")
elif [[ -f "$ROOT_DIR/docs/release-notes.md" ]]; then
  RELEASE_ARGS+=("--notes-file" "$ROOT_DIR/docs/release-notes.md")
else
  # No notes file — let gh autogenerate from commit log
  RELEASE_ARGS+=("--generate-notes")
fi

# If the release already exists, upload assets to it; otherwise create it
if gh release view "$TAG" --repo "$GH_REPO" >/dev/null 2>&1; then
  echo "==> Release $TAG already exists — uploading asset"
  gh release upload "$TAG" "$DMG_PATH" --repo "$GH_REPO" --clobber
else
  echo "==> Creating release $TAG"
  gh release create "$TAG" "$DMG_PATH" "${RELEASE_ARGS[@]}"
fi

echo
echo "✅ Done"
echo "   Release: https://github.com/$GH_REPO/releases/tag/$TAG"
echo "   Download: https://github.com/$GH_REPO/releases/download/$TAG/$DMG_FILE"
