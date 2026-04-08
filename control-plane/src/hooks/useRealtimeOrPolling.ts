import { useEffect } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { createRealtimeClient } from '../services/realtimeClient'

const POLL_INTERVAL_MS = Number(import.meta.env.VITE_POLL_INTERVAL_MS ?? 5000)

export function useRealtimeOrPolling(queryKeys: Array<readonly unknown[]>) {
  const queryClient = useQueryClient()

  useEffect(() => {
    const invalidateAll = () => {
      for (const key of queryKeys) {
        void queryClient.invalidateQueries({ queryKey: key })
      }
    }

    const realtimeClient = createRealtimeClient({
      onMessage: invalidateAll,
      onError: () => {
        invalidateAll()
      },
    })

    if (realtimeClient) {
      realtimeClient.connect()
      return () => realtimeClient.disconnect()
    }

    const timer = window.setInterval(() => {
      invalidateAll()
    }, POLL_INTERVAL_MS)

    return () => window.clearInterval(timer)
  }, [queryClient, queryKeys])
}
