import './assets/main.css'

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { panelParams } from './panels/params'
import { PanelWindow } from './panels/PanelWindow'

const params = panelParams(window.location)

createRoot(document.getElementById('root')!).render(
  <StrictMode>{params ? <PanelWindow params={params} /> : <App />}</StrictMode>
)
