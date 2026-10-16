import { createApp } from 'vue'
import '@fontsource/cinzel/500.css'
import '@fontsource/cinzel/700.css'
import './style.css'
import App from './App.vue'

const app = createApp(App)

// A bug in one component or promise must not take the window down; log it and carry on.
app.config.errorHandler = (err) => console.error('[aetherlog]', err)
window.addEventListener('error', (e) => { console.error('[aetherlog]', e.error || e.message); e.preventDefault() })
window.addEventListener('unhandledrejection', (e) => { console.error('[aetherlog]', e.reason); e.preventDefault() })

// Never navigate away from the app (dropped files, stray links) or open a context menu onto the shell.
window.addEventListener('dragover', (e) => e.preventDefault())
window.addEventListener('drop', (e) => e.preventDefault())
document.addEventListener('click', (e) => {
  if (e.target instanceof Element && e.target.closest('a[href]')) e.preventDefault()
})

// Browser-only behaviour does not belong in a desktop app (kept in dev so reload and devtools still work).
if (!import.meta.env.DEV) {
  window.addEventListener('contextmenu', (e) => e.preventDefault())
  window.addEventListener('keydown', (e) => {
    const key = e.key.toLowerCase()
    const mod = e.ctrlKey || e.metaKey
    const blocked =
      ['f3', 'f5', 'f7', 'f12'].includes(key) ||
      (e.altKey && ['arrowleft', 'arrowright'].includes(key)) ||
      (mod && ['r', 'p', 's', 'u', 'f', 'g', 'j', 'o', 'l', 'n', 'w', 't'].includes(key)) ||
      (mod && e.shiftKey && ['i', 'j', 'c', 'r', 'n', 'delete'].includes(key))
    if (blocked) e.preventDefault()
  }, true)
}

app.mount('#app')
