import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import './styles.css'
import { api } from './api'

async function bootstrap() {
  window.api =
    import.meta.env.DEV && new URLSearchParams(location.search).get('preview') === '1'
      ? (await import('./preview')).previewApi
      : api
  ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(<App />)
}
void bootstrap()
