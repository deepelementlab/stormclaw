# Monitoring UI (React, Phase 1)

This document describes the first phase of the Stormclaw monitoring control plane.

## Location

- Frontend app: `stormclaw/control-plane`
- Entry point: `stormclaw/control-plane/src/main.tsx`

## Scope

Phase 1 covers:

- Dashboard overview (`/`)
- Channel status (`/channels`)
- Cron jobs status (`/cron-jobs`)
- Heartbeat trigger action
- Prometheus metrics parsing and charting
- Realtime-first refresh with polling fallback

## API Mapping

- `GET /health` -> gateway liveness
- `GET /status` -> gateway aggregate status
- `GET /channels` -> channel status list
- `GET /cron/jobs` -> cron jobs and states
- `POST /heartbeat/trigger` -> manual heartbeat trigger
- `GET /metrics` -> Prometheus text metrics

## Realtime Strategy

The UI uses this strategy:

1. If `VITE_REALTIME_URL` is configured and browser supports `EventSource`, use SSE.
2. On each realtime message, invalidate core React Query caches.
3. If no realtime endpoint exists, fallback to periodic polling (`VITE_POLL_INTERVAL_MS`, default `5000`).

This allows the UI to stay functional without backend realtime implementation.

## Key Modules

- `src/services/apiClient.ts`: typed HTTP calls to gateway endpoints
- `src/services/metricsParser.ts`: Prometheus text parser for allowlisted metrics
- `src/services/realtimeClient.ts`: SSE connection wrapper
- `src/hooks/useRealtimeOrPolling.ts`: realtime/polling orchestration
- `src/pages/*`: phase-1 monitoring pages

## Validation Commands

```bash
cd control-plane
npm run test
npm run build
```
