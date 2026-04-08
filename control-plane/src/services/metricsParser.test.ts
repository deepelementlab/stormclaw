import { describe, expect, it } from 'vitest'
import { parsePrometheusMetrics } from './metricsParser'

describe('parsePrometheusMetrics', () => {
  it('keeps only selected stormclaw metrics', () => {
    const input = `
# HELP stormclaw_up Whether the gateway is running
stormclaw_up 1
stormclaw_uptime_seconds 123
custom_metric 99
stormclaw_cron_jobs_total 4
`
    const result = parsePrometheusMetrics(input)
    expect(result).toEqual([
      { name: 'stormclaw_up', value: 1 },
      { name: 'stormclaw_uptime_seconds', value: 123 },
      { name: 'stormclaw_cron_jobs_total', value: 4 },
    ])
  })
})
