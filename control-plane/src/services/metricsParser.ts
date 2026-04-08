import type { MetricPoint } from '../types/monitor'

const METRICS_ALLOWLIST = [
  'stormclaw_up',
  'stormclaw_uptime_seconds',
  'stormclaw_channels_total',
  'stormclaw_cron_jobs_total',
  'stormclaw_cron_jobs_enabled',
]

export function parsePrometheusMetrics(input: string): MetricPoint[] {
  const metrics: MetricPoint[] = []
  const lines = input.split('\n')

  for (const line of lines) {
    const trimmed = line.trim()
    if (!trimmed || trimmed.startsWith('#')) {
      continue
    }
    const [rawName, rawValue] = trimmed.split(/\s+/, 2)
    if (!rawName || !rawValue) {
      continue
    }
    if (!METRICS_ALLOWLIST.includes(rawName)) {
      continue
    }
    const value = Number.parseFloat(rawValue)
    if (Number.isNaN(value)) {
      continue
    }
    metrics.push({ name: rawName, value })
  }

  return metrics
}
