.PHONY: build run format lint check clean

build:
	cargo build

run:
	cargo run

format:
	cargo fmt

lint:
	cargo clippy

check:
	cargo check

clean:
	cargo clean
