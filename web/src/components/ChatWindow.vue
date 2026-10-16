<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import ChatLine from './ChatLine.vue'
import { channelById, countOf, ensureRows, filters, kindsOf, meta, reload, rowAt, selected, setTab, soloKind, tab, toggleKind, view } from '../useLog.js'
import { HeightIndex } from '../virtual.js'
import { fallbackChannel } from '../channels.js'

const scroller = ref(null)

const tabs = computed(() => [
  ...(meta.value?.groups || []).filter((g) => kindsOf(g.id).length),
  { id: 'all', name: 'すべて' },
])
const chips = computed(() => kindsOf(tab.value))
const pages = computed(() => Math.max(1, Math.ceil(view.total / view.limit)))
const page = computed(() => Math.floor(view.offset / view.limit) + 1)

// Day-by-day view: picking a day narrows the date range to that single day.
const days = computed(() => meta.value?.characters.find((c) => c.id === filters.character)?.days || [])
const day = computed(() => (filters.since && filters.since === filters.until ? filters.since : ''))
const dayIndex = computed(() => days.value.findIndex((d) => d.date === day.value))
const dayLabel = (d) => {
  const [y, m, dd] = d.date.split('-').map(Number)
  const weekday = '日月火水木金土'[new Date(Date.UTC(y, m - 1, dd)).getUTCDay()]
  return `${y}/${String(m).padStart(2, '0')}/${String(dd).padStart(2, '0')}（${weekday}）  ${d.count.toLocaleString()} 件`
}
// Another character may have no lines on the chosen day.
watch(days, (list) => {
  if (filters.since && !list.some((d) => d.date === filters.since)) Object.assign(filters, { since: '', until: '' })
})
const pickDay = (date) => Object.assign(filters, { since: date, until: date })
const stepDay = (by) => {
  const next = days.value[dayIndex.value + by]
  if (next) pickDay(next.date)
}
// With no day chosen, the arrows start from the newest day (back) or the oldest (forward).
const stepBack = () => (dayIndex.value < 0 ? pickDay(days.value[days.value.length - 1]?.date || '') : stepDay(-1))
const stepForward = () => (dayIndex.value < 0 ? pickDay(days.value[0]?.date || '') : stepDay(1))

const channel = (k) => channelById.value[k] || fallbackChannel(k)

const bannerOf = (row) => {
  const d = new Date((row.t + 9 * 3600) * 1000)
  return `${d.toISOString().slice(0, 10).replace(/-/g, '/')}（${'日月火水木金土'[d.getUTCDay()]}）`
}

// Virtual list: only the rows near the window exist in the DOM, and only their data is fetched,
// so a page of "all" (hundreds of thousands of lines) opens at once.
const EST = 23.25 // one line: 15px font at line-height 1.55
const OVERSCAN = 12
const PAD = 8 // top padding of .log
const win = ref(null)
const range = reactive({ first: 0, last: -1 })
const rev = ref(0) // bumped whenever a measured height changes
let heights = new HeightIndex(0, EST)
let pinned = true // follow the newest line until the user scrolls

const totalHeight = computed(() => (rev.value, heights.total()))
const top = computed(() => (rev.value, heights.offset(range.first)))
const items = computed(() => {
  view.version
  const out = []
  for (let i = range.first; i <= range.last; i++) out.push({ i, row: rowAt(i) })
  return out
})

function update() {
  const el = scroller.value
  if (!el || !view.count) {
    Object.assign(range, { first: 0, last: -1 })
    return
  }
  const y = Math.max(0, el.scrollTop - PAD)
  const first = Math.max(0, heights.find(y) - OVERSCAN)
  const last = Math.min(view.count - 1, heights.find(y + el.clientHeight) + OVERSCAN)
  if (first !== range.first || last !== range.last) Object.assign(range, { first, last })
  ensureRows(first, last)
}

// Measures the rows that are on screen and corrects the estimates.
async function measure() {
  const el = scroller.value
  if (!el || !win.value) return
  const anchor = heights.find(Math.max(0, el.scrollTop - PAD))
  let shift = 0
  let changed = false
  for (const node of win.value.children) {
    const i = Number(node.dataset.i)
    const h = node.offsetHeight
    if (Math.abs(h - heights.get(i)) > 0.5) {
      // A row above the first visible one that grows pushes the lines down; scroll to compensate.
      if (i < anchor) shift += h - heights.get(i)
      heights.set(i, h)
      changed = true
    }
  }
  if (!changed && !pinned) return
  if (changed) rev.value++
  await nextTick()
  if (pinned) el.scrollTop = el.scrollHeight
  else if (shift) el.scrollTop += shift
}

const unpin = () => {
  pinned = false
}

function onScroll() {
  const el = scroller.value
  // Scrolled away from the newest line by any means (scrollbar, keys, wheel): stop following it.
  if (pinned && el && el.scrollHeight - el.clientHeight - el.scrollTop > 100) pinned = false
  update()
}

watch(() => view.serial, async () => {
  heights = new HeightIndex(view.count, EST)
  pinned = true
  rev.value++
  Object.assign(range, { first: 0, last: -1 })
  await nextTick()
  if (scroller.value) scroller.value.scrollTop = view.newestFirst ? 0 : scroller.value.scrollHeight
  update()
})
watch(items, async () => {
  await nextTick()
  measure()
}, { flush: 'post' })

let observer
let lastWidth = 0
onMounted(() => {
  observer = new ResizeObserver(() => {
    const el = scroller.value
    if (!el) return
    // Line wrapping depends on the width, so earlier measurements no longer hold.
    if (el.clientWidth !== lastWidth) {
      lastWidth = el.clientWidth
      heights = new HeightIndex(view.count, EST)
      rev.value++
    }
    update()
  })
  observer.observe(scroller.value)
})
onBeforeUnmount(() => observer?.disconnect())

const ALL = 1_000_000_000
const sizes = [50, 100, 200, 500, 1000, 10000, 100000, ALL]
const resize = () => reload(Math.floor(view.offset / view.limit) * view.limit)
const go = (p) => reload((Math.min(Math.max(p, 1), pages.value) - 1) * view.limit)
</script>

<template>
  <section class="window chat">
    <header class="tabs" role="tablist">
      <button v-for="t in tabs" :key="t.id" role="tab" :aria-selected="tab === t.id" :class="{ active: tab === t.id }" @click="setTab(t.id)">
        {{ t.name }}
      </button>
    </header>
    <div v-if="days.length" class="daybar">
      <span class="label">日付</span>
      <button class="ff-btn" :disabled="dayIndex === 0" aria-label="前の日" @click="stepBack">‹</button>
      <select class="ff-input" aria-label="日付ごとに表示" :value="day" @change="pickDay($event.target.value)">
        <option value="">すべての日（{{ days.length.toLocaleString() }} 日分）</option>
        <option v-for="d in [...days].reverse()" :key="d.date" :value="d.date">{{ dayLabel(d) }}</option>
      </select>
      <button class="ff-btn" :disabled="dayIndex === days.length - 1" aria-label="次の日" @click="stepForward">›</button>
      <button v-if="day" class="ff-btn" @click="pickDay('')">すべての日</button>
    </div>
    <div class="chips">
      <button
        v-for="c in chips"
        :key="c.id"
        class="chip"
        :class="{ on: selected.has(c.id) }"
        title="クリック: 切替 / ダブルクリック: これだけ表示"
        @click="toggleKind(c.id)"
        @dblclick="soloKind(c.id)"
      >
        <i :style="{ background: c.color }" />{{ c.name }}<b>{{ countOf(c.id).toLocaleString() }}</b>
      </button>
    </div>
    <div ref="scroller" class="log" :class="{ busy: view.busy }" @scroll.passive="onScroll" @wheel.passive="unpin" @pointerdown="unpin" @touchstart.passive="unpin">
      <p v-if="view.error" class="notice err">{{ view.error }}</p>
      <p v-else-if="!view.count && !view.busy" class="notice">条件に合うログがありません。</p>
      <div class="vspace" :style="{ height: totalHeight + 'px' }">
        <div ref="win" class="vwin" :style="{ top: top + 'px' }">
          <div v-for="it in items" :key="it.i" class="vrow" :data-i="it.i" :style="it.row ? null : { height: EST + 'px' }">
            <template v-if="it.row">
              <div v-if="it.row.d || it.i === 0" class="day">{{ bannerOf(it.row) }}</div>
              <ChatLine :row="it.row" :channel="channel(it.row.k)" :query="filters.query" />
            </template>
          </div>
        </div>
      </div>
    </div>
    <footer class="pager">
      <button class="ff-btn" :disabled="page <= 1" aria-label="前のページ" @click="go(page - 1)">‹</button>
      <span class="page">{{ page }} / {{ pages }}</span>
      <button class="ff-btn" :disabled="page >= pages" aria-label="次のページ" @click="go(page + 1)">›</button>
      <button class="ff-btn" :disabled="page >= pages" @click="reload(-1)">最新へ</button>
      <span class="spacer" />
      <span class="count">{{ view.total.toLocaleString() }} 件</span>
      <select v-model.number="view.limit" class="ff-input size" aria-label="1ページの表示件数" @change="resize">
        <option v-for="n in sizes" :key="n" :value="n">{{ n === ALL ? 'すべて' : `${n.toLocaleString()} 件ずつ` }}</option>
      </select>
    </footer>
  </section>
</template>
