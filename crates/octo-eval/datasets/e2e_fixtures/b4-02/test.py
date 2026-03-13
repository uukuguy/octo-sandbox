import sys
sys.path.insert(0, '.')
from caller import process_url

try:
    result = process_url("https://example.com")
    assert result == "https://example.com", f"Expected 'https://example.com', got '{result}'"
    result2 = process_url("test")
    assert result2 == "test", f"Expected 'test', got '{result2}'"
    print("PASS")
except AssertionError as e:
    print(f"FAIL: {e}")
    sys.exit(1)
except Exception as e:
    print(f"FAIL: {e}")
    sys.exit(1)
