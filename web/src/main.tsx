import { MotionConfig } from 'framer-motion'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { RouterProvider } from 'react-router-dom'
import { loadApi } from './api'
import { whoAmI } from './api/auth'
import { appPath, toLogin } from './api/base'
import { setMe } from './api/me'
import { boot } from './api/store'
import { createRouter } from './app/router'
import { TooltipProvider } from './ui/overlay'
import './ui/tokens.css'


const root = document.getElementById('root')
if (root) {
  void loadApi().then(async (api) => {
    const signedIn = await whoAmI().catch(() => undefined)
    const onLogin = appPath() === '/login'
    if (signedIn) setMe(signedIn)
    if (signedIn === null && !onLogin) {
      window.location.replace(toLogin())
      return
    }
    if (signedIn !== null && !onLogin) void boot(api)
    createRoot(root).render(
      <StrictMode>
        <MotionConfig reducedMotion="user">
          <TooltipProvider>
            <RouterProvider router={createRouter()} />
          </TooltipProvider>
        </MotionConfig>
      </StrictMode>,
    )
  })
}
