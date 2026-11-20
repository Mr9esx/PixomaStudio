package comfyui

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"mime"
	"mime/multipart"
	"net/http"
	"net/url"
	"strings"

	"github.com/google/uuid"
)

type Graph map[string]struct {
	ClassType string                     `json:"class_type"`
	Inputs    map[string]json.RawMessage `json:"inputs"`
	Meta      json.RawMessage            `json:"_meta,omitempty"`
}

type File struct {
	Filename  string `json:"filename"`
	Subfolder string `json:"subfolder"`
	Type      string `json:"type"`
}

type NodeOutput struct {
	Images []File   `json:"images"`
	Audio  []File   `json:"audio"`
	Gifs   []File   `json:"gifs"`
	Text   []string `json:"text"`
}

type HistoryEntry struct {
	Prompt  []json.RawMessage     `json:"prompt"`
	Outputs map[string]NodeOutput `json:"outputs"`
	Status  struct {
		StatusStr string            `json:"status_str"`
		Completed bool              `json:"completed"`
		Messages  []json.RawMessage `json:"messages"`
	} `json:"status"`
}

type Queue struct {
	Running [][]json.RawMessage `json:"queue_running"`
	Pending [][]json.RawMessage `json:"queue_pending"`
}

func (q Queue) Contains(promptID string) (bool, error) {
	for _, entries := range [][][]json.RawMessage{q.Running, q.Pending} {
		for _, entry := range entries {
			if len(entry) < 2 {
				return false, errors.New("ComfyUI 队列格式无效")
			}
			var id string
			if err := json.Unmarshal(entry[1], &id); err != nil {
				return false, fmt.Errorf("读取 ComfyUI 队列编号：%w", err)
			}
			if id == promptID {
				return true, nil
			}
		}
	}
	return false, nil
}

func (c *Client) Cancel(ctx context.Context, promptID string) (bool, error) {
	if _, err := uuid.Parse(promptID); err != nil {
		return false, fmt.Errorf("ComfyUI 任务编号无效：%w", err)
	}
	response, err := c.request(ctx, http.MethodPost, "/api/jobs/"+url.PathEscape(promptID)+"/cancel", nil, "application/json")
	if err != nil {
		return false, err
	}
	defer response.Body.Close()
	var result struct {
		Cancelled bool `json:"cancelled"`
	}
	if err := json.NewDecoder(response.Body).Decode(&result); err != nil {
		return false, fmt.Errorf("读取 ComfyUI 取消结果：%w", err)
	}
	return result.Cancelled, nil
}

type SystemStats struct {
	System struct {
		RAMTotal     uint64 `json:"ram_total"`
		ComfyVersion string `json:"comfyui_version"`
	} `json:"system"`
	Devices []struct {
		Index     int    `json:"index"`
		Name      string `json:"name"`
		VRAMTotal uint64 `json:"vram_total"`
		VRAMFree  uint64 `json:"vram_free"`
	} `json:"devices"`
}

type Client struct {
	BaseURL string
	HTTP    *http.Client
}

type HTTPError struct {
	StatusCode int
	Path       string
	Message    string
}

func (e *HTTPError) Error() string {
	return fmt.Sprintf(
		"ComfyUI %s 返回 %d：%s",
		e.Path,
		e.StatusCode,
		e.Message,
	)
}

func (c *Client) Get(ctx context.Context, path string, output any) error {
	response, err := c.request(
		ctx,
		http.MethodGet,
		path,
		nil,
		"",
	)
	if err != nil {
		return err
	}
	defer response.Body.Close()
	if err := json.NewDecoder(io.LimitReader(response.Body, 32<<20)).Decode(output); err != nil {
		return fmt.Errorf("读取 ComfyUI %s：%w", path, err)
	}
	return nil
}

func (c *Client) request(
	ctx context.Context,
	method string,
	path string,
	body io.Reader,
	contentType string,
) (*http.Response, error) {
	request, err := http.NewRequestWithContext(
		ctx,
		method,
		strings.TrimRight(c.BaseURL, "/")+path,
		body,
	)
	if err != nil {
		return nil, fmt.Errorf("构造 ComfyUI 请求：%w", err)
	}
	if contentType != "" {
		request.Header.Set("Content-Type", contentType)
	}
	response, err := c.HTTP.Do(request)
	if err != nil {
		return nil, fmt.Errorf("访问 ComfyUI %s：%w", path, err)
	}
	if response.StatusCode >= 300 {
		defer response.Body.Close()
		message, err := io.ReadAll(io.LimitReader(response.Body, 8192))
		if err != nil {
			return nil, fmt.Errorf("读取 ComfyUI 错误：%w", err)
		}
		return nil, &HTTPError{StatusCode: response.StatusCode, Path: path, Message: string(message)}
	}
	return response, nil
}

func (c *Client) Submit(ctx context.Context, graph Graph, executionID, attemptID string) (string, error) {
	body, err := json.Marshal(map[string]any{
		"prompt": graph, "client_id": attemptID,
		"extra_data": map[string]string{"studio_execution_id": executionID, "studio_attempt_id": attemptID},
	})
	if err != nil {
		return "", fmt.Errorf("编码 ComfyUI 工作流：%w", err)
	}
	response, err := c.request(
		ctx,
		http.MethodPost,
		"/prompt",
		bytes.NewReader(body),
		"application/json",
	)
	if err != nil {
		return "", err
	}
	defer response.Body.Close()
	var result struct {
		PromptID string `json:"prompt_id"`
	}
	if err := json.NewDecoder(response.Body).Decode(&result); err != nil {
		return "", fmt.Errorf("读取提交编号：%w", err)
	}
	if result.PromptID == "" {
		return "", errors.New("ComfyUI 未返回 prompt_id")
	}
	return result.PromptID, nil
}

func (c *Client) FindSubmission(ctx context.Context, executionID, attemptID string) (string, error) {
	var queue Queue
	if err := c.Get(ctx, "/queue", &queue); err != nil {
		return "", err
	}
	history := map[string]HistoryEntry{}
	if err := c.Get(ctx, "/history", &history); err != nil {
		return "", err
	}
	candidates := append(queue.Running, queue.Pending...)
	for _, entry := range history {
		candidates = append(candidates, entry.Prompt)
	}
	found := map[string]bool{}
	for _, candidate := range candidates {
		if len(candidate) < 4 {
			return "", errors.New("ComfyUI 历史或队列格式无效")
		}
		var extra struct {
			ExecutionID string `json:"studio_execution_id"`
			AttemptID   string `json:"studio_attempt_id"`
		}
		if err := json.Unmarshal(candidate[3], &extra); err != nil {
			return "", fmt.Errorf("读取提交关联字段：%w", err)
		}
		if extra.ExecutionID != executionID || extra.AttemptID != attemptID {
			continue
		}
		var promptID string
		if err := json.Unmarshal(candidate[1], &promptID); err != nil {
			return "", fmt.Errorf("读取历史编号：%w", err)
		}
		found[promptID] = true
	}
	if len(found) != 1 {
		return "", fmt.Errorf("提交结果无法唯一确认，匹配数量 %d", len(found))
	}
	for id := range found {
		return id, nil
	}
	return "", errors.New("ComfyUI 提交编号缺失")
}

func (c *Client) UploadImage(ctx context.Context, filename string, data []byte) (string, error) {
	var body bytes.Buffer
	writer := multipart.NewWriter(&body)
	part, err := writer.CreateFormFile("image", filename)
	if err != nil {
		return "", fmt.Errorf("创建输入文件表单：%w", err)
	}
	if _, err := part.Write(data); err != nil {
		return "", fmt.Errorf("写入输入文件：%w", err)
	}
	if err := writer.Close(); err != nil {
		return "", fmt.Errorf("结束输入文件表单：%w", err)
	}
	response, err := c.request(
		ctx,
		http.MethodPost,
		"/upload/image",
		&body,
		writer.FormDataContentType(),
	)
	if err != nil {
		return "", err
	}
	defer response.Body.Close()
	var result struct {
		Name      string `json:"name"`
		Subfolder string `json:"subfolder"`
	}
	if err := json.NewDecoder(response.Body).Decode(&result); err != nil {
		return "", fmt.Errorf("读取 ComfyUI 输入位置：%w", err)
	}
	if result.Name == "" {
		return "", errors.New("ComfyUI 未返回输入文件名称")
	}
	if result.Subfolder != "" {
		return result.Subfolder + "/" + result.Name, nil
	}
	return result.Name, nil
}

func (c *Client) Download(ctx context.Context, file File) ([]byte, string, error) {
	query := url.Values{"filename": {file.Filename}, "subfolder": {file.Subfolder}, "type": {file.Type}}
	response, err := c.request(
		ctx,
		http.MethodGet,
		"/view?"+query.Encode(),
		nil,
		"",
	)
	if err != nil {
		return nil, "", err
	}
	defer response.Body.Close()
	data, err := io.ReadAll(io.LimitReader(response.Body, (256<<20)+1))
	if err != nil {
		return nil, "", fmt.Errorf("下载 ComfyUI 输出：%w", err)
	}
	if len(data) > 256<<20 {
		return nil, "", errors.New("ComfyUI 输出超过 256 MiB")
	}
	mediaType, _, err := mime.ParseMediaType(response.Header.Get("Content-Type"))
	if err != nil {
		return nil, "", fmt.Errorf("读取输出媒体类型：%w", err)
	}
	return data, mediaType, nil
}
