// Thin wrapper over the Tauri backend.  Routes keep the old REST-style names so
// the rest of the front end does not care how the backend is reached.
import { invoke } from '@tauri-apps/api/core'
import { open, save } from '@tauri-apps/plugin-dialog'

const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

// Outside the desktop shell (npm run dev in a browser) a generated mock backend answers instead.
const backend = async (command, args) => {
  if (!import.meta.env.DEV) throw new Error('The Tauri backend is not available')
  const mock = (await import('./mock.js')).default
  return mock[command](args)
}

const call = async (command, args) => {
  try {
    return inTauri ? await invoke(command, args) : await backend(command, args)
  } catch (e) {
    throw new Error(typeof e === 'string' ? e : e?.message || String(e))
  }
}

export const get = (path) => {
  if (path === '/api/state') return call('state')
  if (path === '/api/meta') return call('meta')
  if (path === '/api/notices') return call('notices')
  throw new Error(`unknown route ${path}`)
}

export const post = async (path, body = {}) => {
  switch (path) {
    case '/api/load':
      return call('load_logs', { path: body.path, focus: !!body.focus })
    case '/api/monitoring':
      return call('set_monitoring', { enabled: !!body.enabled })
    case '/api/preferences':
      return call('set_preferences', { preferences: body })
    case '/api/pick': {
      if (!inTauri) return { path: 'C:/mock/logs' }
      const picked = await open(
        body.kind === 'file'
          ? { multiple: false, filters: [{ name: 'FF14ログ', extensions: ['log'] }] }
          : { directory: true, multiple: false },
      )
      return { path: typeof picked === 'string' ? picked : '' }
    }
    case '/api/query':
      return call('query', { params: body })
    case '/api/delete_character':
      return call('delete_character', { id: body.id })
    case '/api/homepage':
      return call('open_homepage', { index: body.index })
    case '/api/lodestone':
      return call('open_lodestone', { name: body.name, world: body.world })
    case '/api/name':
      return call('set_name', { id: body.id, name: body.name })
    default:
      throw new Error(`unknown route ${path}`)
  }
}

// Asks where to save, then lets the backend write the file.
export const download = async (path, body, fallbackName) => {
  if (path !== '/api/export') throw new Error(`unknown route ${path}`)
  const { format, ...params } = body
  if (!inTauri) return call('export_entries', { params, path: '' })
  const target = await save({
    defaultPath: fallbackName,
    filters: [{ name: format.toUpperCase(), extensions: [format] }],
  })
  if (target) await call('export_entries', { params, path: target })
}

// Files or folders dropped on the window. The handler gets {type: 'enter' | 'over' | 'drop' | 'leave', paths}.
// Only the desktop shell reports drops (with real paths); a browser preview ignores them.
export async function onDrop(handler) {
  if (!inTauri) return () => {}
  const { getCurrentWebview } = await import('@tauri-apps/api/webview')
  return getCurrentWebview().onDragDropEvent((event) => handler(event.payload))
}
