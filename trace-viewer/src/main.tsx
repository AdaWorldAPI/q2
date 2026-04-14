import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import { App } from './App'
import { HttpTraceSource } from './trace-source'

const source = new HttpTraceSource('')
const container = document.getElementById('root')!
createRoot(container).render(
  <StrictMode>
    <App source={source} />
  </StrictMode>,
)
