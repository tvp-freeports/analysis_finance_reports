import pytest
from freeports.utils.deserialize import to_int

test_cases = {"200": 200, "309.00": 309, "  090.070,00 ": 90070, "4,500": 4500}


@pytest.mark.parametrize("data", test_cases)
def test_correct_formattig(data):
    assert to_int(data) == test_cases[data]
