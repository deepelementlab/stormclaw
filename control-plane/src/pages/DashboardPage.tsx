import { useMutation, useQuery } from '@tanstack/react-query'
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { StatCard } from '../components/StatCard'
import { useRealtimeOrPolling } from '../hooks/useRealtimeOrPolling'
import { fetchHealth, fetchMetricsRaw, fetchStatus, triggerHeartbeat } from '../services/apiClient'
import { parsePrometheusMetrics } from '../services/metricsParser'
import { formatDateTime, formatDuration } from '../utils/time'

export function DashboardPage() {
  useRealtimeOrPolling([['health'], ['status'], ['metrics']])

  const healthQuery = useQuery({
    queryKey: ['health'],
    queryFn: fetchHealth,
  })
  const statusQuery = useQuery({
    queryKey: ['status'],
    queryFn: fetchStatus,
  })
  const metricsQuery = useQuery({
    queryKey: ['metrics'],
    queryFn: fetchMetricsRaw,
    select: parsePrometheusMetrics,
  })
  const heartbeatMutation = useMutation({
    mutationFn: triggerHeartbeat,
  })

  if (statusQuery.isLoading || metricsQuery.isLoading || healthQuery.isLoading) {
    return <p>Loading monitoring data...</p>
  }

  if (statusQuery.isError || metricsQuery.isError || healthQuery.isError) {
    return (
      <section className="panel">
        <h2>Dashboard</h2>
        <p className="error">Failed to load dashboard data. Please check gateway connectivity.</p>
      </section>
    )
  }

  const status = statusQuery.data
  if (!status) {
    return <p>Waiting for status data...</p>
  }
  const metrics = metricsQuery.data ?? []

  return (
    <section className="page-grid">
      <h2>System Dashboard</h2>
      <div className="cards-grid">
        <StatCard title="Gateway Health" value={healthQuery.data} hint="From /health" />
        <StatCard title="Gateway Running" value={status.running ? 'Yes' : 'No'} />
        <StatCard title="Uptime" value={formatDuration(status.uptime_seconds)} />
        <StatCard title="Enabled Cron Jobs" value={status.cron.enabled_jobs} />
      </div>

      <div className="panel">
        <h3>Heartbeat</h3>
        <p>Last check: {formatDateTime(status.heartbeat.last_check_at)}</p>
        <p>Last action: {formatDateTime(status.heartbeat.last_action_at)}</p>
        <button
          onClick={() => heartbeatMutation.mutate()}
          disabled={heartbeatMutation.isPending}
        >
          {heartbeatMutation.isPending ? 'Triggering...' : 'Trigger Heartbeat'}
        </button>
        {heartbeatMutation.isError ? (
          <p className="error">Heartbeat failed: {(heartbeatMutation.error as Error).message}</p>
        ) : null}
        {heartbeatMutation.isSuccess ? <p>{heartbeatMutation.data}</p> : null}
      </div>

      <div className="panel chart-panel">
        <h3>Core Metrics</h3>
        <ResponsiveContainer width="100%" height={280}>
          <BarChart data={metrics}>
            <CartesianGrid strokeDasharray="3 3" />
            <XAxis dataKey="name" />
            <YAxis />
            <Tooltip />
            <Bar dataKey="value" fill="#4f7cff" />
          </BarChart>
        </ResponsiveContainer>
      </div>
    </section>
  )
}
