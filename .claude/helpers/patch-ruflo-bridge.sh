#!/usr/bin/env bash
# Auto-patch for RuFlo V3 memory bridge NPE
# Called by SessionStart hook to ensure bridge patch is always applied
# Idempotent and silent — suitable for hook execution

NPX_CACHE_DIR=$(find ~/.npm/_npx -name "memory-initializer.js" -path "*/memory/*" 2>/dev/null | head -1)

[[ -z "$NPX_CACHE_DIR" ]] && exit 0

# Already patched?
grep -q 'bridgeResult && bridgeResult.success' "$NPX_CACHE_DIR" 2>/dev/null && exit 0

# Apply patch silently
sed -i '' 's/        if (bridgeResult)$/        if (bridgeResult \&\& bridgeResult.success)/' "$NPX_CACHE_DIR" 2>/dev/null

exit 0
