
.PHONY: test test-full test-unit test-rust test-python test-rust-unit test-python-unit

test: test-unit

test-full: test-rust-full test-python-full

test-unit: test-rust-unit test-python-unit

test-python-unit:
	pytest -m "not integration_tests and not online_tests and not benchmarks"

test-rust-unit:
	cd rust && cargo test --lib

test-python-full:
	pytest

test-rust-full:
	cd rust && cargo test