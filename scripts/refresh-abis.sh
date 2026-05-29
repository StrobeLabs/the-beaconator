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

setup_worktree() {
  # setup_worktree <repo> <tag>
  # Echoes the worktree path. Caller appends it to WORKTREES for cleanup.
  local repo="$1"
  local tag="$2"
  local wt
  wt="$(mktemp -d -t "$(basename "$repo")-${tag}-XXXXXX")"
  echo "  Worktree: $repo @ $tag -> $wt" >&2
  git -C "$repo" worktree add --detach "$wt" "$tag" >/dev/null
  # Submodules (lib/solady, lib/v4-core, etc.) aren't materialized by `worktree add`.
  # Rewrite git@github.com URLs to https so we don't need an SSH key configured here,
  # then init submodules in-place so forge can resolve remappings.
  # Pass the whole `key=value` as a single shell token so ShellCheck SC2140
  # doesn't trip on the embedded double-quote sandwich, and so the value
  # propagates verbatim into git's config parser.
  git -C "$wt" \
    -c 'url.https://github.com/.insteadOf=git@github.com:' \
    submodule update --init --recursive --jobs 4 >/dev/null
  echo "$wt"
}

# Write the output of `forge inspect <contract> <what>` to $ABIS_DIR/$out
# atomically: stream into a temp file in the same directory, and only mv to
# the final name once forge succeeds. Prevents a failing forge run from
# leaving a truncated artifact behind (which `set -e` would then propagate
# silently into a half-broken `git diff abis/`). 2026-05-29: CodeRabbit
# flagged the earlier non-atomic pattern.
inspect_to() {
  # inspect_to <worktree> <ContractName> <what> <output_filename> <label>
  # <what>  is forge inspect's positional arg ("abi --json" or "bytecode").
  # <label> is a short tag for the log line so callers don't have to repeat it.
  local wt="$1"
  local contract="$2"
  local what="$3"
  local out="$4"
  local label="$5"
  local tmp
  tmp="$(mktemp "$ABIS_DIR/.${out}.tmp.XXXXXX")"
  if ( cd "$wt" && eval "forge inspect \"$contract\" $what" ) > "$tmp"; then
    mv "$tmp" "$ABIS_DIR/$out"
    echo "  Wrote $ABIS_DIR/$out ($label)"
  else
    rm -f "$tmp"
    echo "  FAILED forge inspect $contract $what" >&2
    return 1
  fi
}

inspect_abi_to() {
  # inspect_abi_to <worktree> <ContractName> <output_filename>
  inspect_to "$1" "$2" "abi --json" "$3" "abi"
}

inspect_bytecode_to() {
  # inspect_bytecode_to <worktree> <ContractName> <output_filename>
  # Writes deploy-time (creation) bytecode — what the-beaconator passes to
  # eth_sendTransaction when raw-deploying a contract from
  # `state.contracts.identity_beacon_bytecode`. Forgetting to refresh this
  # in tandem with the source is exactly how we shipped IdentityBeacons
  # whose constructor pre-dated BindingLib (2026-05-29 incident).
  inspect_to "$1" "$2" "bytecode" "$3" "deploy-time bytecode"
}

echo "Refreshing ABIs from beacons@$BEACONS_TAG and perpcity-contracts@$PERPCITY_TAG..."

# beacons worktree, reused for ABI + bytecode artefacts.
BEACONS_WT="$(setup_worktree "$BEACONS_REPO" "$BEACONS_TAG")"
WORKTREES+=("$BEACONS_REPO::$BEACONS_WT")
inspect_abi_to "$BEACONS_WT" BeaconRegistry BeaconRegistry.json
# IdentityBeacon is the only contract we deploy via raw bytecode (the
# beaconator's create_identity_beacon does the legwork). Regenerate the
# deploy-time bytecode every refresh so a beacons-side change (e.g. the
# BindingLib add) propagates the next time the beaconator is built.
inspect_bytecode_to "$BEACONS_WT" IdentityBeacon IdentityBeacon.bytecode

# perpcity-contracts worktree, reused for all four contracts.
PERPCITY_WT="$(setup_worktree "$PERPCITY_REPO" "$PERPCITY_TAG")"
WORKTREES+=("$PERPCITY_REPO::$PERPCITY_WT")
inspect_abi_to "$PERPCITY_WT" Perp Perp.json
inspect_abi_to "$PERPCITY_WT" PerpFactory PerpFactory.json
inspect_abi_to "$PERPCITY_WT" ProtocolFeeManager ProtocolFeeManager.json
inspect_abi_to "$PERPCITY_WT" ModuleRegistry ModuleRegistry.json

echo "Done. Run 'git diff abis/' to review changes."
