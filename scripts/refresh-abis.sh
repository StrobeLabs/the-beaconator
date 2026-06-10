#!/usr/bin/env bash
# Regenerate the-beaconator/abis/*.json from pinned contract release tags.
#
# Reads .contracts-versions for the pinned tags, creates a temporary git worktree at
# each tag, runs `forge inspect`, and writes the JSONs back into abis/. Removes
# the worktrees on exit.
#
# Usage: ./scripts/refresh-abis.sh
#        make refresh-abis
#
# Assumes sibling repo layout: ../beacons and ../perpcity-contracts next to the-beaconator.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ABIS_DIR="$REPO_ROOT/abis"
VERSIONS_FILE="$REPO_ROOT/.contracts-versions"

if ! command -v forge >/dev/null 2>&1; then
  echo "error: 'forge' (Foundry) not found in PATH. Install with: curl -L https://foundry.paradigm.xyz | bash" >&2
  exit 1
fi

if [[ ! -f "$VERSIONS_FILE" ]]; then
  echo "error: $VERSIONS_FILE not found" >&2
  exit 1
fi

read_pin() {
  local key="$1"
  awk -F= -v k="$key" '$1 == k { print $2 }' "$VERSIONS_FILE" | head -n1
}

BEACONS_TAG="$(read_pin beacons)"
PERPCITY_TAG="$(read_pin perpcity-contracts)"

if [[ -z "$BEACONS_TAG" || -z "$PERPCITY_TAG" ]]; then
  echo "error: missing pinned tag in .contracts-versions (beacons=$BEACONS_TAG, perpcity-contracts=$PERPCITY_TAG)" >&2
  exit 1
fi

BEACONS_REPO="$REPO_ROOT/../beacons"
PERPCITY_REPO="$REPO_ROOT/../perpcity-contracts"

if [[ ! -d "$BEACONS_REPO/.git" ]]; then
  echo "error: beacons repo not found at $BEACONS_REPO" >&2
  exit 1
fi
if [[ ! -d "$PERPCITY_REPO/.git" ]]; then
  echo "error: perpcity-contracts repo not found at $PERPCITY_REPO" >&2
  exit 1
fi

ensure_tag() {
  local repo="$1"
  local tag="$2"
  if ! git -C "$repo" rev-parse "$tag^{tag}" >/dev/null 2>&1 \
     && ! git -C "$repo" rev-parse "$tag^{commit}" >/dev/null 2>&1; then
    echo "error: tag '$tag' not found in $repo. Try: git -C $repo fetch --tags" >&2
    exit 1
  fi
}

ensure_tag "$BEACONS_REPO" "$BEACONS_TAG"
ensure_tag "$PERPCITY_REPO" "$PERPCITY_TAG"

mkdir -p "$ABIS_DIR"

WORKTREES=()
cleanup() {
  for wt in "${WORKTREES[@]:-}"; do
    if [[ -n "$wt" && -d "$wt" ]]; then
      # First arg is the repo, second is the worktree path.
      local repo="${wt%%::*}"
      local path="${wt##*::}"
      git -C "$repo" worktree remove "$path" --force >/dev/null 2>&1 || true
    fi
  done
}
trap cleanup EXIT

inspect_to() {
  # inspect_to <repo> <tag> <ContractName> <output_filename>
  local repo="$1"
  local tag="$2"
  local contract="$3"
  local out="$4"
  local wt
  wt="$(mktemp -d -t "$(basename "$repo")-${tag}-XXXXXX")"
  WORKTREES+=("$repo::$wt")
  echo "  Worktree: $repo @ $tag -> $wt"
  git -C "$repo" worktree add --detach "$wt" "$tag" >/dev/null
  # Submodules (lib/solady, lib/v4-core, etc.) aren't materialized by `worktree add`.
  # Rewrite git@github.com URLs to https so we don't need an SSH key configured here,
  # then init submodules in-place so forge can resolve remappings.
  git -C "$wt" \
    -c url."https://github.com/".insteadOf="git@github.com:" \
    submodule update --init --recursive --jobs 4 >/dev/null
  ( cd "$wt" && forge inspect "$contract" abi --json ) > "$ABIS_DIR/$out"
  echo "  Wrote $ABIS_DIR/$out"
}

# Same as inspect_to but emits creation BYTECODE instead of the ABI. The
# beaconator deploys IdentityBeacon directly from this snapshot (the verifier
# comes from the on-chain factory), so a stale snapshot silently deploys a
# DIFFERENT contract than the pinned tag: the pre-v0.0.1 IdentityBeacon never
# bound its verifier, leaving every beacon un-updatable against v0.0.1
# CallerBound verifiers (UnauthorizedCaller on every update).
bytecode_to() {
  local repo="$1"
  local tag="$2"
  local contract="$3"
  local out="$4"
  local wt
  wt="$(mktemp -d -t "$(basename "$repo")-${tag}-XXXXXX")"
  WORKTREES+=("$repo::$wt")
  echo "  Worktree: $repo @ $tag -> $wt"
  git -C "$repo" worktree add --detach "$wt" "$tag" >/dev/null
  git -C "$wt" \
    -c url."https://github.com/".insteadOf="git@github.com:" \
    submodule update --init --recursive --jobs 4 >/dev/null
  ( cd "$wt" && forge inspect "$contract" bytecode ) > "$ABIS_DIR/$out"
  echo "  Wrote $ABIS_DIR/$out"
}

echo "Refreshing ABIs from beacons@$BEACONS_TAG and perpcity-contracts@$PERPCITY_TAG..."
inspect_to "$BEACONS_REPO" "$BEACONS_TAG" BeaconRegistry BeaconRegistry.json
bytecode_to "$BEACONS_REPO" "$BEACONS_TAG" IdentityBeacon IdentityBeacon.bytecode
inspect_to "$PERPCITY_REPO" "$PERPCITY_TAG" Perp Perp.json
inspect_to "$PERPCITY_REPO" "$PERPCITY_TAG" PerpFactory PerpFactory.json
inspect_to "$PERPCITY_REPO" "$PERPCITY_TAG" ProtocolFeeManager ProtocolFeeManager.json
inspect_to "$PERPCITY_REPO" "$PERPCITY_TAG" ModuleRegistry ModuleRegistry.json

echo "Done. Run 'git diff abis/' to review changes."
