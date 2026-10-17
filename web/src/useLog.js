import { computed, markRaw, reactive, ref, watch } from 'vue'
import { get, post } from './api.js'

export const state = reactive({ status: 'idle', phase: '', progress: [0, 0], path: '', error: '', total: 0, focus: '', dict: '', dict_error: '', generation: 0, refreshing: false, preferences: { animations: true } })
export const meta = ref(null)
export const tab = ref('chat')
export const selected = ref(new Set())
export const filters = reactive({ query: '', character: '', since: '', until: '' })
export const view = reactive({ total: 0, offset: 0, count: 0, limit: 1_000_000_000, busy: false, slow: false, error: '', newestFirst: false, version: 0, serial: 0 })
export const channelById = computed(() => Object.fromEntries((meta.value?.channels || []).map((c) => [c.id, c])))

const characterCounts = computed(() =>
  meta.value?.characters.find((c) => c.id === filters.character)?.counts || {})
const counts = (id) => characterCounts.value[id] || 0
export const kindsOf = (groupId) =>
  (meta.value?.channels || []).filter((c) => counts(c.id) > 0 && (groupId === 'all' || c.group === groupId))
export const countOf = counts

// A character change must also reset hidden channel selections and unavailable tabs.
watch(() => filters.character, () => {
  if (!kindsOf(tab.value).length) tab.value = 'all'
  selected.value = new Set(kindsOf(tab.value).map((c) => c.id))
}, { flush: 'sync' })

export function setTab(groupId) {
  tab.value = groupId
  selected.value = new Set(kindsOf(groupId).map((c) => c.id))
  reload(-1)
}

export function toggleKind(id) {
  const next = new Set(selected.value)
  next.has(id) ? next.delete(id) : next.add(id)
  selected.value = next
  reload(-1)
}

export function soloKind(id) {
  selected.value = new Set([id])
  reload(-1)
}

export function setAllInTab(on) {
  const ids = kindsOf(tab.value).map((c) => c.id)
  const next = new Set(selected.value)
  ids.forEach((id) => (on ? next.add(id) : next.delete(id)))
  selected.value = next
  reload(-1)
}

const queryBody = () => ({
  query: filters.query,
  character: filters.character,
  since: filters.since,
  until: filters.until,
  channels: [...selected.value],
  newest_first: view.newestFirst,
})

// The "loading" popup only appears when a query is still running after this long.
const SLOW_MS = 400
// The list shows one page, but only the rows near the window are ever fetched, in blocks.
const BLOCK = 400
const KEEP_BLOCKS = 60
let token = 0
let cancelled = false
let slowTimer
let source = null
let blocks = new Map()

// Row at position i of the current page, or null while its block is still on the way.
export function rowAt(i) {
  const block = blocks.get(Math.floor(i / BLOCK))
  return Array.isArray(block) ? block[i % BLOCK] || null : null
}

async function fetchBlock(index, src) {
  blocks.set(index, 'pending')
  const from = index * BLOCK
  try {
    // The backend caches the filter result, so a block request only renders its own rows.
    const part = await post('/api/query', { ...src.body, offset: src.start + from, limit: Math.min(BLOCK, src.count - from) })
    if (src !== source) return
    // markRaw: rows must not be wrapped in proxies.
    blocks.set(index, markRaw(part.rows))
  } catch (e) {
    if (src !== source) return
    blocks.set(index, [])
    view.error = e.message
  }
  view.version++
}

// Makes sure the rows first..last (positions in the page) are loaded.
export function ensureRows(first, last) {
  if (!source || last < first) return
  const lo = Math.floor(first / BLOCK)
  const hi = Math.floor(last / BLOCK)
  for (let b = lo; b <= hi; b++) if (!blocks.has(b)) fetchBlock(b, source)
  if (blocks.size > KEEP_BLOCKS) {
    const centre = (lo + hi) / 2
    for (const [b, value] of blocks) if (value !== 'pending' && Math.abs(b - centre) > KEEP_BLOCKS / 2) blocks.delete(b)
  }
}

// Stops waiting for the query that is running; the list keeps what it showed before.
export function cancelLoad() {
  cancelled = true
}

export async function reload(offset = 0, preservePosition = false) {
  const mine = ++token
  cancelled = false
  view.busy = true
  view.error = ''
  clearTimeout(slowTimer)
  slowTimer = setTimeout(() => {
    if (mine === token) view.slow = true
  }, SLOW_MS)
  try {
    const body = queryBody()
    const { total } = await post('/api/query', { ...body, offset: 0, limit: 1 })
    if (mine !== token || cancelled) return
    const size = view.limit
    const start = total === 0 ? 0 : offset < 0 ? Math.floor((total - 1) / size) * size : Math.min(offset, total - 1)
    const count = Math.min(size, total - start)
    blocks = new Map()
    source = { body, start, count }
    Object.assign(view, { total, offset: start, count, preservePosition })
    view.serial++
    view.version++
  } catch (e) {
    if (mine === token) view.error = e.message
  } finally {
    if (mine === token) {
      clearTimeout(slowTimer)
      view.busy = false
      view.slow = false
    }
  }
}

export const exportBody = queryBody

export async function renameCharacter(id, name) {
  await post('/api/name', { id, name })
  meta.value = await get('/api/meta')
}

let timer
let polling = false
let seenGeneration = -1
let manualLoad = true
// focus: show the character of this log afterwards (used when the user picks or drops a log).
export async function startLoad(path, focus = false) {
  const result = await post('/api/load', { path, focus })
  if (!result.started) throw new Error('更新中です。少し待ってからもう一度お試しください。')
  manualLoad = true
  await poll()
}

export async function setPreferences(changes) {
  state.preferences = await post('/api/preferences', { ...state.preferences, ...changes })
}

export async function poll() {
  if (polling) return
  polling = true
  clearTimeout(timer)
  try {
    Object.assign(state, await get('/api/state'))
    if (state.status !== 'ready' || state.generation === seenGeneration) return
    const previousKinds = new Set(kindsOf(tab.value).map((c) => c.id))
    meta.value = await get('/api/meta')
    // One character is always shown: keep the current one, else the one with the most lines.
    const chars = meta.value.characters
    if (!chars.some((c) => c.id === filters.character)) {
      filters.character = [...chars].sort((x, y) => y.count - x.count)[0]?.id || ''
    }
    if (manualLoad) {
      if (state.focus && chars.some((c) => c.id === state.focus)) filters.character = state.focus
      setTab(tab.value === 'all' || kindsOf(tab.value).length ? tab.value : 'all')
    } else {
      const next = new Set(selected.value)
      for (const c of kindsOf(tab.value)) if (!previousKinds.has(c.id)) next.add(c.id)
      selected.value = next
      const wasLastPage = view.offset + view.count >= view.total
      await reload(wasLastPage && view.followLatest !== false ? -1 : view.offset, true)
    }
    seenGeneration = state.generation
    manualLoad = false
  } catch (e) {
    state.error = e.message
  } finally {
    polling = false
    timer = setTimeout(poll, state.status === 'loading' || state.dict === 'downloading' ? 300 : 1000)
  }
}

// Deletes the archived logs of one character, then shows what is left.
export async function deleteCharacter(id) {
  await post('/api/delete_character', { id })
  manualLoad = true
  poll()
}
