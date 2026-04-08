package compilerwasm

import (
	"compiler-rs/bindings/go/internal/assets"
	"context"
	"strings"
	"testing"
)

func TestClientAnalyzeJSON(t *testing.T) {
	ctx := context.Background()

	client, err := New(ctx, assets.CompilerWasm)
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}
	defer func() {
		if err := client.Close(ctx); err != nil {
			t.Fatalf("Close() error = %v", err)
		}
	}()

	policyJSON, err := client.DefaultPolicyJSON(ctx)
	if err != nil {
		t.Fatalf("DefaultPolicyJSON() error = %v", err)
	}
	if !strings.Contains(policyJSON, "\"sensitivity\":\"balanced\"") {
		t.Fatalf("DefaultPolicyJSON() = %s, want balanced policy", policyJSON)
	}

	responseJSON, err := client.AnalyzeJSON(ctx, assets.SampleRequest)
	if err != nil {
		t.Fatalf("AnalyzeJSON() error = %v", err)
	}
	if !strings.Contains(responseJSON, "\"subject_id\":\"host-1\"") {
		t.Fatalf("AnalyzeJSON() = %s, want host-1 subject", responseJSON)
	}
}
