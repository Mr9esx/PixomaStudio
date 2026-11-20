package controlplane

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"strconv"
	"strings"
)

type Client struct {
	BaseURL string
	Token   string
	HTTP    *http.Client
}

type Claim struct {
	ProtocolVersion int    `json:"protocol_version"`
	ExecutionID     string `json:"execution_id"`
	AttemptID       string `json:"attempt_id"`
	PackageURL      string `json:"package_url"`
}

func (c *Client) JSON(ctx context.Context, method, path string, input, output any) (int, error) {
	var body io.Reader
	if input != nil {
		data, err := json.Marshal(input)
		if err != nil {
			return 0, fmt.Errorf("编码 Studio 请求：%w", err)
		}
		body = bytes.NewReader(data)
	}
	request, err := c.request(
		ctx,
		method,
		path,
		body,
	)
	if err != nil {
		return 0, err
	}
	request.Header.Set("Content-Type", "application/json")
	response, err := c.HTTP.Do(request)
	if err != nil {
		return 0, fmt.Errorf("访问 Studio：%w", err)
	}
	defer response.Body.Close()
	if err := check(response); err != nil {
		return response.StatusCode, err
	}
	if output != nil && response.StatusCode != http.StatusNoContent {
		if err := json.NewDecoder(io.LimitReader(response.Body, 32<<20)).Decode(output); err != nil {
			return response.StatusCode, fmt.Errorf("读取 Studio 响应：%w", err)
		}
	}
	return response.StatusCode, nil
}

func (c *Client) Download(ctx context.Context, path string) ([]byte, error) {
	request, err := c.request(
		ctx,
		http.MethodGet,
		path,
		nil,
	)
	if err != nil {
		return nil, err
	}
	response, err := c.HTTP.Do(request)
	if err != nil {
		return nil, fmt.Errorf("读取输入文件：%w", err)
	}
	defer response.Body.Close()
	if err := check(response); err != nil {
		return nil, err
	}
	data, err := io.ReadAll(io.LimitReader(response.Body, (256<<20)+1))
	if err != nil {
		return nil, fmt.Errorf("下载输入文件：%w", err)
	}
	if len(data) > 256<<20 {
		return nil, errors.New("输入文件超过 256 MiB")
	}
	return data, nil
}

type Upload struct {
	Path      string
	AttemptID string
	FieldKey  string
	Index     int
	SHA256    string
	MediaType string
}

func (c *Client) Upload(ctx context.Context, path string, upload Upload) error {
	file, err := os.Open(upload.Path)
	if err != nil {
		return fmt.Errorf("打开结果文件：%w", err)
	}
	defer file.Close()
	info, err := file.Stat()
	if err != nil {
		return fmt.Errorf("检查结果文件：%w", err)
	}
	request, err := c.request(
		ctx,
		http.MethodPut,
		path,
		file,
	)
	if err != nil {
		return err
	}
	request.ContentLength = info.Size()
	request.Header.Set("Content-Type", upload.MediaType)
	request.Header.Set("X-Attempt-Id", upload.AttemptID)
	request.Header.Set("X-Output-Field", upload.FieldKey)
	request.Header.Set("X-Output-Index", strconv.Itoa(upload.Index))
	request.Header.Set("X-Content-Sha256", upload.SHA256)
	response, err := c.HTTP.Do(request)
	if err != nil {
		return fmt.Errorf("上传执行结果：%w", err)
	}
	defer response.Body.Close()
	return check(response)
}

func (c *Client) request(ctx context.Context, method, path string, body io.Reader) (*http.Request, error) {
	if !strings.HasPrefix(path, "/agent/v1/") || strings.Contains(path, "..") {
		return nil, errors.New("Studio 节点接口路径无效")
	}
	request, err := http.NewRequestWithContext(
		ctx,
		method,
		strings.TrimRight(c.BaseURL, "/")+path,
		body,
	)
	if err != nil {
		return nil, fmt.Errorf("构造 Studio 请求：%w", err)
	}
	request.Header.Set("Authorization", "Bearer "+c.Token)
	return request, nil
}

func check(response *http.Response) error {
	if response.StatusCode < 300 {
		return nil
	}
	message, err := io.ReadAll(io.LimitReader(response.Body, 8192))
	if err != nil {
		return fmt.Errorf("读取 Studio 错误：%w", err)
	}
	return fmt.Errorf("Studio 返回 %d：%s", response.StatusCode, message)
}
