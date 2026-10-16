import assert from 'node:assert/strict'
import test from 'node:test'
import { countOf, filters, kindsOf, meta, selected, tab } from './useLog.js'

test('character changes scope channel chips, counts, tabs, and selections', () => {
  meta.value = {
    channels: [{ id: 10, group: 'chat' }, { id: 60, group: 'system' }],
    counts: { 10: 8, 60: 7 },
    characters: [
      { id: 'first', counts: { 10: 3 } },
      { id: 'second', counts: { 10: 5, 60: 7 } },
    ],
  }
  filters.character = 'second'
  tab.value = 'system'
  selected.value = new Set([60])
  assert.equal(countOf(10), 5)
  assert.deepEqual(kindsOf('system').map((c) => c.id), [60])

  filters.character = 'first'
  assert.equal(countOf(10), 3)
  assert.equal(countOf(60), 0)
  assert.deepEqual(kindsOf('system'), [])
  assert.equal(tab.value, 'all')
  assert.deepEqual([...selected.value], [10])

  filters.character = 'second'
  assert.deepEqual([...selected.value], [10, 60])
  filters.character = 'missing'
  assert.deepEqual(kindsOf('all'), [])
  assert.deepEqual([...selected.value], [])
})
