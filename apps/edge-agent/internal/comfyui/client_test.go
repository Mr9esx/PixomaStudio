package comfyui

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"os"
	"testing"
	"time"

	"github.com/google/uuid"
)

func TestRealComfyRejectsAnUnavailableModel(t *testing.T) {
	address := os.Getenv("PIXOMA_TEST_COMFY_URL")
	workflowPath := os.Getenv("PIXOMA_TEST_COMFY_WORKFLOW")
	if address == "" || workflowPath == "" {
		t.Skip("需要实际 ComfyUI 地址与验收工作流文件")
	}
	data, err := os.ReadFile(workflowPath)
	if err != nil {
		t.Fatal(err)
	}
	graph := Graph{}
	if err := json.Unmarshal(data, &graph); err != nil {
		t.Fatal(err)
	}
	node, exists := graph["10"]
	if !exists {
		t.Fatal("验收工作流缺少 UNETLoader")
	}
	node.Inputs["unet_name"] = json.RawMessage(`"__studio_unavailable_model__.safetensors"`)
	client := &Client{BaseURL: address, HTTP: &http.Client{Timeout: 10 * time.Second}}
	promptID, err := client.Submit(
		context.Background(),
		graph,
		uuid.NewString(),
		uuid.NewString(),
	)
	var rejection *HTTPError
	if !errors.As(err, &rejection) || rejection.StatusCode != http.StatusBadRequest || promptID != "" {
		t.Fatalf("ComfyUI 未明确拒绝不可用模型：prompt_id=%q，error=%v", promptID, err)
	}
}
