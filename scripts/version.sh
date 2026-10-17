#!/usr/bin/env bash
# Version bump helper for the geode-fem Rust workspace.
#
# Implements the interface Loom's /loom:release skill expects (see the
# `scripts/version.sh interface` section of .claude/commands/loom/release.md).
# Single source of truth is `[workspace.package].version` in the workspace
# root Cargo.toml; all crates inherit via `version.workspace = true`.
#
# Usage:
#   ./scripts/version.sh                       # print current version
#   ./scripts/version.sh list                  # list version-bearing files
#   ./scripts/version.sh check                 # verify the version parses
#   ./scripts/version.sh bump <level> [--tag]  # patch|minor|major
#   ./scripts/version.sh set <X.Y.Z> [--tag]   # explicit version

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/Cargo.toml"

# The workspace version is the only top-level (column-0) `version = "..."`
# line in Cargo.toml — dependency entries use inline `{ version = "..." }`.
current_version() {
  sed -n 's/^version = "\([^"]*\)"/\1/p' "$MANIFEST" | head -n1
}

bump_semver() {
  local level="$1" cur="$2"
  IFS='.' read -r major minor patch <<<"$cur"
  case "$level" in
    major) printf '%d.0.0\n' $((major + 1)) ;;
    minor) printf '%d.%d.0\n' "$major" $((minor + 1)) ;;
    patch) printf '%d.%d.%d\n' "$major" "$minor" $((patch + 1)) ;;
    *) echo "version.sh: unknown bump level: $level (expected patch|minor|major)" >&2; exit 2 ;;
  esac
}

set_version_in_manifest() {
  local new="$1"
  # Portable across GNU and BSD/macOS sed: write a backup, then drop it.
  sed -i.bak 's/^version = "[^"]*"/version = "'"$new"'"/' "$MANIFEST"
  rm -f "$MANIFEST.bak"
}

regenerate_lockfile() {
  (cd "$ROOT" && cargo update --workspace --offline 2>/dev/null) \
    || (cd "$ROOT" && cargo update --workspace)
}

commit_and_maybe_tag() {
  local new="$1" tag_flag="${2:-}"
  (cd "$ROOT"
   git add Cargo.toml Cargo.lock
   git commit -m "chore: bump version to $new"
   if [[ "$tag_flag" == "--tag" ]]; then
     git tag -a "v$new" -m "v$new"
   fi)
}

cmd="${1:-show}"
case "$cmd" in
  ""|show)
    current_version
    ;;
  list)
    echo "Cargo.toml"
    ;;
  check)
    v=$(current_version)
    if [[ -z "$v" ]]; then
      echo "version.sh: could not read [workspace.package].version from $MANIFEST" >&2
      exit 1
    fi
    echo "$v"
    ;;
  bump)
    level="${2:?usage: version.sh bump <patch|minor|major> [--tag]}"
    tag_flag="${3:-}"
    cur=$(current_version)
    new=$(bump_semver "$level" "$cur")
    set_version_in_manifest "$new"
    regenerate_lockfile
    commit_and_maybe_tag "$new" "$tag_flag"
    echo "v$new"
    ;;
  set)
    new="${2:?usage: version.sh set <X.Y.Z> [--tag]}"
    tag_flag="${3:-}"
    set_version_in_manifest "$new"
    regenerate_lockfile
    commit_and_maybe_tag "$new" "$tag_flag"
    echo "v$new"
    ;;
  *)
    echo "usage: version.sh [show|list|check|bump <level> [--tag]|set <X.Y.Z> [--tag]]" >&2
    exit 2
    ;;
esac
