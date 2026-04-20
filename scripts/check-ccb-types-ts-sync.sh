#!/usr/bin/env bash
# check-ccb-types-ts-sync.sh — verify that every CHUNK_TYPE_* variant defined
# in proto/eaasp/runtime/v2/common.proto has a matching identifier in
# lang/ccb-runtime-ts/src/proto/types.ts AND the wire int values agree.
#
# ccb-runtime-ts uses @grpc/proto-loader (dynamic JSON messages) so most
# types never drift. The ChunkType enum is the one hand-written mirror in
# types.ts, so adding a proto variant — or changing a wire int — MUST be
# echoed there manually. This script is the CI gate that catches silent
# drift (D149, Option B).
#
# Exit 0: every proto variant has a matching TS identifier AND the wire
#         ints match 1:1.
# Exit 1: at least one missing variant, one wire-int mismatch, or a
#         required file is absent.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROTO_FILE="${REPO_ROOT}/proto/eaasp/runtime/v2/common.proto"
TS_FILE="${REPO_ROOT}/lang/ccb-runtime-ts/src/proto/types.ts"

for f in "$PROTO_FILE" "$TS_FILE"; do
  if [ ! -f "$f" ]; then
    echo "error: required file not found: $f" >&2
    exit 1
  fi
done

# Extract the `enum ChunkType { ... }` block from the proto, then pull the
# CHUNK_TYPE_* identifier AND its int value on each non-comment line.
# Bounded to a single block so other future enums can coexist without
# collateral matches. The closing-brace regex allows leading whitespace so
# a future reformat that indents '}' does not leave in_enum stuck at 1.
# Output format (tab-separated, one pair per line):  CHUNK_TYPE_NAME\tINT
proto_pairs=$(
  awk '
    /^enum ChunkType[[:space:]]*\{/   { in_enum = 1; next }
    in_enum && /^[[:space:]]*\}/      { in_enum = 0; next }
    in_enum {
      line = $0
      sub(/\/\/.*/, "", line)          # strip line comments
      if (match(line, /CHUNK_TYPE_[A-Z0-9_]+[[:space:]]*=[[:space:]]*[0-9]+/)) {
        pair = substr(line, RSTART, RLENGTH)
        # Split on "=" and trim whitespace on both sides
        eq = index(pair, "=")
        name = substr(pair, 1, eq - 1)
        val  = substr(pair, eq + 1)
        gsub(/[[:space:]]/, "", name)
        gsub(/[[:space:]]/, "", val)
        printf "%s\t%s\n", name, val
      }
    }
  ' "$PROTO_FILE"
)

if [ -z "$proto_pairs" ]; then
  echo "error: no CHUNK_TYPE_* variants parsed from $PROTO_FILE (enum block malformed?)" >&2
  exit 1
fi

# Extract the `export enum ChunkType { ... }` block from types.ts. The
# closing-brace regex also tolerates leading whitespace to survive future
# Prettier/clang-format reflows.
ts_block=$(
  awk '
    /^export enum ChunkType[[:space:]]*\{/ { in_enum = 1; next }
    in_enum && /^[[:space:]]*\}/           { in_enum = 0; next }
    in_enum { print }
  ' "$TS_FILE"
)

if [ -z "$ts_block" ]; then
  echo "error: could not locate 'export enum ChunkType { ... }' block in $TS_FILE" >&2
  exit 1
fi

missing=()
mismatched=()
count=0
while IFS=$'\t' read -r proto_name proto_int; do
  [ -z "$proto_name" ] && continue
  count=$((count + 1))
  ts_name="${proto_name#CHUNK_TYPE_}"
  # Match `  <NAME> = <INT>,?` inside the enum block (leading spaces, then
  # name, `=`, digits). Use printf for locale-safe piping into grep.
  ts_line=$(printf '%s\n' "$ts_block" | grep -E "^[[:space:]]*${ts_name}[[:space:]]*=[[:space:]]*[0-9]+" || true)
  if [ -z "$ts_line" ]; then
    missing+=("$proto_name -> (expected TS: ${ts_name})")
    continue
  fi
  # Extract the integer on the TS side (first run of digits after `=`).
  ts_int=$(printf '%s\n' "$ts_line" | sed -E 's/^[[:space:]]*[A-Za-z0-9_]+[[:space:]]*=[[:space:]]*([0-9]+).*/\1/')
  if [ "$ts_int" != "$proto_int" ]; then
    mismatched+=("${proto_name} = ${proto_int} in proto but ${ts_name} = ${ts_int} in types.ts")
  fi
done <<< "$proto_pairs"

if [ ${#missing[@]} -gt 0 ] || [ ${#mismatched[@]} -gt 0 ]; then
  echo "✗ ccb-runtime-ts types.ts is out of sync with proto ChunkType (D149):" >&2
  if [ ${#missing[@]} -gt 0 ]; then
    for m in "${missing[@]}"; do
      echo "  - missing $m" >&2
    done
  fi
  if [ ${#mismatched[@]} -gt 0 ]; then
    for m in "${mismatched[@]}"; do
      echo "  - wire-int mismatch: $m" >&2
    done
  fi
  echo "" >&2
  echo "Fix: in ${TS_FILE#${REPO_ROOT}/} ..." >&2
  echo "     - for missing variants: add the enum member (strip the CHUNK_TYPE_ prefix)." >&2
  echo "     - for wire-int mismatches: set the TS int equal to the proto number." >&2
  echo "     Both names and wire ints MUST match the proto enum 1:1." >&2
  exit 1
fi

echo "OK: ${count} ChunkType variants in sync (proto ↔ ccb-runtime-ts/types.ts; names + wire ints verified)"
