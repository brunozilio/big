import { createBrowserRouter, Navigate } from 'react-router-dom'
import { App } from '@/screens/App'

export const routes = [
  { path: '/', element: <App /> },
  { path: '/items/:id', element: <App /> },
  { path: '/items/:id/:step', element: <App /> },
  { path: '/settings', element: <Navigate to="/settings/connections" replace /> },
  { path: '/settings/:tab', element: <App /> },
  { path: '/security', element: <App /> },
  { path: '*', element: <Navigate to="/" replace /> },
]

export function createRouter() {
  return createBrowserRouter(routes)
}
