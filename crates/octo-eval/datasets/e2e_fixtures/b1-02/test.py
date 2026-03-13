import sys
sys.path.insert(0, '.')
from src import greet

try:
    result = greet("World")
    assert result == "Hello, World!", f"Expected 'Hello, World!', got '{result}'"
    result2 = greet("Alice")
    assert result2 == "Hello, Alice!", f"Expected 'Hello, Alice!', got '{result2}'"
    print("PASS")
except AssertionError as e:
    print(f"FAIL: {e}")
    sys.exit(1)
except Exception as e:
    print(f"FAIL: {e}")
    sys.exit(1)
