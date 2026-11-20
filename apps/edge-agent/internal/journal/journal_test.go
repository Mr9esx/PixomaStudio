package journal

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	_ "modernc.org/sqlite"
)

func TestJournalRetainsSubmissionAndExcludesAnotherProcess(t *testing.T) {
	ctx := context.Background()
	if path := os.Getenv("PIXOMA_AGENT_LOCK_CHECK"); path != "" {
		other, err := Open(ctx, path)
		if err == nil {
			if err := other.Close(); err != nil {
				t.Fatal(err)
			}
			t.Fatal("相同目录允许第二个 Agent 启动")
		}
		return
	}
	path := filepath.Join(t.TempDir(), "agent.sqlite")
	store, err := Open(ctx, path)
	if err != nil {
		t.Fatal(err)
	}
	instance := store.InstanceID
	executable, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	command := exec.CommandContext(ctx, executable, "-test.run=^TestJournalRetainsSubmissionAndExcludesAnotherProcess$")
	command.Env = append(os.Environ(), "PIXOMA_AGENT_LOCK_CHECK="+path)
	if output, err := command.CombinedOutput(); err != nil {
		t.Fatalf("目录锁检查失败：%v\n%s", err, output)
	}
	job := Job{ExecutionID: "execution-one", AttemptID: "attempt-one", Stage: "submitting", Sequence: 2}
	if err := store.Save(ctx, &job); err != nil {
		t.Fatal(err)
	}
	job.PromptID = "comfy-prompt-one"
	job.Stage = "running"
	if err := store.Save(ctx, &job); err != nil {
		t.Fatal(err)
	}
	if err := store.Close(); err != nil {
		t.Fatal(err)
	}
	reopened, err := Open(ctx, path)
	if err != nil {
		t.Fatal(err)
	}
	defer reopened.Close()
	if reopened.InstanceID != instance {
		t.Fatal("持久实例编号发生变化")
	}
	resumed, err := reopened.Active(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if resumed == nil || resumed.PromptID != job.PromptID || resumed.Sequence != 2 {
		t.Fatal("原提交编号或事件序号未保存")
	}
}
