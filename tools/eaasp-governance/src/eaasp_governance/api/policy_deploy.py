"""契约 1: 策略部署 API (§8.1).

PUT  /v1/policies/deploy             — compile & store policy (versioned, BH-D10)
GET  /v1/policies                     — list deployed policies (current versions)
GET  /v1/policies/{id}                — get current policy details
GET  /v1/policies/{id}/versions       — list all versions (BH-D10)
POST /v1/policies/{id}/rollback       — rollback to a specific version (BH-D10)
"""

from __future__ import annotations

from datetime import datetime, timezone

from fastapi import APIRouter, HTTPException, Query, Request

from eaasp_governance.compiler import CompileError, compile_policy_yaml, compile_yaml_to_hooks

router = APIRouter(prefix="/v1/policies", tags=["policies"])


def _build_policy_record(bundle, hooks_json: str, digest: str, version_num: int) -> dict:
    """Build a policy record dict from a compiled bundle."""
    return {
        "id": bundle.metadata.name,
        "name": bundle.metadata.name,
        "scope": bundle.metadata.scope,
        "org_unit": bundle.metadata.org_unit,
        "version": bundle.metadata.version,
        "version_num": version_num,
        "rules_count": len(bundle.rules),
        "compiled_hooks_json": hooks_json,
        "compiled_hooks_digest": digest,
        "deployed_at": datetime.now(timezone.utc).isoformat(),
    }


@router.put("/deploy")
async def deploy_policy(request: Request):
    """Compile and deploy a policy YAML.

    Appends to version history (BH-D10). Current = latest version.
    """
    body = await request.body()
    yaml_content = body.decode("utf-8")

    try:
        hooks_json, digest = compile_yaml_to_hooks(yaml_content)
    except CompileError as e:
        raise HTTPException(status_code=400, detail=str(e))

    bundle = compile_policy_yaml(yaml_content)
    policy_id = bundle.metadata.name

    # Version history: policy_store[id] = list[dict]
    store = request.app.state.policy_store
    if policy_id not in store:
        store[policy_id] = []

    version_num = len(store[policy_id]) + 1
    record = _build_policy_record(bundle, hooks_json, digest, version_num)
    store[policy_id].append(record)

    return {
        "policy_id": policy_id,
        "rules_count": len(bundle.rules),
        "compiled_hooks_digest": digest,
        "version_num": version_num,
    }


def _current(store: dict, policy_id: str) -> dict | None:
    """Get the current (latest) version of a policy."""
    versions = store.get(policy_id, [])
    return versions[-1] if versions else None


@router.get("")
async def list_policies(request: Request):
    """List all deployed policies (current versions only)."""
    store = request.app.state.policy_store
    result = []
    for policy_id, versions in store.items():
        if versions:
            p = versions[-1]  # current = latest
            result.append({
                "id": p["id"],
                "name": p["name"],
                "scope": p["scope"],
                "org_unit": p["org_unit"],
                "version": p["version"],
                "version_num": p["version_num"],
                "rules_count": p["rules_count"],
            })
    return result


@router.get("/{policy_id}")
async def get_policy(policy_id: str, request: Request):
    """Get current policy details including compiled hooks."""
    store = request.app.state.policy_store
    policy = _current(store, policy_id)
    if not policy:
        raise HTTPException(status_code=404, detail=f"Policy not found: {policy_id}")
    return policy


@router.get("/{policy_id}/versions")
async def list_policy_versions(policy_id: str, request: Request):
    """List all versions of a policy (BH-D10)."""
    store = request.app.state.policy_store
    versions = store.get(policy_id, [])
    if not versions:
        raise HTTPException(status_code=404, detail=f"Policy not found: {policy_id}")

    return {
        "policy_id": policy_id,
        "current_version": len(versions),
        "versions": [
            {
                "version_num": v["version_num"],
                "version": v["version"],
                "rules_count": v["rules_count"],
                "compiled_hooks_digest": v["compiled_hooks_digest"],
                "deployed_at": v.get("deployed_at", ""),
            }
            for v in versions
        ],
    }


@router.post("/{policy_id}/rollback")
async def rollback_policy(
    policy_id: str,
    request: Request,
    version: int = Query(..., description="Version number to rollback to"),
):
    """Rollback to a specific version (BH-D10).

    Creates a new version entry that copies the target version's content.
    """
    store = request.app.state.policy_store
    versions = store.get(policy_id, [])
    if not versions:
        raise HTTPException(status_code=404, detail=f"Policy not found: {policy_id}")

    # Find target version
    target = None
    for v in versions:
        if v["version_num"] == version:
            target = v
            break

    if not target:
        raise HTTPException(
            status_code=400,
            detail=f"Version {version} not found for policy {policy_id}",
        )

    # Create new version entry as a copy of the target
    new_version_num = len(versions) + 1
    rollback_record = {**target, "version_num": new_version_num,
                       "deployed_at": datetime.now(timezone.utc).isoformat()}
    versions.append(rollback_record)

    return {
        "policy_id": policy_id,
        "rolled_back_to": version,
        "new_version_num": new_version_num,
        "rules_count": target["rules_count"],
    }
