import type { EdgeMetrics } from '@/lib/api/types'

export function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value < 0) return '—'
  if (value === 0) return '0 B'
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']
  const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1)
  const scaled = value / 1024 ** index
  return `${index === 0 ? Math.round(scaled) : scaled >= 100 ? scaled.toFixed(0) : scaled.toFixed(1)} ${units[index]}`
}

export function formatFullTime(milliseconds: number): string {
  if (!Number.isFinite(milliseconds)) return '—'
  const date = new Date(milliseconds)
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`
}

export function ioAxisTicks(values: number[]) {
  const maximum = values.length ? Math.max(...values) : 0
  if (maximum <= 0) return { ticks: [0], format: () => '0 B' }
  const unit = maximum >= 4 * 1024 ** 3 ? 1024 ** 3 : maximum >= 4 * 1024 ** 2 ? 1024 ** 2 : maximum >= 4 * 1024 ? 1024 : 1
  const label = unit === 1024 ** 3 ? 'GiB' : unit === 1024 ** 2 ? 'MiB' : unit === 1024 ? 'KiB' : 'B'
  const raw = maximum / unit / 4
  const magnitude = 10 ** Math.floor(Math.log10(raw))
  const normalized = raw / magnitude
  const step = (normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10) * magnitude
  const ticks: number[] = []
  for (let value = 0; value <= maximum / unit + step && ticks.length < 7; value += step) ticks.push(Math.round(value * unit))
  return { ticks, format: (value: number) => `${Math.round(value / unit)} ${label}` }
}

export function seriesStats(values: Array<number | null | undefined>, format: (value: number) => string) {
  const numbers = values.filter((value): value is number => typeof value === 'number' && Number.isFinite(value))
  if (!numbers.length) return { current: '—', max: '—', avg: '—' }
  return { current: format(numbers[numbers.length - 1]), max: format(Math.max(...numbers)), avg: format(numbers.reduce((sum, value) => sum + value, 0) / numbers.length) }
}

export function formatRuntime(milliseconds: number): string {
  if (milliseconds < 1000) return `${milliseconds} ms`
  if (milliseconds < 60000) return `${(milliseconds / 1000).toFixed(1)} s`
  if (milliseconds < 3600000) return `${(milliseconds / 60000).toFixed(1)} m`
  return `${(milliseconds / 3600000).toFixed(1)} h`
}
export function metricDevices(series: EdgeMetrics[]) {
  const devices = new Map<string, { id: string; name: string }>()
  for (const point of series) for (const [index, gpu] of (point.gpus ?? []).entries()) {
    const id = gpu.id ?? String(index)
    devices.set(id, { id, name: gpu.name })
  }
  return [...devices.values()]
}

