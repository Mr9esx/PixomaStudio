package application

import (
	"context"
	"testing"
	"time"
)

func TestSamplerReadsActualHost(t *testing.T) {
	s := &sampler{}
	hardware, first := s.sample(context.Background())
	if hardware["cpu_cores"] == nil || hardware["ram_bytes"] == nil {
		t.Fatalf("设备信息缺失：%v", hardware)
	}
	if first["cpu_usage_percent"] == nil || first["mem_total_bytes"] == nil {
		t.Fatalf("真实采集数据缺失：%v", first)
	}
	if first["disk_read_bytes_per_sec"] != nil {
		t.Fatal("首次采集不能计算磁盘速率")
	}
	time.Sleep(20 * time.Millisecond)
	_, second := s.sample(context.Background())
	if second["collected_at"] == first["collected_at"] {
		t.Fatal("采集时间没有更新")
	}
	if second["mem_used_bytes"].(uint64) > second["mem_total_bytes"].(uint64) {
		t.Fatal("已用内存超过总内存")
	}
}
