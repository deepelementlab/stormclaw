import type { CronJob, GatewayStatus } from '../types/monitor'

const baseUrl = (import.meta.env.VITE_GATEWAY_BASE_URL as string | undefined) ?? ''

async function parseResponse<T>(response: Response): Promise<T> {
  if (!response.ok) {
    const text = await response.text()
    throw new Error(text || `HTTP ${response.status}`)
  }
  return (await response.json()) as T
}

export async function fetchHealth(): Promise<string> {
  const response = await fetch(`${baseUrl}/health`)
  if (!response.ok) {
    throw new Error(`Health check failed: ${response.status}`)
  }
  return response.text()
}

export async function fetchStatus(): Promise<GatewayStatus> {
  const response = await fetch(`${baseUrl}/status`)
  return parseResponse<GatewayStatus>(response)
}

export async function fetchChannels(): Promise<GatewayStatus['channels']> {
  const response = await fetch(`${baseUrl}/channels`)
  return parseResponse<GatewayStatus['channels']>(response)
}

export async function fetchCronJobs(): Promise<CronJob[]> {
  const response = await fetch(`${baseUrl}/cron/jobs`)
  return parseResponse<CronJob[]>(response)
}

export async function fetchMetricsRaw(): Promise<string> {
  const response = await fetch(`${baseUrl}/metrics`)
  if (!response.ok) {
    throw new Error(`Metrics fetch failed: ${response.status}`)
  }
  return response.text()
}

export async function triggerHeartbeat(): Promise<string> {
  const response = await fetch(`${baseUrl}/heartbeat/trigger`, { method: 'POST' })
  if (!response.ok) {
    const text = await response.text()
    throw new Error(text || `Heartbeat trigger failed: ${response.status}`)
  }
  return response.text()
}
