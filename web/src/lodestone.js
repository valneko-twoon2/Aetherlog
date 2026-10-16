// Ctrl+click on a player name opens that name on the Lodestone in the system browser.
import { post } from './api.js'

export const openPage = (player) => post('/api/lodestone', player).catch((e) => console.error('[aetherlog]', e))

// CSS cannot see the Ctrl key, so mirror it on <html> for the pointer style.
const mark = (e) => document.documentElement.classList.toggle('ctrl', e.ctrlKey || e.metaKey)
window.addEventListener('keydown', mark)
window.addEventListener('keyup', mark)
window.addEventListener('blur', () => document.documentElement.classList.remove('ctrl'))
