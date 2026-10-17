// Takes the README screenshots (docs/images/main.png, menu.png, battle.png) from a built exe.
// Run it through tools\make_screenshots.ps1, which prepares the demo logs and a scratch data folder.
//
// The app is started with the WebView2 debugging port open. This script clicks the tabs and the
// menu through that port and calls tools\capture_window.ps1 for each picture.
//   node make_screenshots.mjs <exe> <logs> <data> <outDir>
import { spawn, spawnSync } from 'node:child_process'
import { mkdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

if (typeof WebSocket === 'undefined') throw new Error('Node.js 22 or newer is required (global WebSocket).')
const [exe, logs, data, outDir] = process.argv.slice(2)
if (!outDir) throw new Error('usage: node make_screenshots.mjs <exe> <logs> <data> <outDir>')
const capture = join(resolve(dirname(fileURLToPath(import.meta.url))), 'capture_window.ps1')
const PORT = 9333
mkdirSync(outDir, { recursive: true })
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

// AETHELOG_LOGS / AETHELOG_DATA keep the demo logs and names apart from the real ones.
const app = spawn(exe, [], {
  env: { ...process.env, AETHELOG_LOGS: logs, AETHELOG_DATA: data, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
  stdio: 'ignore',
})
const stopApp = () => spawnSync('taskkill', ['/PID', String(app.pid), '/T', '/F'], { stdio: 'ignore' })
process.on('exit', stopApp)

let socket
let nextId = 1
const pending = new Map()
async function connect() {
  for (let i = 0; i < 120; i++) {
    try {
      const targets = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json()
      const page = targets.find((t) => t.type === 'page')
      if (page) {
        socket = new WebSocket(page.webSocketDebuggerUrl)
        await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject })
        socket.onmessage = (m) => {
          const msg = JSON.parse(m.data)
          pending.get(msg.id)?.(msg)
          pending.delete(msg.id)
        }
        return
      }
    } catch { /* the app is still starting */ }
    await sleep(500)
  }
  throw new Error('The app did not open its debugging port.')
}

const evaluate = (expression) => new Promise((resolve, reject) => {
  const id = nextId++
  pending.set(id, (msg) => (msg.error || msg.result?.exceptionDetails ? reject(new Error(JSON.stringify(msg))) : resolve(msg.result.result.value)))
  socket.send(JSON.stringify({ id, method: 'Runtime.evaluate', params: { expression, returnByValue: true, awaitPromise: true } }))
})

async function waitFor(expression, what, ms = 120_000) {
  const end = Date.now() + ms
  while (Date.now() < end) {
    if (await evaluate(expression).catch(() => false)) return
    await sleep(300)
  }
  throw new Error(`Timed out waiting for ${what}.`)
}

// Clicks the first element of `selector` whose text starts with `text`.
async function click(selector, text) {
  const found = await evaluate(`(() => {
    const el = [...document.querySelectorAll(${JSON.stringify(selector)})].find((e) => e.textContent.trim().startsWith(${JSON.stringify(text)}))
    if (!el) return false
    el.click()
    return true
  })()`)
  if (!found) throw new Error(`Not found: ${selector} "${text}"`)
}

function shoot(name, resize = false) {
  const file = join(outDir, `${name}.png`)
  const args = ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', capture, '-ProcessId', String(app.pid), '-Out', file]
  if (resize) args.push('-Resize')
  const result = spawnSync('powershell', args, { encoding: 'utf8' })
  if (result.status !== 0) throw new Error(`capture failed: ${result.stderr || result.stdout}`)
  console.log(`${name}.png  ${result.stdout.trim()}`)
}

try {
  await connect()
  // The dictionary is downloaded first on a fresh data folder, then the logs are read.
  const rows = `document.querySelectorAll('.vrow').length > 5 && !document.querySelector('.gauge')`
  await waitFor(rows, 'the log lines')
  shoot('_resize', true)
  await sleep(1500)
  await waitFor(rows, 'the log lines after resizing')
  await sleep(1500)

  shoot('main')

  await click('button', 'メニュー')
  await sleep(900)
  shoot('menu')
  await click('button', 'メニュー')
  await sleep(500)

  await click('button[role=tab]', '戦闘')
  await sleep(1500)
  shoot('battle')
} finally {
  stopApp()
}
