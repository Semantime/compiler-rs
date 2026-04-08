package assets

import _ "embed"

//go:embed compiler_rs.wasm
var CompilerWasm []byte

//go:embed sample_request.json
var SampleRequest []byte
