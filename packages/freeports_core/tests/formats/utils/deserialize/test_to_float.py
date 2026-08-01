import pytest
from freeports_analysis.formats.utils.deserialize import to_float

test_cases = {"200": 200.0, "309.00": 309.0, "  090.070,00 ": 90070, "4,500": 4.5}


@pytest.mark.parametrize("data", test_cases)
def test_correct_formattig(data):
    assert to_float(data) == test_cases[data]
