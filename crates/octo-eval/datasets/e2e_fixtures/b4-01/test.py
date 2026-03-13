import sys
sys.path.insert(0, '.')
from src import calculate, format_result

try:
    assert calculate(3, 4) == 12, f"Expected 12, got {calculate(3, 4)}"
    assert calculate(0, 5) == 0, f"Expected 0, got {calculate(0, 5)}"
    assert format_result(42) == "Result: 42", f"Expected 'Result: 42', got '{format_result(42)}'"
    assert format_result(0) == "Result: 0", f"Expected 'Result: 0', got '{format_result(0)}'"
    print("PASS")
except AssertionError as e:
    print(f"FAIL: {e}")
    sys.exit(1)
except Exception as e:
    print(f"FAIL: {e}")
    sys.exit(1)
