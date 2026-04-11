from __future__ import annotations

import json

import pytest

from mock_scada.snapshots import (
    SAMPLE_DEVICE_IDS,
    build_snapshot,
    snapshot_hash,
)


def test_snapshot_is_deterministic_for_same_device() -> None:
    a = build_snapshot("xfmr-042", "5m")
    b = build_snapshot("xfmr-042", "5m")
    assert a == b
    assert snapshot_hash(a) == snapshot_hash(b)


def test_snapshot_hash_changes_when_device_changes() -> None:
    a = build_snapshot("xfmr-042", "5m")
    b = build_snapshot("brk-17", "5m")
    assert snapshot_hash(a) != snapshot_hash(b)


def test_snapshot_hash_changes_when_time_window_changes() -> None:
    a = build_snapshot("xfmr-042", "5m")
    b = build_snapshot("xfmr-042", "1h")
    assert snapshot_hash(a) != snapshot_hash(b)


def test_snapshot_shape_contains_required_fields() -> None:
    snap = build_snapshot("xfmr-042", "5m")
    assert snap["device_id"] == "xfmr-042"
    assert snap["time_window"] == "5m"
    assert snap["sample_count"] == len(snap["samples"])
    assert snap["sample_count"] >= 1
    for sample in snap["samples"]:
        assert "temperature_c" in sample
        assert "load_pct" in sample
        assert "doa_h2_ppm" in sample
        assert "t_offset_s" in sample
    assert set(snap["baseline"].keys()) == {
        "temperature_c",
        "load_pct",
        "doa_h2_ppm",
    }


def test_snapshot_falls_back_to_default_baseline_for_unknown_device() -> None:
    snap = build_snapshot("unknown-device-xyz", "5m")
    assert snap["device_id"] == "unknown-device-xyz"
    assert snap["baseline"]["temperature_c"] == pytest.approx(60.0)
    assert snap["baseline"]["load_pct"] == pytest.approx(0.70)
    assert snap["baseline"]["doa_h2_ppm"] == pytest.approx(20.0)


def test_snapshot_hash_is_stable_across_json_field_order() -> None:
    snap = build_snapshot("xfmr-042", "5m")
    reordered = json.loads(json.dumps(snap))
    assert snapshot_hash(reordered) == snapshot_hash(snap)


def test_sample_device_ids_are_non_empty() -> None:
    assert len(SAMPLE_DEVICE_IDS) >= 1
    for dev in SAMPLE_DEVICE_IDS:
        assert isinstance(dev, str) and dev
