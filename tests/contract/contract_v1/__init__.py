"""Frozen contract-v1 test cases (ADR-V2-017 §2 / plan §S0.T2).

Every test in this package MUST carry ``@pytest.mark.contract_v1`` and
MUST remain schema-stable after S0.T6 freezes contract-v1.0.0. Any
behavioural change after freeze requires a contract version bump and a
DEFERRED_LEDGER entry.
"""
