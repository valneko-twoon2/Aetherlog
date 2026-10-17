// In-browser stand-in for the Tauri backend, used only by `npm run dev` outside the
// desktop shell (see tools/preview.ps1).  It serves a generated, fictional log of
// 150,000 lines so paging, the slow-load popup and the start-up gauge can be tried
// without Rust or any real logs.  It is never part of a production build.

const TOTAL = 150_000
const STARTED = Date.now()
const LOAD_MS = 1800 // how long the fake start-up load takes
const LATENCY_PER_ROW = 0.02 // ms; makes very large pages visibly slow

const people = ['Kaito Windrider', 'Mina Ashgrove', 'Rin Duskbloom', 'Toma Brightwater', 'Sora Emberfall', 'Nia Frostvale']
const worlds = ['Tiamat', 'Ifrit', 'Garuda', 'Carbuncle']
const phrases = ['よろしくお願いします！', 'お疲れ様でした。', 'ありがとう。', '準備完了！', '回復してください！', '待ってください。']
const SELF = 'Lumi Starfall'
const CHARACTER = 'FFXIV_CHR00400000DEC0DE01'

const catalog = [
  [10, 'Say', 'chat', '#f7f7f7', '{n}: {m}'],
  [11, 'Shout', 'chat', '#ffa666', '{n}: {m}'],
  [12, 'Tell（送信）', 'chat', '#ffb8e8', '>> {n}: {m}'],
  [14, 'Party', 'chat', '#66e5ff', '({n}) {m}'],
  [16, 'Linkshell 1', 'chat', '#d4ff7d', '[1]<{n}> {m}'],
  [24, 'Free Company', 'chat', '#abdbe5', '[FC]<{n}> {m}'],
  [29, 'エモート', 'chat', '#bafff0', '{m}'],
  [57, 'システム', 'system', '#ccccff', '{m}'],
  [62, '戦利品', 'event', '#9be3a8', '{m}'],
  [41, 'ダメージ', 'battle', '#ffffff', '{m}'],
  [43, 'アクション詠唱', 'battle', '#d6d6ff', '{m}'],
  [45, 'HP回復', 'battle', '#a8ff9c', '{m}'],
  [46, 'バフ付与', 'battle', '#9ce0ff', '{m}'],
].map(([id, name, group, color, layout]) => ({ id, name, group, color, layout }))

const groups = [
  { id: 'chat', name: '会話' },
  { id: 'system', name: 'システム' },
  { id: 'event', name: 'イベント' },
  { id: 'battle', name: '戦闘' },
]

// Weighted kind choice: mostly battle noise, like a real log.
const kindWheel = [41, 41, 41, 41, 43, 43, 45, 46, 14, 14, 10, 11, 12, 16, 24, 29, 57, 62]

let rows = null
function build() {
  let seed = 14
  const rand = (n) => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff
    return seed % n
  }
  const start = Date.UTC(2026, 5, 20, 11, 0, 0) / 1000 // 2026-06-20 20:00 JST
  rows = []
  for (let i = 0; i < TOTAL; i++) {
    const k = kindWheel[rand(kindWheel.length)]
    const who = []
    const msg = []
    const name = people[rand(people.length)]
    const world = worlds[rand(worlds.length)]
    if (k === 14) {
      const mine = rand(4) === 0
      who.push({ s: String.fromCodePoint(0xe090 + (mine ? 0 : 1 + rand(3))) })
      if (mine) who.push({ s: SELF })
      else who.push({ s: name, l: 1 }, { k: 'icon', n: 88, a: 'CrossWorld' }, { s: world })
      msg.push({ k: 'at', s: phrases[rand(phrases.length)] })
    } else if ([10, 11, 12, 16, 24].includes(k)) {
      who.push({ s: name, l: 1 }, { k: 'icon', n: 88, a: 'CrossWorld' }, { s: world })
      msg.push({ s: ['おつかれさまです', 'こんばんは', '今夜の零式は21時からです。', 'カララント安売り中！'][rand(4)] })
    } else if (k === 29) {
      msg.push({ s: `${name}はお辞儀した。` })
    } else if (k === 57) {
      msg.push({ s: ['レストエリアに入った！', 'レストエリアから離れた。', '離席状態を解除しました。'][rand(3)] })
    } else if (k === 62) {
      msg.push({ s: '「黒衣森のブナ材」を手に入れた。', c: '#7EFFA0' })
    } else if (k === 41) {
      msg.push({ s: `あなたの「ファイガ」 エンシェントゴーレムに${(3000 + rand(4000)).toLocaleString()}ダメージ。`, b: 1 })
    } else if (k === 43) {
      msg.push({ s: `${name}は「ケアルガ」を唱えた。` })
    } else if (k === 45) {
      msg.push({ s: `${name}のHPを${(2000 + rand(2000)).toLocaleString()}回復した。` })
    } else {
      msg.push({ s: 'あなたに「迅速魔」の効果。' })
    }
    rows.push({ i, t: start + i * 2, k, who, msg, ch: CHARACTER, w: false })
  }
}

const plain = (spans) => spans.map((s) => s.s || '').join('')
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
const jstDay = (t) => new Date((t + 9 * 3600) * 1000).toISOString().slice(0, 10)

let cache = { key: '', picked: [] }
function select(params) {
  rows || build()
  const key = JSON.stringify([params.query, params.channels, params.character, params.since, params.until, params.newest_first])
  if (cache.key === key) return cache.picked
  const needle = (params.query || '').toLowerCase()
  const kinds = params.channels ? new Set(params.channels) : null
  let picked = rows.filter(
    (r) =>
      (!kinds || kinds.has(r.k)) &&
      (!params.character || r.ch === params.character) &&
      (!params.since || jstDay(r.t) >= params.since) &&
      (!params.until || jstDay(r.t) <= params.until) &&
      (!needle || (plain(r.who) + '\n' + plain(r.msg)).toLowerCase().includes(needle)),
  )
  if (params.newest_first) picked = picked.slice().reverse()
  cache = { key, picked }
  return picked
}

let manualName = ''
let monitoring = true
let preferences = { animations: true, refresh_seconds: 1 }
try {
  const saved = JSON.parse(localStorage.getItem('aethelog-preferences') || 'null')
  if (typeof saved?.animations === 'boolean' && Number.isInteger(saved.refresh_seconds) && saved.refresh_seconds >= 1 && saved.refresh_seconds <= 60) preferences = saved
} catch { /* Missing or invalid browser storage uses the same defaults as the desktop app. */ }

export default {
  async notices() {
    return JSON.stringify({
      packages: Array.from({ length: 40 }, (_, i) => ({ n: `sample-package-${i}`, v: `1.${i}.0`, l: 'MIT', h: 'https://example.com', t: [0] })),
      texts: ['MIT License\n\nCopyright (c) example\n\nPermission is hereby granted, free of charge, ...'],
    })
  },
  async state() {
    const elapsed = Date.now() - STARTED
    if (elapsed < LOAD_MS) {
      const total = 463
      return { status: 'loading', phase: elapsed < LOAD_MS / 2 ? 'copy' : 'read', progress: [Math.floor((total * (elapsed % (LOAD_MS / 2))) / (LOAD_MS / 2)), total], path: 'C:\\mock\\logs', error: '', total: 0, dict: 'ready', dict_error: '' }
    }
    return { status: 'ready', phase: 'read', progress: [463, 463], path: 'C:\\mock\\logs', error: '', total: TOTAL, dict: 'ready', dict_error: '', generation: 1, monitoring, refreshing: false, preferences }
  },
  async set_monitoring({ enabled }) { monitoring = enabled },
  async set_preferences({ preferences: next }) {
    if (!Number.isInteger(next.refresh_seconds) || next.refresh_seconds < 1 || next.refresh_seconds > 60) throw new Error('Invalid refresh interval')
    localStorage.setItem('aethelog-preferences', JSON.stringify(next))
    preferences = { ...next }
    return preferences
  },
  async meta() {
    rows || build()
    const counts = {}
    for (const r of rows) counts[r.k] = (counts[r.k] || 0) + 1
    return {
      channels: catalog,
      groups,
      counts,
      characters: [{ id: CHARACTER, name: manualName || SELF, guess: !manualName, count: TOTAL, counts, days: Object.entries(rows.reduce((acc, r) => ((acc[jstDay(r.t)] = (acc[jstDay(r.t)] || 0) + 1), acc), {})).map(([date, count]) => ({ date, count })) }],
      range: [jstDay(rows[0].t), jstDay(rows[rows.length - 1].t)],
      total: TOTAL,
      files: 463,
      trailing_files: 0,
      warning_records: 0,
      errors: [],
    }
  },
  async delete_character() {
    return { started: false }
  },
  async load_logs() {
    return { started: true }
  },
  async query({ params }) {
    const picked = select(params)
    const limit = params.limit > 0 ? params.limit : 200
    const offset = params.offset < 0 ? Math.floor(Math.max(0, picked.length - 1) / limit) * limit : Math.min(params.offset, Math.max(0, picked.length - 1))
    const page = picked.slice(offset, offset + limit).map((r, k) => ({ ...r, d: offset + k === 0 || jstDay(picked[offset + k - 1].t) !== jstDay(r.t) }))
    await delay(page.length * LATENCY_PER_ROW)
    return { total: picked.length, offset, limit, rows: page }
  },
  async export_entries({ params }) {
    const picked = select(params)
    const text = picked.slice(0, 1000).map((r) => `${r.t},${r.k},${plain(r.who)},${plain(r.msg)}`).join('\n')
    const url = URL.createObjectURL(new Blob([text], { type: 'text/plain' }))
    Object.assign(document.createElement('a'), { href: url, download: 'mock-export.txt' }).click()
    URL.revokeObjectURL(url)
  },
  async open_lodestone({ name, world }) {
    console.info('Lodestone search:', name, world)
  },
  async set_name({ name }) {
    manualName = name.trim()
  },
}
