package application

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"sync"
	"sync/atomic"
	"time"

	"github.com/google/uuid"

	"github.com/Mr9esx/PixomaStudio/apps/edge-agent/internal/comfyui"
	"github.com/Mr9esx/PixomaStudio/apps/edge-agent/internal/controlplane"
	"github.com/Mr9esx/PixomaStudio/apps/edge-agent/internal/journal"
)

type Config struct {
	ControlPlaneURL string
	EdgeID          string
	Token           string
	ComfyUIBaseURL  string
	DataDir         string
}

type taskPackage struct {
	ProtocolVersion int           `json:"protocol_version"`
	ExecutionID     string        `json:"execution_id"`
	AttemptID       string        `json:"attempt_id"`
	ComfyUIBaseURL  string        `json:"comfyui_base_url"`
	ContentDigest   string        `json:"content_digest"`
	Workflow        comfyui.Graph `json:"workflow"`
	Outputs         []struct {
		Key    string `json:"key"`
		NodeID string `json:"node_id"`
		Index  int    `json:"index"`
	} `json:"outputs"`
	InputFiles []struct {
		FileID    string `json:"file_id"`
		URL       string `json:"url"`
		SHA256    string `json:"sha256"`
		SizeBytes int64  `json:"size_bytes"`
		NodeID    string `json:"node_id"`
		FieldPath string `json:"field_path"`
	} `json:"input_files"`
}

type App struct {
	config  Config
	store   *journal.Store
	plane   *controlplane.Client
	http    *http.Client
	active  atomic.Pointer[controlplane.Claim]
	sampler sampler
}

var errCancelled = errors.New("执行已取消")
var errResultUnknown = errors.New("ComfyUI 队列和历史均缺少原任务，执行结果无法确认")

func Run(ctx context.Context, config Config) (result error) {
	if config.ControlPlaneURL == "" || config.Token == "" || config.DataDir == "" {
		return errors.New("请配置 CONTROL_PLANE_URL、AGENT_TOKEN 和 EDGE_DATA_DIR")
	}
	address, err := url.Parse(config.ControlPlaneURL)
	if err != nil || address.Host == "" || (address.Scheme != "http" && address.Scheme != "https") {
		return errors.New("CONTROL_PLANE_URL 无效")
	}
	store, err := journal.Open(ctx, filepath.Join(config.DataDir, "agent.sqlite"))
	if err != nil {
		return err
	}
	defer func() { result = errors.Join(result, store.Close()) }()
	client := &http.Client{
		Timeout: 30 * time.Second,
		CheckRedirect: func(_ *http.Request, _ []*http.Request) error {
			return errors.New("节点请求不接受重定向")
		},
	}
	app := &App{config: config, store: store, http: client}
	app.plane = &controlplane.Client{BaseURL: config.ControlPlaneURL, Token: config.Token, HTTP: client}
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	started := uuid.NewString()
	revision, comfyAddress, err := app.presence(
		ctx,
		started,
		0,
		"",
	)
	if err != nil {
		return err
	}
	if _, _, err := app.presence(
		ctx,
		started,
		revision,
		comfyAddress,
	); err != nil {
		return err
	}
	var monitorError error
	var wait sync.WaitGroup
	wait.Go(func() {
		monitorError = app.monitor(ctx, started, revision, comfyAddress)
		cancel()
	})
	err = app.work(ctx)
	cancel()
	wait.Wait()
	if errors.Is(err, context.Canceled) {
		err = nil
	}
	if errors.Is(monitorError, context.Canceled) {
		monitorError = nil
	}
	return errors.Join(err, monitorError)
}

func (a *App) presence(
	ctx context.Context,
	started string,
	revision int64,
	address string,
) (int64, string, error) {
	comfyState := map[string]any{"ready": false}
	hardware, metrics := a.sampler.sample(ctx)
	if address != "" {
		comfy := &comfyui.Client{BaseURL: address, HTTP: a.http}
		info := map[string]any{}
		var stats comfyui.SystemStats
		if err := comfy.Get(ctx, "/object_info", &info); err != nil {
			comfyState["error"] = err.Error()
		} else if err := comfy.Get(ctx, "/system_stats", &stats); err != nil {
			comfyState["error"] = err.Error()
		} else {
			comfyState["ready"] = true
			comfyState["object_info"] = info
			gpus := []map[string]any{}
			for _, device := range stats.Devices {
				gpus = append(gpus, map[string]any{"name": device.Name, "vram_bytes": device.VRAMTotal})
			}
			hardware["gpus"] = gpus
			comfyState["version"] = stats.System.ComfyVersion
			if metrics["gpus"] == nil {
				deviceMetrics := []map[string]any{}
				for _, device := range stats.Devices {
					if device.VRAMTotal == 0 || device.VRAMFree > device.VRAMTotal {
						continue
					}
					used := device.VRAMTotal - device.VRAMFree
					deviceMetrics = append(deviceMetrics, map[string]any{
						"id": strconv.Itoa(device.Index), "name": device.Name, "vram_total_bytes": device.VRAMTotal,
						"vram_used_bytes": used, "vram_usage_percent": float64(used) / float64(device.VRAMTotal) * 100,
					})
				}
				metrics["gpus"] = deviceMetrics
			}
		}
	}
	var response struct {
		NodeID                string `json:"node_id"`
		ConfigurationRevision int64  `json:"configuration_revision"`
		ComfyUIBaseURL        string `json:"comfyui_base_url"`
	}
	_, err := a.plane.JSON(
		ctx,
		http.MethodPost,
		"/agent/v1/presence",
		map[string]any{
			"protocol_version": 1, "agent_version": "0.1.0", "agent_instance_id": a.store.InstanceID,
			"started_id": started, "configuration_revision": revision, "comfyui": comfyState, "hardware": hardware,
			"metrics": metrics,
		},
		&response,
	)
	if err != nil {
		return 0, "", err
	}
	if a.config.EdgeID != "" && a.config.EdgeID != response.NodeID {
		return 0, "", errors.New("EDGE_ID 与 Studio 节点不一致")
	}
	if a.config.ComfyUIBaseURL != "" && a.config.ComfyUIBaseURL != response.ComfyUIBaseURL {
		return 0, "", errors.New("COMFYUI_BASE_URL 与节点配置不一致")
	}
	return response.ConfigurationRevision, response.ComfyUIBaseURL, nil
}

func (a *App) monitor(ctx context.Context, started string, revision int64, address string) error {
	ticker := time.NewTicker(10 * time.Second)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return nil
		case <-ticker.C:
			var err error
			revision, address, err = a.presence(
				ctx,
				started,
				revision,
				address,
			)
			if err != nil {
				return err
			}
			if current := a.active.Load(); current != nil {
				if _, err := a.heartbeat(ctx, current); err != nil {
					return err
				}
			}
		}
	}
}

func (a *App) work(ctx context.Context) error {
	ticker := time.NewTicker(2 * time.Second)
	defer ticker.Stop()
	for {
		job, err := a.store.Active(ctx)
		if err != nil {
			return err
		}
		resuming := job != nil
		if job == nil {
			var claim controlplane.Claim
			status, err := a.plane.JSON(
				ctx,
				http.MethodGet,
				"/agent/v1/jobs/claim",
				nil,
				&claim,
			)
			if err != nil {
				return err
			}
			if status != http.StatusNoContent {
				if claim.ProtocolVersion != 1 {
					return errors.New("Studio 节点协议版本不受支持")
				}
				if _, err := uuid.Parse(claim.ExecutionID); err != nil {
					return fmt.Errorf("执行标识无效：%w", err)
				}
				var raw json.RawMessage
				if _, err := a.plane.JSON(
					ctx,
					http.MethodGet,
					claim.PackageURL,
					nil,
					&raw,
				); err != nil {
					return err
				}
				job = &journal.Job{ExecutionID: claim.ExecutionID, AttemptID: claim.AttemptID, Stage: "claimed", Package: raw}
				if err := a.store.Save(ctx, job); err != nil {
					return err
				}
			}
		}
		if job != nil {
			a.active.Store(&controlplane.Claim{ExecutionID: job.ExecutionID, AttemptID: job.AttemptID})
			if err := a.execute(ctx, job, resuming); err != nil {
				return err
			}
			a.active.Store(nil)
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-ticker.C:
		}
	}
}

func (a *App) heartbeat(ctx context.Context, claim *controlplane.Claim) (bool, error) {
	var response struct {
		CancelRequested bool `json:"cancel_requested"`
	}
	_, err := a.plane.JSON(
		ctx,
		http.MethodPost,
		"/agent/v1/executions/"+claim.ExecutionID+"/heartbeat",
		map[string]string{"attempt_id": claim.AttemptID},
		&response,
	)
	return response.CancelRequested, err
}

func (a *App) flushEvent(ctx context.Context, job *journal.Job) error {
	if len(job.Pending) == 0 {
		return nil
	}
	if _, err := a.plane.JSON(
		ctx,
		http.MethodPost,
		"/agent/v1/executions/"+job.ExecutionID+"/events",
		job.Pending,
		nil,
	); err != nil {
		return err
	}
	job.Sequence++
	job.Stage = job.PendingStage
	job.Pending = nil
	job.PendingStage = ""
	return a.store.Save(ctx, job)
}

func (a *App) event(ctx context.Context, job *journal.Job, kind, nextStage string, payload any) error {
	document, err := json.Marshal(map[string]any{
		"attempt_id": job.AttemptID, "sequence": job.Sequence + 1, "kind": kind, "payload": payload,
	})
	if err != nil {
		return fmt.Errorf("编码执行事件：%w", err)
	}
	job.Pending = document
	job.PendingStage = nextStage
	if err := a.store.Save(ctx, job); err != nil {
		return err
	}
	return a.flushEvent(ctx, job)
}

func (a *App) execute(ctx context.Context, job *journal.Job, resuming bool) error {
	var task taskPackage
	if err := json.Unmarshal(job.Package, &task); err != nil {
		return fmt.Errorf("读取持久任务包：%w", err)
	}
	if task.ProtocolVersion != 1 || task.ExecutionID != job.ExecutionID || task.AttemptID != job.AttemptID {
		return errors.New("任务包与执行日志不匹配")
	}
	if err := a.flushEvent(ctx, job); err != nil {
		return err
	}
	comfy := &comfyui.Client{BaseURL: task.ComfyUIBaseURL, HTTP: a.http}
	if job.Stage == "claimed" {
		if err := a.event(
			ctx,
			job,
			"accepted",
			"accepted",
			map[string]string{"content_digest": task.ContentDigest},
		); err != nil {
			return err
		}
	}
	if job.Stage == "accepted" {
		if err := a.prepare(ctx, comfy, &task); err != nil {
			return err
		}
		if err := a.waitAvailable(ctx, comfy, job); err != nil {
			if errors.Is(err, errCancelled) {
				job.Stage = "complete_pending"
				job.CompletionStatus = "cancelled"
				job.Error = err.Error()
				if err := a.store.Save(ctx, job); err != nil {
					return err
				}
				return a.confirm(ctx, job)
			}
			return err
		}
		if err := a.event(
			ctx,
			job,
			"submission_started",
			"submitting",
			map[string]string{"content_digest": task.ContentDigest},
		); err != nil {
			return err
		}
		promptID, err := comfy.Submit(
			ctx,
			task.Workflow,
			job.ExecutionID,
			job.AttemptID,
		)
		if err != nil {
			if ctx.Err() != nil {
				return ctx.Err()
			}
			var responseError *comfyui.HTTPError
			if errors.As(err, &responseError) && responseError.StatusCode >= 400 && responseError.StatusCode < 500 {
				job.Stage = "complete_pending"
				job.CompletionStatus = "failed"
				job.Error = err.Error()
				if err := a.store.Save(ctx, job); err != nil {
					return err
				}
				return a.confirm(ctx, job)
			}
			if reportErr := a.event(
				ctx,
				job,
				"submission_unknown",
				"submission_unknown",
				map[string]string{"error": err.Error()},
			); reportErr != nil {
				return reportErr
			}
			return err
		}
		job.PromptID = promptID
		if err := a.store.Save(ctx, job); err != nil {
			return err
		}
		slog.Info("已提交 ComfyUI", "execution_id", job.ExecutionID, "prompt_id", promptID)
		resuming = false
	}
	if job.Stage == "submitting" || job.Stage == "submission_unknown" {
		if job.PromptID == "" {
			if !resuming {
				return errors.New("提交阶段缺少 ComfyUI 编号")
			}
			promptID, err := comfy.FindSubmission(ctx, job.ExecutionID, job.AttemptID)
			if err != nil {
				return err
			}
			job.PromptID = promptID
			if err := a.store.Save(ctx, job); err != nil {
				return err
			}
		}
		if err := a.event(
			ctx,
			job,
			"submitted",
			"running",
			map[string]string{"prompt_id": job.PromptID},
		); err != nil {
			return err
		}
	}
	if job.Stage == "running" || job.Stage == "collecting" {
		entry, err := a.waitHistory(ctx, comfy, job)
		if err != nil {
			if errors.Is(err, errCancelled) || errors.Is(err, errResultUnknown) {
				job.Stage = "complete_pending"
				job.CompletionStatus = "result_unknown"
				if errors.Is(err, errCancelled) {
					job.CompletionStatus = "cancelled"
				}
				job.Error = err.Error()
				if err := a.store.Save(ctx, job); err != nil {
					return err
				}
				return a.confirm(ctx, job)
			}
			return err
		}
		if entry.Status.StatusStr == "error" {
			job.CompletionStatus = "failed"
			job.Error = fmt.Sprintf("ComfyUI 执行失败：%s", entry.Status.Messages)
			job.Stage = "complete_pending"
		} else {
			if job.Stage == "running" {
				if err := a.event(
					ctx,
					job,
					"collecting",
					"collecting",
					map[string]string{"prompt_id": job.PromptID},
				); err != nil {
					return err
				}
			}
			if len(job.Outputs) == 0 {
				if err := a.collect(ctx, comfy, &task, job, entry); err != nil {
					return err
				}
			}
			job.CompletionStatus = "succeeded"
			job.Stage = "complete_pending"
		}
		if err := a.store.Save(ctx, job); err != nil {
			return err
		}
	}
	if job.Stage != "complete_pending" {
		return fmt.Errorf("未知执行阶段 %q", job.Stage)
	}
	return a.confirm(ctx, job)
}

func (a *App) confirm(ctx context.Context, job *journal.Job) error {
	manifest := []map[string]string{}
	for _, output := range job.Outputs {
		if err := a.plane.Upload(ctx, "/agent/v1/executions/"+job.ExecutionID+"/outputs/"+output.OutputID, controlplane.Upload{
			Path: output.Path, AttemptID: job.AttemptID, FieldKey: output.FieldKey, Index: output.Index,
			SHA256: output.SHA256, MediaType: output.MediaType,
		}); err != nil {
			return err
		}
		manifest = append(manifest, map[string]string{"output_id": output.OutputID, "sha256": output.SHA256})
	}
	if _, err := a.plane.JSON(
		ctx,
		http.MethodPost,
		"/agent/v1/executions/"+job.ExecutionID+"/complete",
		map[string]any{
			"attempt_id": job.AttemptID, "status": job.CompletionStatus, "error": job.Error, "outputs": manifest,
		},
		nil,
	); err != nil {
		return err
	}
	job.Stage = "done"
	if err := a.store.Save(ctx, job); err != nil {
		return err
	}
	slog.Info("已确认执行结果", "execution_id", job.ExecutionID, "status", job.CompletionStatus, "outputs", len(job.Outputs))
	return nil
}

func (a *App) prepare(ctx context.Context, comfy *comfyui.Client, task *taskPackage) error {
	for _, input := range task.InputFiles {
		data, err := a.plane.Download(ctx, input.URL)
		if err != nil {
			return err
		}
		if int64(len(data)) != input.SizeBytes || fmt.Sprintf("%x", sha256.Sum256(data)) != input.SHA256 {
			return errors.New("输入文件长度或摘要校验失败")
		}
		name, err := comfy.UploadImage(ctx, input.FileID, data)
		if err != nil {
			return err
		}
		value, err := json.Marshal(name)
		if err != nil {
			return fmt.Errorf("编码输入文件位置：%w", err)
		}
		node, exists := task.Workflow[input.NodeID]
		if !exists {
			return errors.New("输入绑定节点缺失")
		}
		node.Inputs[input.FieldPath] = value
	}
	return nil
}

func (a *App) waitAvailable(ctx context.Context, comfy *comfyui.Client, job *journal.Job) error {
	ticker := time.NewTicker(time.Second)
	defer ticker.Stop()
	for {
		cancelled, err := a.heartbeat(ctx, &controlplane.Claim{ExecutionID: job.ExecutionID, AttemptID: job.AttemptID})
		if err != nil {
			return err
		}
		if cancelled {
			return errCancelled
		}
		var queue comfyui.Queue
		if err := comfy.Get(ctx, "/queue", &queue); err != nil {
			return err
		}
		if len(queue.Running) == 0 && len(queue.Pending) == 0 {
			return nil
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-ticker.C:
		}
	}
}

func (a *App) waitHistory(ctx context.Context, comfy *comfyui.Client, job *journal.Job) (comfyui.HistoryEntry, error) {
	ticker := time.NewTicker(time.Second)
	defer ticker.Stop()
	cancellationIssued := false
	for {
		history := map[string]comfyui.HistoryEntry{}
		if err := comfy.Get(ctx, "/history/"+url.PathEscape(job.PromptID), &history); err != nil {
			return comfyui.HistoryEntry{}, err
		}
		if entry, exists := history[job.PromptID]; exists && (entry.Status.Completed || entry.Status.StatusStr == "error") {
			cancelled, err := a.heartbeat(ctx, &controlplane.Claim{ExecutionID: job.ExecutionID, AttemptID: job.AttemptID})
			if err != nil {
				return comfyui.HistoryEntry{}, err
			}
			if cancelled && entry.Status.StatusStr == "error" {
				for _, message := range entry.Status.Messages {
					var event []json.RawMessage
					if err := json.Unmarshal(message, &event); err != nil {
						return comfyui.HistoryEntry{}, fmt.Errorf("读取 ComfyUI 结束事件：%w", err)
					}
					if len(event) > 0 && string(event[0]) == `"execution_interrupted"` {
						return comfyui.HistoryEntry{}, errCancelled
					}
				}
			}
			return entry, nil
		}
		var queue comfyui.Queue
		if err := comfy.Get(ctx, "/queue", &queue); err != nil {
			return comfyui.HistoryEntry{}, err
		}
		present, err := queue.Contains(job.PromptID)
		if err != nil {
			return comfyui.HistoryEntry{}, err
		}
		if !present {
			if err := comfy.Get(ctx, "/history/"+url.PathEscape(job.PromptID), &history); err != nil {
				return comfyui.HistoryEntry{}, err
			}
			if _, exists := history[job.PromptID]; !exists {
				if cancellationIssued {
					return comfyui.HistoryEntry{}, errCancelled
				}
				return comfyui.HistoryEntry{}, errResultUnknown
			}
		}
		cancelled, err := a.heartbeat(ctx, &controlplane.Claim{ExecutionID: job.ExecutionID, AttemptID: job.AttemptID})
		if err != nil {
			return comfyui.HistoryEntry{}, err
		}
		if cancelled && !cancellationIssued {
			cancellationIssued, err = comfy.Cancel(ctx, job.PromptID)
			if err != nil {
				return comfyui.HistoryEntry{}, err
			}
		}
		select {
		case <-ctx.Done():
			return comfyui.HistoryEntry{}, ctx.Err()
		case <-ticker.C:
		}
	}
}

func (a *App) collect(
	ctx context.Context,
	comfy *comfyui.Client,
	task *taskPackage,
	job *journal.Job,
	entry comfyui.HistoryEntry,
) error {
	directory := filepath.Join(a.config.DataDir, "outputs", job.ExecutionID)
	if err := os.MkdirAll(directory, 0o700); err != nil {
		return fmt.Errorf("创建输出目录：%w", err)
	}
	outputs := []journal.Output{}
	for index, binding := range task.Outputs {
		node, exists := entry.Outputs[binding.NodeID]
		if !exists {
			return fmt.Errorf("输出节点 %q 缺少结果", binding.NodeID)
		}
		files := append(node.Images, node.Audio...)
		files = append(files, node.Gifs...)
		var data []byte
		var mediaType string
		if len(files) > binding.Index && binding.Index >= 0 {
			var err error
			data, mediaType, err = comfy.Download(ctx, files[binding.Index])
			if err != nil {
				return err
			}
		} else if len(node.Text) > binding.Index && binding.Index >= 0 {
			data = []byte(node.Text[binding.Index])
			mediaType = "text/plain"
		} else {
			return fmt.Errorf("输出 %q 的位置 %d 无效", binding.Key, binding.Index)
		}
		outputID := fmt.Sprintf("output-%d-0", index)
		path := filepath.Join(directory, outputID)
		if err := writeFile(path, data); err != nil {
			return err
		}
		outputs = append(outputs, journal.Output{
			OutputID: outputID, FieldKey: binding.Key, Index: 0, Path: path,
			SHA256: fmt.Sprintf("%x", sha256.Sum256(data)), MediaType: mediaType,
		})
	}
	job.Outputs = outputs
	return a.store.Save(ctx, job)
}

func writeFile(path string, data []byte) error {
	file, err := os.OpenFile(path+".pending", os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o600)
	if errors.Is(err, os.ErrExist) {
		if err := os.Remove(path + ".pending"); err != nil {
			return fmt.Errorf("清理中断输出：%w", err)
		}
		file, err = os.OpenFile(path+".pending", os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o600)
	}
	if err != nil {
		return fmt.Errorf("创建结果文件：%w", err)
	}
	if _, err := file.Write(data); err != nil {
		return errors.Join(fmt.Errorf("写入结果文件：%w", err), file.Close())
	}
	if err := file.Sync(); err != nil {
		return errors.Join(fmt.Errorf("保存结果文件：%w", err), file.Close())
	}
	if err := file.Close(); err != nil {
		return fmt.Errorf("关闭结果文件：%w", err)
	}
	if err := os.Rename(path+".pending", path); err != nil {
		return fmt.Errorf("确认结果文件：%w", err)
	}
	return nil
}
