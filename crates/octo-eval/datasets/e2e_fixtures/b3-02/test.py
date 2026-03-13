import sys
sys.path.insert(0, '.')
from src import to_pairs

try:
    result = to_pairs([1, 2, 3, 4])
    assert result == [(1, 2), (3, 4)], f"Expected [(1, 2), (3, 4)], got {result}"
    assert isinstance(result, list), f"Expected list, got {type(result).__name__}"
    result2 = to_pairs(["a", "b"])
    assert result2 == [("a", "b")], f"Expected [('a', 'b')], got {result2}"
    print("PASS")
except AssertionError as e:
    print(f"FAIL: {e}")
    sys.exit(1)
except Exception as e:
    print(f"FAIL: {e}")
    sys.exit(1)
