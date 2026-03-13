def clamp(val, lo, hi):
    return max(lo, min(val, hi - 1))  # BUG: hi - 1 should be hi
