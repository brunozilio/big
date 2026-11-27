import { createBrowserRouter, Navigate } from 'react-router-dom'
import { basePath } from '@/api/base'
import { App } from '@/screens/App'
import { Account } from '@/screens/login/Account'
import { Login } from '@/screens/login/Login'

export const routes = [
  { path: '/', element: <App /> },
  { path: '/items/:id', element: <App /> },
  { path: '/items/:id/:step', element: <App /> },
  { path: '/settings', element: <Navigate to="/settings/connections" replace /> },
  { path: '/settings/:tab', element: <App /> },
  { path: '/security', element: <App /> },
  { path: '/login', element: <Login /> },
  { path: '/account', element: <Account /> },
  { path: '*', element: <Navigate to="/" replace /> },
]

export function createRouter() {
  return createBrowserRouter(routes, { basename: basePath() || undefined })
}
