import { useQuery } from '@tanstack/react-query'
import { useRealtimeOrPolling } from '../hooks/useRealtimeOrPolling'
import { fetchChannels } from '../services/apiClient'

export function ChannelsPage() {
  useRealtimeOrPolling([['channels']])

  const channelsQuery = useQuery({
    queryKey: ['channels'],
    queryFn: fetchChannels,
  })

  if (channelsQuery.isLoading) {
    return <p>Loading channels...</p>
  }

  if (channelsQuery.isError) {
    return <p className="error">Failed to load channels.</p>
  }

  const channels = Object.entries(channelsQuery.data ?? {})

  return (
    <section className="page-grid">
      <h2>Channels</h2>
      <div className="panel">
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Enabled</th>
              <th>Running</th>
            </tr>
          </thead>
          <tbody>
            {channels.map(([name, state]) => (
              <tr key={name}>
                <td>{name}</td>
                <td>{state.enabled ? 'Yes' : 'No'}</td>
                <td>{state.running ? 'Running' : 'Stopped'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  )
}
