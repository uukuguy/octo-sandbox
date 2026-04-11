"""EAASP v2 SessionPayload — priority block tests.

Covers:
  * P1 PolicyContext + P2 EventContext survive trimming (they are never removed).
  * P5 UserPreferences is removed first; then P4; then P3.
  * Proto round-trip: dict → SessionPayload → Initialize → HermesSession.
"""

from hermes_runtime._fix_proto_imports import fix as _fix_proto_imports

_fix_proto_imports()

from eaasp.runtime.v2 import common_pb2  # noqa: E402

from hermes_runtime.mapper import extract_policy_hooks, extract_skill_content, extract_user_id
from hermes_runtime.session import HermesSession, SessionManager


def _build_full_payload() -> common_pb2.SessionPayload:
    """Build a SessionPayload with all 5 priority blocks populated."""
    return common_pb2.SessionPayload(
        policy_context=common_pb2.PolicyContext(
            org_unit="eng",
            policy_version="v1.0.0",
            deploy_timestamp="2026-04-11T00:00:00Z",
            hooks=[
                common_pb2.ManagedHook(
                    hook_id="h1",
                    hook_type="PRE_TOOL_USE",
                    condition="tool_name == 'terminal'",
                    action="allow",
                    precedence=10,
                    scope="managed",
                )
            ],
        ),
        event_context=common_pb2.EventContext(
            event_id="evt-1",
            event_type="alert.fired",
            severity="warning",
            source="prometheus",
            payload_json='{"metric": "cpu"}',
            timestamp="2026-04-11T00:00:01Z",
        ),
        memory_refs=[
            common_pb2.MemoryRef(
                memory_id="m1",
                memory_type="calibration",
                relevance_score=0.87,
                content="Threshold lowered 2 weeks ago.",
                source_session_id="hermes-aaaabbbbcccc",
                created_at="2026-03-28T00:00:00Z",
            )
        ],
        skill_instructions=common_pb2.SkillInstructions(
            skill_id="on-call-triage",
            name="On-Call Triage",
            content="Gather metrics, correlate with recent deploys, propose mitigation.",
        ),
        user_preferences=common_pb2.UserPreferences(
            user_id="alice",
            language="en",
            timezone="Asia/Shanghai",
            prefs={"verbosity": "concise"},
        ),
        session_id="sess-xyz",
        user_id="alice",
        runtime_id="hermes-runtime",
        created_at="2026-04-11T00:00:02Z",
    )


# ── P1 + P2 must survive trimming ───────────────────────────────

def test_p1_and_p2_survive_when_p5_and_p4_and_p3_trimmed():
    """Even after trimming P5/P4/P3, P1 PolicyContext and P2 EventContext remain."""
    session = HermesSession(
        session_id="s-1",
        policy_context={"org_unit": "eng", "hooks": [{"hook_id": "h1"}]},
        event_context={"event_id": "evt-1", "event_type": "alert.fired"},
        memory_refs=[{"memory_id": "m1"}],
        skill_instructions={"skill_id": "sk1"},
        user_preferences={"user_id": "alice", "prefs": {"verbosity": "concise"}},
    )

    session.trim_p5()
    assert session.user_preferences is None

    session.trim_p4()
    assert session.skill_instructions is None

    session.trim_p3()
    assert session.memory_refs == []

    # P1 and P2 MUST still be present
    assert session.policy_context is not None
    assert session.policy_context["org_unit"] == "eng"
    assert session.event_context is not None
    assert session.event_context["event_id"] == "evt-1"


# ── P5 trimmed before P4 before P3 ─────────────────────────────

def test_trim_order_p5_first_then_p4_then_p3():
    session = HermesSession(
        session_id="s-2",
        policy_context={"org_unit": "eng"},
        memory_refs=[{"memory_id": "m1"}, {"memory_id": "m2"}],
        skill_instructions={"skill_id": "sk1"},
        user_preferences={"user_id": "bob", "prefs": {}},
    )

    # Step 1: trim P5 only
    session.trim_p5()
    assert session.user_preferences is None
    assert session.skill_instructions is not None
    assert len(session.memory_refs) == 2

    # Step 2: trim P4
    session.trim_p4()
    assert session.skill_instructions is None
    assert len(session.memory_refs) == 2

    # Step 3: trim P3
    session.trim_p3()
    assert session.memory_refs == []


# ── Proto round-trip ───────────────────────────────────────────

def test_full_payload_round_trip_through_session_manager():
    """Full 5-block proto payload → SessionManager.create → HermesSession with all blocks."""
    payload = _build_full_payload()

    # Helpers should read priority blocks correctly
    assert extract_user_id(payload) == "alice"
    assert "Gather metrics" in extract_skill_content(payload)
    hooks = extract_policy_hooks(payload)
    assert len(hooks) == 1
    assert hooks[0]["hook_id"] == "h1"

    # Serialize to bytes and back (proto round-trip)
    blob = payload.SerializeToString()
    restored = common_pb2.SessionPayload()
    restored.ParseFromString(blob)

    assert restored.policy_context.org_unit == "eng"
    assert restored.event_context.event_id == "evt-1"
    assert len(restored.memory_refs) == 1
    assert restored.skill_instructions.skill_id == "on-call-triage"
    assert restored.user_preferences.user_id == "alice"
    assert restored.user_preferences.prefs["verbosity"] == "concise"

    # Assembly into HermesSession via SessionManager
    mgr = SessionManager()
    session = mgr.create(
        user_id=extract_user_id(restored),
        policy_context={
            "org_unit": restored.policy_context.org_unit,
            "hooks": extract_policy_hooks(restored),
        },
        event_context={"event_id": restored.event_context.event_id},
        memory_refs=[{"memory_id": m.memory_id} for m in restored.memory_refs],
        skill_instructions={"skill_id": restored.skill_instructions.skill_id},
        user_preferences={"user_id": restored.user_preferences.user_id},
    )

    assert session.policy_context["org_unit"] == "eng"
    assert session.event_context["event_id"] == "evt-1"
    assert session.skill_instructions["skill_id"] == "on-call-triage"
    assert session.user_preferences["user_id"] == "alice"
