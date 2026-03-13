import sys
sys.path.insert(0, '.')
from src import parse_json

try:
    result = parse_json('{"a": 1}')
    assert result == {"a": 1}, f"Expected {{'a': 1}}, got {result}"
    result2 = parse_json('[1, 2, 3]')
    assert result2 == [1, 2, 3], f"Expected [1, 2, 3], got {result2}"
    print("PASS")
except AssertionError as e:
    print(f"FAIL: {e}")
    sys.exit(1)
except Exception as e:
    print(f"FAIL: {e}")
    sys.exit(1)
