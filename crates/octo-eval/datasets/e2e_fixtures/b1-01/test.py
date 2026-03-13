import sys
sys.path.insert(0, '.')
from src import count_items

try:
    assert count_items([1, 2, 3]) == 3, f"Expected 3, got {count_items([1, 2, 3])}"
    assert count_items([]) == 0, f"Expected 0, got {count_items([])}"
    assert count_items([42]) == 1, f"Expected 1, got {count_items([42])}"
    print("PASS")
except AssertionError as e:
    print(f"FAIL: {e}")
    sys.exit(1)
except Exception as e:
    print(f"FAIL: {e}")
    sys.exit(1)
