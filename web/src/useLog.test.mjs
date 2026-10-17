import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { test } from 'node:test'
import vm from 'node:vm'
import * as vue from 'vue'

// Run with: node --experimental-vm-modules --test src/useLog.test.mjs
test('live updates retain filters and paging, and unchanged generations do not reload', async () => {
  const context = vm.createContext({ setTimeout: () => 0, clearTimeout: () => {} })
  let generation = 1
  let total = 100
  let metaCalls = 0
  let queries = 0
  let preferences = { animations: true }
  let saveFails = false
  const get = async (path) => {
    if (path === '/api/state') return { status: 'ready', generation, preferences, error: '' }
    metaCalls++
    const counts = generation > 1 ? { 14: total - 1, 10: 1 } : { 14: total }
    return {
      channels: [{ id: 14, group: 'chat' }, { id: 10, group: 'chat' }],
      characters: [{ id: 'fixture', count: total, counts }],
    }
  }
  const post = async (path, body) => {
    if (path === '/api/preferences') {
      if (saveFails) throw new Error('Save failed')
      preferences = { ...body }
      return preferences
    }
    assert.equal(path, '/api/query')
    queries++
    return { total, rows: [] }
  }
  const vueModule = new vm.SyntheticModule(Object.keys(vue), function () {
    for (const [key, value] of Object.entries(vue)) this.setExport(key, value)
  }, { context })
  const apiModule = new vm.SyntheticModule(['get', 'post'], function () {
    this.setExport('get', get)
    this.setExport('post', post)
  }, { context })
  const module = new vm.SourceTextModule(await readFile(new URL('./useLog.js', import.meta.url), 'utf8'), { context })
  await module.link((name) => name === 'vue' ? vueModule : apiModule)
  await module.evaluate()
  const log = module.namespace
  await log.poll()
  await log.reload()
  const previousQueries = queries
  await log.poll()
  assert.equal(metaCalls, 1)
  assert.equal(queries, previousQueries)

  log.filters.query = 'Alice'
  log.filters.since = '2026-10-17'
  log.filters.until = '2026-10-17'
  log.selected.value = new Set()
  log.view.limit = 50
  await log.reload(0)
  log.view.followLatest = false
  generation++
  total = 101
  await log.poll()
  assert.equal(log.filters.query, 'Alice')
  assert.equal(log.filters.character, 'fixture')
  assert.equal(log.filters.since, '2026-10-17')
  assert.equal(log.filters.until, '2026-10-17')
  assert.equal(log.selected.value.has(14), false, 'a disabled channel stays disabled')
  assert.equal(log.selected.value.has(10), true, 'a newly seen channel is available')
  assert.equal(log.view.offset, 0, 'reading an older page does not jump to the latest page')
  assert.equal(log.view.preservePosition, true)

  await log.reload(-1)
  log.view.followLatest = true
  generation++
  total = 151
  await log.poll()
  assert.equal(log.view.offset, 150, 'following the latest page advances when a page fills')
  await log.setPreferences({ animations: false })
  assert.equal(log.state.preferences.animations, false)
  saveFails = true
  await assert.rejects(log.setPreferences({ animations: true }), /Save failed/)
  assert.equal(log.state.preferences.animations, false, 'failed saves preserve the previous setting')
  await log.poll()
  assert.equal(log.state.preferences.animations, false)
})
