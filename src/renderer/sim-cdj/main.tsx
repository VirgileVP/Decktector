import '@fontsource-variable/geist'
import '@fontsource-variable/geist-mono'
import '@renderer/assets/themes.css'
import './styles/page.css'
import './styles/cdj.css'
import './styles/mixer.css'
import './styles/browser.css'

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import { preloadCanvasFont } from '@renderer/lib/canvas-font'

import { SimPage } from './SimPage'

preloadCanvasFont()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <SimPage />
  </StrictMode>
)
