import type { ReactNode } from 'react'

interface StatCardProps {
  title: string
  value: ReactNode
  hint?: string
}

export function StatCard({ title, value, hint }: StatCardProps) {
  return (
    <article className="card">
      <h3>{title}</h3>
      <p className="metric-value">{value}</p>
      {hint ? <small>{hint}</small> : null}
    </article>
  )
}
