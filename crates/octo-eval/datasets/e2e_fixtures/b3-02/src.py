def to_pairs(items):
    return {items[i]: items[i + 1] for i in range(0, len(items), 2)}  # BUG: returns dict, should return list of tuples
