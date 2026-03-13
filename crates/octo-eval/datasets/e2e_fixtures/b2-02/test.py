import sys
sys.path.insert(0, '.')
from src import clamp

try:
    assert clamp(10, 0, 10) == 10, f"Expected 10, got {clamp(10, 0, 10)}"
    assert clamp(-5, 0, 10) == 0, f"Expected 0, got {clamp(-5, 0, 10)}"
    assert clamp(5, 0, 10) == 5, f"Expected 5, got {clamp(5, 0, 10)}"
    assert clamp(15, 0, 10) == 10, f"Expected 10, got {clamp(15, 0, 10)}"
    print("PASS")
except AssertionError as e:
    print(f"FAIL: {e}")
    sys.exit(1)
except Exception as e:
    print(f"FAIL: {e}")
    sys.exit(1)
