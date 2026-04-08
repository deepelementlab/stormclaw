import { useQuery } from '@tanstack/react-query'
import { useRealtimeOrPolling } from '../hooks/useRealtimeOrPolling'
import { fetchCronJobs } from '../services/apiClient'
import { formatDateTime } from '../utils/time'

export function CronJobsPage() {
  useRealtimeOrPolling([['cron-jobs']])

  const cronJobsQuery = useQuery({
    queryKey: ['cron-jobs'],
    queryFn: fetchCronJobs,
  })

  if (cronJobsQuery.isLoading) {
    return <p>Loading cron jobs...</p>
  }

  if (cronJobsQuery.isError) {
    return <p className="error">Failed to load cron jobs.</p>
  }

  const jobs = cronJobsQuery.data ?? []

  return (
    <section className="page-grid">
      <h2>Cron Jobs</h2>
      <div className="panel">
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Enabled</th>
              <th>Next Run</th>
              <th>Last Run</th>
              <th>Last Status</th>
              <th>Last Error</th>
            </tr>
          </thead>
          <tbody>
            {jobs.map((job) => (
              <tr key={job.id}>
                <td>{job.name}</td>
                <td>{job.enabled ? 'Yes' : 'No'}</td>
                <td>{formatDateTime(job.state.nextRunAtMs)}</td>
                <td>{formatDateTime(job.state.lastRunAtMs)}</td>
                <td>{job.state.lastStatus ?? '-'}</td>
                <td>{job.state.lastError ?? '-'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  )
}
