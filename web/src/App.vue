<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import ChatWindow from './components/ChatWindow.vue'
import { download, get, onDrop, post } from './api.js'
import { cancelLoad, deleteCharacter, exportBody, filters, meta, poll, reload, renameCharacter, setPreferences, startLoad, state, view } from './useLog.js'

const path = ref('')
const menu = ref(false)
const exporting = ref(false)
const message = ref('')
const preferencesBusy = ref(false)
watch(() => state.preferences.animations, (enabled) => {
  document.documentElement.dataset.animations = enabled ? 'on' : 'off'
}, { immediate: true })

async function savePreferences(changes, event) {
  preferencesBusy.value = true
  message.value = ''
  try {
    await setPreferences(changes)
  } catch (e) {
    message.value = e.message
    event.target.checked = state.preferences.animations
  } finally {
    preferencesBusy.value = false
  }
}
// Injected by vite.config.js from package.json.
const version = __APP_VERSION__
const aboutDlg = ref(null)
const noticesDlg = ref(null)
const notices = ref(null) // { packages: [{ n, v, l, h, t }], texts }
const openedPackage = ref(-1)
// Closes on a click outside the box. A text selection that is dragged and released over the box
// padding also ends in a click on the dialog itself, so both press and release must be outside.
const closeOnOutside = (dlg) => {
  const outside = (e) => {
    const r = dlg.value.getBoundingClientRect()
    return e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom
  }
  let pressedOutside = false
  return {
    down: (e) => (pressedOutside = outside(e)),
    click: (e) => {
      if (pressedOutside && outside(e)) dlg.value.close()
      pressedOutside = false
    },
  }
}
const aboutClose = closeOnOutside(aboutDlg)
const noticesClose = closeOnOutside(noticesDlg)
const openNotices = async () => {
  if (!notices.value) {
    try {
      notices.value = JSON.parse(await get('/api/notices'))
    } catch {
      notices.value = { packages: [], texts: [] }
    }
  }
  openedPackage.value = -1
  noticesDlg.value.showModal()
}
const openHomepage = (index) => post('/api/homepage', { index }).catch(() => {})
const toggleLicense = (index) => (openedPackage.value = openedPackage.value === index ? -1 : index)
const licenseOf = (p) =>
  p.t.length ? p.t.map((i) => notices.value.texts[i]).join('\n\n---\n\n') : `ライセンス本文は同梱されていません（宣言: ${p.l || '不明'}）`
const license = `MIT License

Copyright (c) 2026 valneko-twoon2

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.`
// A modal dialog makes everything behind it inert (filters, menu, pager) while a page loads.
const waitDlg = ref(null)
watch(() => view.slow, async (slow) => {
  await nextTick()
  const dlg = waitDlg.value
  if (!dlg) return
  if (slow && !dlg.open) dlg.showModal()
  else if (!slow && dlg.open) dlg.close()
})
const dragging = ref(false)
let stopDrop = () => {}
onMounted(async () => {
  await poll()
  path.value = state.path
  stopDrop = await onDrop((e) => {
    dragging.value = e.type === 'enter' || e.type === 'over'
    if (e.type === 'drop' && e.paths?.length) {
      path.value = e.paths[0]
      load(true)
    }
  })
})
onBeforeUnmount(() => stopDrop())

// Deleting a character's archived logs takes two confirmations.
const delDlg = ref(null)
const delStep = ref(1)
const askDelete = () => {
  delStep.value = 1
  delDlg.value.showModal()
}
async function confirmDelete() {
  const id = current.value?.id
  delDlg.value.close()
  if (!id) return
  try {
    await deleteCharacter(id)
  } catch (e) {
    message.value = e.message
  }
}

const loading = computed(() => state.status === 'loading')
const percent = computed(() => (state.progress[1] ? Math.round((state.progress[0] / state.progress[1]) * 100) : 0))
const current = computed(() => meta.value?.characters.find((c) => c.id === filters.character))
// The name box edits a draft: binding it straight to the stored name would put the old text back
// whenever the page re-renders (the state is polled), erasing what is being typed.
const nameDraft = ref('')
watch(() => `${current.value?.id}\n${current.value?.name || ''}`, () => {
  nameDraft.value = current.value?.name || ''
}, { immediate: true })

// focus: the log was picked or dropped by hand, so show its character.
async function load(focus = false) {
  message.value = ''
  try {
    await startLoad(path.value, focus)
    menu.value = false
  } catch (e) {
    message.value = e.message
  }
}

async function pick(kind) {
  const res = await post('/api/pick', { kind })
  if (res.path) {
    path.value = res.path
    load(true)
  }
}

let timer
watch(filters, () => {
  clearTimeout(timer)
  timer = setTimeout(() => reload(-1), 300)
})

function save(format) {
  exporting.value = false
  download('/api/export', { ...exportBody(), format }, `ffxiv-log.${format}`).catch((e) => (message.value = e.message))
}

</script>

<template>
  <div class="app" @keydown.esc="menu = false; exporting = false">
    <header class="topbar">
      <h1 class="brand"><button type="button" class="brand-about" aria-label="Aethelog について" aria-haspopup="dialog" @click="menu = false; exporting = false; aboutDlg.showModal()">Aethelog</button></h1>
      <input v-model="filters.query" class="ff-input search" type="search" placeholder="発言者・本文を検索" aria-label="検索" />
      <div class="export">
        <button class="ff-btn" :disabled="!meta" aria-haspopup="true" :aria-expanded="exporting" @click="menu = false; exporting = !exporting">エクスポート ▾</button>
        <div v-if="exporting" class="window export-menu" role="menu">
          <button class="ff-btn" role="menuitem" @click="save('csv')">CSV</button>
          <button class="ff-btn" role="menuitem" @click="save('json')">JSON</button>
        </div>
      </div>
      <button class="ff-btn menu-btn" aria-haspopup="true" :aria-expanded="menu" @click="exporting = false; menu = !menu">メニュー ▾</button>
    </header>

    <div v-if="menu || exporting" class="scrim" @click="menu = false; exporting = false" />
    <div v-if="menu" class="window menu" role="menu">
      <template v-if="meta">
        <section>
          <h2>絞り込み</h2>
          <label>キャラクター
            <select v-model="filters.character" class="ff-input">
              <option v-for="c in meta.characters" :key="c.id" :value="c.id">{{ c.name || c.id }}（{{ c.count.toLocaleString() }}）</option>
            </select>
          </label>
          <label v-if="current" class="rename">名前（自動で判別できない場合は入力）
            <input v-model="nameDraft" class="ff-input" :placeholder="current.id" maxlength="40" @change="renameCharacter(current.id, nameDraft).catch((e) => (message = e.message))" />
          </label>
          <div v-if="current" class="del-row">
            <button type="button" class="ff-btn danger" :disabled="loading || state.refreshing" @click="askDelete">このキャラクターのログを削除</button>
          </div>
        </section>
      </template>
      <section>
        <h2>ログの場所</h2>
        <form @submit.prevent="load()">
          <input v-model="path" class="ff-input" aria-label="ログのフォルダーまたはファイル" />
          <div class="row">
            <button type="button" class="ff-btn" @click="pick('folder')">フォルダー</button>
            <button type="button" class="ff-btn" @click="pick('file')">ファイル</button>
            <button class="ff-btn primary" :disabled="loading || state.dict === 'downloading'">再読み込み</button>
          </div>
        </form>
      </section>
      <section>
        <h2>表示設定</h2>
        <label class="check"><input type="checkbox" :checked="state.preferences.animations" :disabled="preferencesBusy" @change="savePreferences({ animations: $event.target.checked }, $event)" /> アニメーションを有効にする</label>
      </section>
      <section class="end">
        <button type="button" class="link-btn" @click="menu = false; aboutDlg.showModal()">Aethelog について</button>
      </section>
    </div>

    <div v-if="loading" class="gauge" role="progressbar" :aria-valuenow="percent">
      <div :style="{ width: percent + '%' }" /><span>{{ state.phase === 'copy' ? 'ログを保存中…' : '読み込み中…' }} {{ state.progress[0] }} / {{ state.progress[1] }}</span>
    </div>
    <p v-if="message || state.error" class="notice err">{{ message || state.error }}</p>
    <p v-if="state.dict === 'downloading'" class="notice">定型文などの辞書をダウンロード中です。終わるとログを表示します。</p>
    <p v-else-if="state.dict === 'error'" class="notice err">辞書をダウンロードできなかったため、定型文などは仮の表示です（次回の起動時にもう一度試します）: {{ state.dict_error }}</p>

    <dialog v-if="view.slow" ref="waitDlg" class="wait-pop" aria-live="polite" @cancel.prevent="cancelLoad">
      <span class="spinner" aria-hidden="true" />
      <strong>ログを読み込み中…</strong>
      <small>絞り込みに時間がかかっています</small>
      <button type="button" class="ff-btn" @click="cancelLoad">中止</button>
    </dialog>

    <dialog ref="delDlg" class="about-pop del-pop">
      <template v-if="current">
        <template v-if="delStep === 1">
          <h2>ログの削除</h2>
          <p>「{{ current.name || current.id }}」のアーカイブ済みログを削除します。</p>
          <p>削除するのは、このアプリが保存したコピーだけです。ゲームのフォルダーにある元のログは削除されません。</p>
          <div class="actions">
            <button type="button" class="ff-btn" @click="delDlg.close()">キャンセル</button>
            <button type="button" class="ff-btn danger" @click="delStep = 2">次へ</button>
          </div>
        </template>
        <template v-else>
          <h2>本当に削除しますか？</h2>
          <p>「{{ current.name || current.id }}」の {{ current.count.toLocaleString() }} 件が、アーカイブから消えます。元に戻せません。</p>
          <p>削除後は自動更新を停止します。元のログが残っている場合は、再読み込みや自動更新の再開で取り込み直されます。</p>
          <div class="actions">
            <button type="button" class="ff-btn" @click="delDlg.close()">やめる</button>
            <button type="button" class="ff-btn danger" @click="confirmDelete">完全に削除する</button>
          </div>
        </template>
      </template>
    </dialog>

    <div v-if="dragging" class="drop-hint" aria-hidden="true">ここにドロップして、ログを読み込みます</div>

    <dialog ref="aboutDlg" class="about-pop" @pointerdown="aboutClose.down" @click="aboutClose.click">
      <h2>Aethelog</h2>
      <p class="ver">バージョン {{ version }}</p>
      <p>FINAL FANTASY XIV のチャットログとシステムログを、読み取り専用で閲覧するツールです。</p>
      <h3>ライセンス</h3>
      <pre class="license">{{ license }}</pre>
      <p>この許諾はソースコードだけが対象です。表示するゲームの内容は含みません。</p>
      <h3>免責事項</h3>
      <p>非公式のファンメイドツールです。</p>
      <p>株式会社スクウェア・エニックスとは関係がなく、同社の承認も受けていません。</p>
      <p>FINAL FANTASY XIV © SQUARE ENIX</p>
      <div class="actions">
        <button type="button" class="ff-btn" @click="openNotices">Credits</button>
        <button type="button" class="ff-btn" @click="aboutDlg.close()">閉じる</button>
      </div>
    </dialog>

    <dialog ref="noticesDlg" class="notices-pop" @pointerdown="noticesClose.down" @click="noticesClose.click">
      <h2>Credits</h2>
      <ul v-if="notices && notices.packages.length" class="notice-list">
        <li v-for="(p, i) in notices.packages" :key="i" :class="{ open: openedPackage === i }">
          <div class="notice-row">
            <span class="pkg"><strong>{{ p.n }}</strong> <small>{{ p.v }}</small></span>
            <span class="lic">{{ p.l }}</span>
            <button type="button" class="link-btn" :aria-expanded="openedPackage === i" @click="toggleLicense(i)">ライセンスを表示</button>
            <button type="button" class="link-btn" :disabled="!p.h" @click="openHomepage(i)">ホームページ</button>
          </div>
          <pre v-if="openedPackage === i" class="notice-text">{{ licenseOf(p) }}</pre>
        </li>
      </ul>
      <p v-else class="notice-empty">一覧を読み込めませんでした。</p>
      <button type="button" class="ff-btn" @click="noticesDlg.close()">閉じる</button>
    </dialog>

    <ChatWindow v-if="meta" />
    <p v-else-if="!loading && state.dict !== 'downloading'" class="notice">「メニュー」からログの場所を指定してください。</p>

  </div>
</template>
