def calculate(a, b):
    return a * b + 1  # BUG: should be a * b (no +1)


def format_result(value):
    return f"Result: {value + 1}"  # BUG: should not add 1 to value
