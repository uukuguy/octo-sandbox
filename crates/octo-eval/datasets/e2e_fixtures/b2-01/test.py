import sys
sys.path.insert(0, '.')
from src import is_positive

try:
    assert is_positive(5) == True, f"Expected True for 5, got {is_positive(5)}"
    assert is_positive(-3) == False, f"Expected False for -3, got {is_positive(-3)}"
    assert is_positive(0) == False, f"Expected False for 0, got {is_positive(0)}"
    print("PASS")
except AssertionError as e:
    print(f"FAIL: {e}")
    sys.exit(1)
except Exception as e:
    print(f"FAIL: {e}")
    sys.exit(1)
