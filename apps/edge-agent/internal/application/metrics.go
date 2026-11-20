package application

import (
	"context"
	"encoding/csv"
	"errors"
	"fmt"
	"os/exec"
	"strconv"
	"strings"
	"time"

	"github.com/shirou/gopsutil/v4/cpu"
	"github.com/shirou/gopsutil/v4/disk"
	"github.com/shirou/gopsutil/v4/mem"
)

type sampler struct {
	lastDisk map[string]disk.IOCountersStat
	lastTime time.Time
}

func (s *sampler) sample(ctx context.Context) (map[string]any, map[string]any) {
	now := time.Now().UTC()
	hardware := map[string]any{"collected_at": now}
	metrics := map[string]any{"collected_at": now}
	failures := []string{}
	if info, err := cpu.InfoWithContext(ctx); err != nil {
		failures = append(failures, fmt.Sprintf("读取 CPU 信息：%v", err))
	} else if len(info) > 0 {
		hardware["cpu_model"] = info[0].ModelName
	}
	if count, err := cpu.CountsWithContext(ctx, false); err != nil {
		failures = append(failures, fmt.Sprintf("读取 CPU 核心：%v", err))
	} else {
		hardware["cpu_cores"] = count
	}
	if values, err := cpu.PercentWithContext(ctx, 0, false); err != nil {
		failures = append(failures, fmt.Sprintf("读取 CPU 使用率：%v", err))
	} else if len(values) > 0 {
		metrics["cpu_usage_percent"] = values[0]
	}
	if memory, err := mem.VirtualMemoryWithContext(ctx); err != nil {
		failures = append(failures, fmt.Sprintf("读取内存：%v", err))
	} else {
		hardware["ram_bytes"] = memory.Total
		metrics["mem_total_bytes"] = memory.Total
		metrics["mem_used_bytes"] = memory.Used
		metrics["mem_usage_percent"] = memory.UsedPercent
	}
	if counters, err := disk.IOCountersWithContext(ctx); err != nil {
		failures = append(failures, fmt.Sprintf("读取磁盘 IO：%v", err))
	} else {
		if !s.lastTime.IsZero() {
			var read, write uint64
			for name, counter := range counters {
				previous, exists := s.lastDisk[name]
				if !exists || counter.ReadBytes < previous.ReadBytes || counter.WriteBytes < previous.WriteBytes {
					continue
				}
				read += counter.ReadBytes - previous.ReadBytes
				write += counter.WriteBytes - previous.WriteBytes
			}
			seconds := now.Sub(s.lastTime).Seconds()
			metrics["disk_read_bytes_per_sec"] = float64(read) / seconds
			metrics["disk_write_bytes_per_sec"] = float64(write) / seconds
		}
		s.lastDisk = counters
		s.lastTime = now
	}
	if path, err := exec.LookPath("nvidia-smi"); err == nil {
		gpus, err := sampleNvidia(ctx, path)
		if err != nil {
			failures = append(failures, fmt.Sprintf("读取 NVIDIA GPU：%v", err))
		} else {
			metrics["gpus"] = gpus
		}
	} else if !errors.Is(err, exec.ErrNotFound) {
		failures = append(failures, fmt.Sprintf("查找 nvidia-smi：%v", err))
	}
	metrics["collection_errors"] = failures
	return hardware, metrics
}

func sampleNvidia(ctx context.Context, path string) ([]map[string]any, error) {
	output, err := exec.CommandContext(
		ctx,
		path,
		"--query-gpu=index,name,utilization.gpu,memory.used,memory.total",
		"--format=csv,noheader,nounits",
	).Output()
	if err != nil {
		return nil, fmt.Errorf("执行 nvidia-smi：%w", err)
	}
	reader := csv.NewReader(strings.NewReader(string(output)))
	reader.TrimLeadingSpace = true
	reader.FieldsPerRecord = 5
	rows, err := reader.ReadAll()
	if err != nil {
		return nil, fmt.Errorf("读取 GPU CSV：%w", err)
	}
	gpus := []map[string]any{}
	for _, row := range rows {
		gpu := map[string]any{"id": strings.TrimSpace(row[0]), "name": strings.TrimSpace(row[1])}
		usage, err := strconv.ParseFloat(strings.TrimSpace(row[2]), 64)
		if err != nil {
			return nil, fmt.Errorf("读取 GPU 使用率：%w", err)
		}
		used, err := strconv.ParseUint(strings.TrimSpace(row[3]), 10, 64)
		if err != nil {
			return nil, fmt.Errorf("读取已用显存：%w", err)
		}
		total, err := strconv.ParseUint(strings.TrimSpace(row[4]), 10, 64)
		if err != nil || total == 0 {
			return nil, fmt.Errorf("GPU 总显存无效：%q", row[4])
		}
		gpu["usage_percent"] = usage
		gpu["vram_used_bytes"] = used << 20
		gpu["vram_total_bytes"] = total << 20
		gpu["vram_usage_percent"] = float64(used) / float64(total) * 100
		gpus = append(gpus, gpu)
	}
	return gpus, nil
}
