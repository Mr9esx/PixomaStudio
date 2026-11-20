package journal

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/google/uuid"
)

type Output struct {
	OutputID  string `json:"output_id"`
	FieldKey  string `json:"field_key"`
	Index     int    `json:"index"`
	SHA256    string `json:"sha256"`
	MediaType string `json:"media_type"`
	Path      string `json:"path"`
}

type Job struct {
	ExecutionID      string          `json:"execution_id"`
	AttemptID        string          `json:"attempt_id"`
	Stage            string          `json:"stage"`
	Sequence         int64           `json:"sequence"`
	PromptID         string          `json:"prompt_id"`
	Package          json.RawMessage `json:"package"`
	Pending          json.RawMessage `json:"pending,omitempty"`
	PendingStage     string          `json:"pending_stage,omitempty"`
	CompletionStatus string          `json:"completion_status,omitempty"`
	Error            string          `json:"error,omitempty"`
	Outputs          []Output        `json:"outputs"`
}

type Store struct {
	db         *sql.DB
	InstanceID string
}

func Open(ctx context.Context, path string) (*Store, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return nil, fmt.Errorf("创建 Agent 目录：%w", err)
	}
	info, err := os.Lstat(path)
	if err != nil && !errors.Is(err, os.ErrNotExist) {
		return nil, fmt.Errorf("检查日志文件：%w", err)
	}
	if info != nil && info.Mode()&os.ModeSymlink != 0 {
		return nil, errors.New("执行日志不能使用符号链接")
	}
	db, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, fmt.Errorf("打开执行日志：%w", err)
	}
	db.SetMaxOpenConns(1)
	db.SetMaxIdleConns(1)
	store := &Store{db: db}
	if err := store.initialize(ctx); err != nil {
		return nil, errors.Join(err, db.Close())
	}
	if err := os.Chmod(path, 0o600); err != nil {
		return nil, errors.Join(fmt.Errorf("设置日志权限：%w", err), db.Close())
	}
	return store, nil
}

func (s *Store) initialize(ctx context.Context) error {
	statements := []string{
		"PRAGMA busy_timeout=0",
		"PRAGMA journal_mode=DELETE",
		"PRAGMA synchronous=FULL",
		"PRAGMA locking_mode=EXCLUSIVE",
		"CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL)",
		"CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY,document TEXT NOT NULL CHECK(json_valid(document)))",
		"CREATE UNIQUE INDEX IF NOT EXISTS one_active_job ON jobs((1)) WHERE json_extract(document,'$.stage')!='done'",
	}
	for _, statement := range statements {
		if _, err := s.db.ExecContext(ctx, statement); err != nil {
			return fmt.Errorf("初始化日志或获取目录锁：%w", err)
		}
	}
	if _, err := s.db.ExecContext(ctx,
		"INSERT INTO metadata(key,value) VALUES('instance_id',?) ON CONFLICT(key) DO NOTHING", uuid.NewString(),
	); err != nil {
		return fmt.Errorf("保存实例编号：%w", err)
	}
	row := s.db.QueryRowContext(ctx, "SELECT value FROM metadata WHERE key='instance_id'")
	if err := row.Scan(&s.InstanceID); err != nil {
		return fmt.Errorf("读取实例编号：%w", err)
	}
	return nil
}

func (s *Store) Save(ctx context.Context, job *Job) error {
	if job.ExecutionID == "" || job.AttemptID == "" || job.Stage == "" {
		return errors.New("执行日志缺少必要字段")
	}
	if job.Outputs == nil {
		job.Outputs = []Output{}
	}
	document, err := json.Marshal(job)
	if err != nil {
		return fmt.Errorf("编码执行日志：%w", err)
	}
	if _, err := s.db.ExecContext(
		ctx,
		"INSERT INTO jobs(id,document) VALUES(?,?) ON CONFLICT(id) DO UPDATE SET document=excluded.document",
		job.ExecutionID,
		document,
	); err != nil {
		return fmt.Errorf("保存执行日志：%w", err)
	}
	return nil
}

func (s *Store) Active(ctx context.Context) (*Job, error) {
	var document []byte
	row := s.db.QueryRowContext(ctx, "SELECT document FROM jobs WHERE json_extract(document,'$.stage')!='done'")
	err := row.Scan(&document)
	if errors.Is(err, sql.ErrNoRows) {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("读取未结束执行：%w", err)
	}
	var job Job
	if err := json.Unmarshal(document, &job); err != nil {
		return nil, fmt.Errorf("读取执行内容：%w", err)
	}
	return &job, nil
}

func (s *Store) Close() error {
	return s.db.Close()
}
