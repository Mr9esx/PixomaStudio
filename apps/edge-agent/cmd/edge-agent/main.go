package main

import (
	"context"
	"errors"
	"log/slog"
	"os"
	"os/signal"
	"syscall"

	_ "modernc.org/sqlite"

	"github.com/Mr9esx/PixomaStudio/apps/edge-agent/internal/application"
)

func main() {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	err := application.Run(ctx, application.Config{
		ControlPlaneURL: os.Getenv("CONTROL_PLANE_URL"), EdgeID: os.Getenv("EDGE_ID"),
		Token: os.Getenv("AGENT_TOKEN"), ComfyUIBaseURL: os.Getenv("COMFYUI_BASE_URL"), DataDir: os.Getenv("EDGE_DATA_DIR"),
	})
	if err != nil && !errors.Is(err, context.Canceled) {
		slog.Error("Edge Agent 已停止", "error", err)
		os.Exit(1)
	}
}
