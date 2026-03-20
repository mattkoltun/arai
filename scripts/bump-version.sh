#!/usr/bin/env bash
set -euo pipefail

# Bump the version in Cargo.toml, commit, and tag.
#
# Usage:
#   ./scripts/bump-version.sh patch   # 0.1.0 -> 0.1.1
#   ./scripts/bump-version.sh minor   # 0.1.0 -> 0.2.0
#   ./scripts/bump-version.sh major   # 0.1.0 -> 1.0.0

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CARGO_TOML="$PROJECT_ROOT/Cargo.toml"

current=$(grep '^version' "$CARGO_TOML" | head -1 | sed 's/.*"\(.*\)"/\1/')
IFS='.' read -r major minor patch <<< "$current"

bump="${1:-}"
case "$bump" in
    patch) patch=$((patch + 1)) ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    major) major=$((major + 1)); minor=0; patch=0 ;;
    *)
        echo "Usage: $0 [patch|minor|major]"
        echo "Current version: $current"
        exit 1
        ;;
esac

new_version="$major.$minor.$patch"
echo "$current -> $new_version"

# Update Cargo.toml
sed -i '' "s/^version = \"$current\"/version = \"$new_version\"/" "$CARGO_TOML"

# Update Cargo.lock
cargo check --quiet 2>/dev/null

# Commit and tag
git add "$CARGO_TOML" "$PROJECT_ROOT/Cargo.lock"
git commit -m "chore: bump version to v$new_version"
git tag -a "v$new_version" -m "v$new_version"

echo ""
echo "Version bumped to v$new_version"
echo "Run 'git push && git push --tags' to publish"
