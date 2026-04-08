export interface ChannelStatus {
  enabled: boolean
  running: boolean
}

export interface CronServiceStatus {
  enabled: boolean
  total_jobs: number
  enabled_jobs: number
  next_wake_at_ms: number | null
}

export interface HeartbeatStatus {
  enabled: boolean
  last_check_at: string | null
  last_action_at: string | null
  checks_performed: number
  actions_taken: number
}

export interface GatewayStatus {
  running: boolean
  uptime_seconds: number
  channels: Record<string, ChannelStatus>
  cron: CronServiceStatus
  heartbeat: HeartbeatStatus
}

export interface CronJobState {
  nextRunAtMs?: number
  lastRunAtMs?: number
  lastStatus?: string
  lastError?: string
}

export interface CronSchedule {
  kind: string
  atMs?: number
  everyMs?: number
  expr?: string
  tz?: string
}

export interface CronPayload {
  kind: string
  message: string
  deliver: boolean
  channel?: string
  to?: string
}

export interface CronJob {
  id: string
  name: string
  enabled: boolean
  schedule: CronSchedule
  payload: CronPayload
  state: CronJobState
  createdAtMs: number
  updatedAtMs: number
  deleteAfterRun: boolean
}

export interface MetricPoint {
  name: string
  value: number
}
