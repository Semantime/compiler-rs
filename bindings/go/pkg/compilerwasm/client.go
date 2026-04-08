package compilerwasm

import (
	"context"
	"fmt"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
)

type Client struct {
	runtime wazero.Runtime
	module  api.Module
}

func New(ctx context.Context, wasmBytes []byte) (*Client, error) {
	runtime := wazero.NewRuntime(ctx)
	module, err := runtime.Instantiate(ctx, wasmBytes)
	if err != nil {
		runtime.Close(ctx)
		return nil, fmt.Errorf("instantiate wasm: %w", err)
	}

	return &Client{
		runtime: runtime,
		module:  module,
	}, nil
}

func (c *Client) Close(ctx context.Context) error {
	var firstErr error
	if c.module != nil {
		if err := c.module.Close(ctx); err != nil {
			firstErr = err
		}
	}
	if c.runtime != nil {
		if err := c.runtime.Close(ctx); err != nil && firstErr == nil {
			firstErr = err
		}
	}
	return firstErr
}

func (c *Client) DefaultPolicyJSON(ctx context.Context) (string, error) {
	return c.callNoInput(ctx, "compiler_default_policy_json")
}

func (c *Client) AnalyzeJSON(ctx context.Context, payload []byte) (string, error) {
	return c.callWithBytes(ctx, "compiler_analyze_json", payload)
}

func (c *Client) callNoInput(ctx context.Context, fnName string) (string, error) {
	fn, err := c.requiredFunction(fnName)
	if err != nil {
		return "", err
	}
	status, err := fn.Call(ctx)
	if err != nil {
		return "", err
	}
	if len(status) != 1 {
		return "", fmt.Errorf("%s returned unexpected result count %d", fnName, len(status))
	}
	if status[0] != 0 {
		return "", c.readHostError(ctx)
	}
	return c.readBuffer(ctx, "compiler_result_ptr", "compiler_result_len")
}

func (c *Client) callWithBytes(ctx context.Context, fnName string, payload []byte) (string, error) {
	alloc, err := c.requiredFunction("compiler_alloc")
	if err != nil {
		return "", err
	}
	free, err := c.requiredFunction("compiler_free")
	if err != nil {
		return "", err
	}
	fn, err := c.requiredFunction(fnName)
	if err != nil {
		return "", err
	}

	allocated, err := alloc.Call(ctx, uint64(len(payload)))
	if err != nil {
		return "", fmt.Errorf("alloc payload: %w", err)
	}
	if len(allocated) != 1 {
		return "", fmt.Errorf("alloc returned unexpected result count %d", len(allocated))
	}
	ptr := allocated[0]
	defer func() {
		_, _ = free.Call(ctx, ptr, uint64(len(payload)))
	}()

	if ok := c.module.Memory().Write(uint32(ptr), payload); !ok {
		return "", fmt.Errorf("write payload to wasm memory failed")
	}

	status, err := fn.Call(ctx, ptr, uint64(len(payload)))
	if err != nil {
		return "", err
	}
	if len(status) != 1 {
		return "", fmt.Errorf("%s returned unexpected result count %d", fnName, len(status))
	}
	if status[0] != 0 {
		return "", c.readHostError(ctx)
	}
	return c.readBuffer(ctx, "compiler_result_ptr", "compiler_result_len")
}

func (c *Client) readHostError(ctx context.Context) error {
	errMsg, err := c.readBuffer(ctx, "compiler_error_ptr", "compiler_error_len")
	if err != nil {
		return err
	}
	return fmt.Errorf("%s", errMsg)
}

func (c *Client) readBuffer(ctx context.Context, ptrFnName, lenFnName string) (string, error) {
	ptrFn, err := c.requiredFunction(ptrFnName)
	if err != nil {
		return "", err
	}
	lenFn, err := c.requiredFunction(lenFnName)
	if err != nil {
		return "", err
	}

	ptrResult, err := ptrFn.Call(ctx)
	if err != nil {
		return "", err
	}
	lenResult, err := lenFn.Call(ctx)
	if err != nil {
		return "", err
	}
	if len(ptrResult) != 1 || len(lenResult) != 1 {
		return "", fmt.Errorf("buffer accessors returned unexpected result shape")
	}

	ptr := uint32(ptrResult[0])
	size := uint32(lenResult[0])
	bytes, ok := c.module.Memory().Read(ptr, size)
	if !ok {
		return "", fmt.Errorf("read wasm buffer failed")
	}
	return string(bytes), nil
}

func (c *Client) requiredFunction(name string) (api.Function, error) {
	fn := c.module.ExportedFunction(name)
	if fn == nil {
		return nil, fmt.Errorf("missing wasm export %q", name)
	}
	return fn, nil
}
