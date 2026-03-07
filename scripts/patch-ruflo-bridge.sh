#!/usr/bin/env bash
# patch-ruflo-bridge.sh — Fix RuFlo/claude-flow embedding model failures
#
# Root cause: @xenova/transformers tries to download from HuggingFace but gets
# "Unauthorized access" error. Local ONNX model exists at .claude-flow/models/
# but the code doesn't configure transformers.js to use it.
#
# This patch fixes 3 issues across ALL installation paths:
# 1. bridgeResult NPE: adds .success check to bare bridgeResult checks
# 2. null state NPE: guards against embeddingModelState being null
# 3. local model: configures @xenova/transformers to use local ONNX model
#
# Idempotent: safe to run multiple times.

set -euo pipefail

python3 << 'PYEOF'
import os, glob, sys

patched_count = 0
skipped_count = 0

def patch_memory_initializer(fpath):
    global patched_count, skipped_count
    if not os.path.exists(fpath):
        return
    with open(fpath, 'r') as f:
        content = f.read()

    original = content

    # Patch 1: bridgeResult NPE checks
    content = content.replace(
        '        if (bridgeResult)\n            return bridgeResult;',
        '        if (bridgeResult && bridgeResult.success)\n            return bridgeResult;'
    )

    # Patch 2: null state guard in generateEmbedding
    old_state = """    const state = embeddingModelState;
    // Use ONNX model if available"""
    new_state = """    const state = embeddingModelState;
    // Guard against null state (loadEmbeddingModel may have failed)
    if (!state) {
        const embedding = generateHashEmbedding(text, 128);
        return { embedding, dimensions: 128, model: 'hash-fallback' };
    }
    // Use ONNX model if available"""

    if old_state in content and 'Guard against null state' not in content:
        content = content.replace(old_state, new_state)

    # Patch 3: local model path for @xenova/transformers
    old_load = """        // Try to import @xenova/transformers for ONNX embeddings
        const transformers = await import('@xenova/transformers').catch(() => null);
        if (transformers) {
            if (verbose) {
                console.log('Loading ONNX embedding model (all-MiniLM-L6-v2)...');
            }
            // Use small, fast model for local embeddings
            const { pipeline } = transformers;
            const embedder = await pipeline('feature-extraction', 'Xenova/all-MiniLM-L6-v2');"""

    new_load = """        // Try to import @xenova/transformers for ONNX embeddings
        const transformers = await import('@xenova/transformers').catch(() => null);
        if (transformers) {
            if (verbose) {
                console.log('Loading ONNX embedding model (all-MiniLM-L6-v2)...');
            }
            // Configure local model path to avoid HuggingFace download issues
            const localModelDir = path.resolve(process.cwd(), '.claude-flow', 'models');
            if (fs.existsSync(path.join(localModelDir, 'all-MiniLM-L6-v2', 'onnx', 'model.onnx'))) {
                transformers.env.localModelPath = localModelDir;
                transformers.env.allowRemoteModels = false;
                transformers.env.allowLocalModels = true;
            }
            // Use small, fast model for local embeddings
            const { pipeline } = transformers;
            const modelName = fs.existsSync(path.join(localModelDir, 'all-MiniLM-L6-v2', 'onnx', 'model.onnx'))
                ? 'all-MiniLM-L6-v2' : 'Xenova/all-MiniLM-L6-v2';
            const embedder = await pipeline('feature-extraction', modelName, { quantized: false });"""

    if old_load in content:
        content = content.replace(old_load, new_load)

    if content != original:
        with open(fpath, 'w') as f:
            f.write(content)
        patched_count += 1
        print(f"  [PATCHED] {fpath}")
    else:
        skipped_count += 1
        print(f"  [OK] Already patched: {fpath}")


def patch_embedding_service(fpath):
    global patched_count, skipped_count
    if not os.path.exists(fpath):
        return
    with open(fpath, 'r') as f:
        content = f.read()

    old = "this.pipeline = await transformers.pipeline('feature-extraction', this.config.model);"
    new = """// Configure local model path to avoid HuggingFace download issues
                const path = await import('path');
                const fs = await import('fs');
                const localModelDir = path.resolve(process.cwd(), '.claude-flow', 'models');
                const localModelName = this.config.model.replace('Xenova/', '');
                if (fs.existsSync(path.join(localModelDir, localModelName, 'onnx', 'model.onnx'))) {
                    transformers.env.localModelPath = localModelDir;
                    transformers.env.allowRemoteModels = false;
                    transformers.env.allowLocalModels = true;
                    this.pipeline = await transformers.pipeline('feature-extraction', localModelName, { quantized: false });
                } else {
                    this.pipeline = await transformers.pipeline('feature-extraction', this.config.model);
                }"""

    if old in content and 'Configure local model path' not in content:
        content = content.replace(old, new)
        with open(fpath, 'w') as f:
            f.write(content)
        patched_count += 1
        print(f"  [PATCHED] {fpath}")
    else:
        skipped_count += 1
        print(f"  [OK] Already patched: {fpath}")


# === Discover all installation paths ===
print("=== Patching memory-initializer.js ===")

# npx cache
for f in glob.glob(os.path.expanduser("~/.npm/_npx/*/node_modules/@claude-flow/cli/dist/src/memory/memory-initializer.js")):
    patch_memory_initializer(f)

# Global ruflo
patch_memory_initializer("/opt/homebrew/lib/node_modules/ruflo/node_modules/@claude-flow/cli/dist/src/memory/memory-initializer.js")

# Project local
cwd = os.getcwd()
patch_memory_initializer(os.path.join(cwd, "node_modules/@claude-flow/cli/dist/src/memory/memory-initializer.js"))

print("\n=== Patching EmbeddingService.js (AgentDB) ===")

# npx cache
for f in glob.glob(os.path.expanduser("~/.npm/_npx/*/node_modules/agentdb/dist/src/controllers/EmbeddingService.js")):
    patch_embedding_service(f)

# Global ruflo
patch_embedding_service("/opt/homebrew/lib/node_modules/ruflo/node_modules/agentdb/dist/src/controllers/EmbeddingService.js")

# Project local
patch_embedding_service(os.path.join(cwd, "node_modules/agentdb/dist/src/controllers/EmbeddingService.js"))

print(f"\n=== Summary: {patched_count} files patched, {skipped_count} already up-to-date ===")
PYEOF
