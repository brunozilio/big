import { MotionConfig } from 'framer-motion'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { RouterProvider } from 'react-router-dom'
import { loadApi } from './api'
import { boot } from './api/store'
import { createRouter } from './app/router'
import { TooltipProvider } from './ui/overlay'
import './ui/tokens.css'


const root = document.getElementById('root')
if (root) {
  void loadApi().then((api) => {
    void boot(api)
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
