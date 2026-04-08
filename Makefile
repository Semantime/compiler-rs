.PHONY: test viewer viewer-no-open viewer-json sync-compiler-cases \
	go-binding-wasm go-binding-test go-binding-check

PORT ?= 8765

test:
	cargo test --workspace

go-binding-wasm:
	./bindings/go/build-wasm.sh

go-binding-test:
	cd bindings/go && go test ./...

go-binding-check: go-binding-wasm go-binding-test

viewer-json:
	cargo run -- viewer-json

viewer:
	cargo build
	python3 tools/regression_viewer_py/server.py --compiler-bin target/debug/compiler-rs --port $(PORT) --open

viewer-no-open:
	cargo build
	python3 tools/regression_viewer_py/server.py --compiler-bin target/debug/compiler-rs --port $(PORT)

sync-compiler-cases:
	cargo build
	python3 tools/sync_compiler_cases.py --compiler-bin target/debug/compiler-rs
