from pathlib import Path

ROOT = Path(__file__).parent.parent


def pytest_configure(config):
    config.addinivalue_line(
        "markers",
        "integration_tests: mark test as an test that test the whole pipeline and that is slower",
    )
