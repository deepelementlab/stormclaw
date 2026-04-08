export interface RealtimeClient {
  connect: () => void
  disconnect: () => void
}

interface RealtimeOptions {
  onMessage: () => void
  onError?: (error: unknown) => void
}

export function createRealtimeClient(options: RealtimeOptions): RealtimeClient | null {
  const realtimeUrl = import.meta.env.VITE_REALTIME_URL as string | undefined
  if (!realtimeUrl || typeof EventSource === 'undefined') {
    return null
  }

  let source: EventSource | null = null

  return {
    connect: () => {
      if (source) {
        return
      }
      source = new EventSource(realtimeUrl)
      source.onmessage = () => options.onMessage()
      source.onerror = (event) => {
        options.onError?.(event)
      }
    },
    disconnect: () => {
      source?.close()
      source = null
    },
  }
}
