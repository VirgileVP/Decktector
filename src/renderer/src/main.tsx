import '@fontsource-variable/geist'
import '@fontsource-variable/geist-mono'
import './assets/main.css'

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { preloadCanvasFont } from './lib/canvas-font'
import { panelParams } from './panels/params'
import { PanelWindow } from './panels/PanelWindow'

preloadCanvasFont()

const params = panelParams(window.location)

createRoot(document.getElementById('root')!).render(
  <StrictMode>{params ? <PanelWindow params={params} /> : <App />}</StrictMode>
)
