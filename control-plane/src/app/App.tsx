import { NavLink, Route, Routes } from 'react-router-dom'
import { DashboardPage } from '../pages/DashboardPage'
import { ChannelsPage } from '../pages/ChannelsPage'
import { CronJobsPage } from '../pages/CronJobsPage'

export function App() {
  return (
    <div className="app-shell">
      <header className="topbar">
        <h1>Stormclaw Control Plane</h1>
        <nav className="nav">
          <NavLink to="/" end>
            Dashboard
          </NavLink>
          <NavLink to="/channels">Channels</NavLink>
          <NavLink to="/cron-jobs">Cron Jobs</NavLink>
        </nav>
      </header>

      <main className="content">
        <Routes>
          <Route path="/" element={<DashboardPage />} />
          <Route path="/channels" element={<ChannelsPage />} />
          <Route path="/cron-jobs" element={<CronJobsPage />} />
        </Routes>
      </main>
    </div>
  )
}
