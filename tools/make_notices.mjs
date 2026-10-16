// Writes THIRD-PARTY-NOTICES.txt: the licence texts of everything that ends up in the exe
// (Rust crates for Windows, and the npm packages the front end bundles).
// Run it through tools\make_notices.ps1; tools\publish.ps1 does so before every build.
import { execFileSync, execSync } from 'node:child_process'
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const options = (cwd) => ({ cwd, maxBuffer: 1 << 29, encoding: 'utf8' })

const packages = [] // { name, version, license, dir }

// Rust: the runtime dependency closure of the app for the Windows build.
const meta = JSON.parse(
  execFileSync('cargo', ['metadata', '--format-version', '1', '--filter-platform', 'x86_64-pc-windows-msvc'], options(join(root, 'src-tauri'))),
)
const byId = new Map(meta.packages.map((p) => [p.id, p]))
const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]))
const seen = new Set()
const stack = [meta.resolve.root]
while (stack.length) {
  const id = stack.pop()
  if (seen.has(id)) continue
  seen.add(id)
  for (const dep of nodes.get(id).deps) {
    if (dep.dep_kinds.some((k) => k.kind === null || k.kind === 'normal')) stack.push(dep.pkg)
  }
}
for (const id of seen) {
  if (id === meta.resolve.root) continue
  const p = byId.get(id)
  packages.push({ name: p.name, version: p.version, license: p.license || '', repository: p.repository || '', dir: dirname(p.manifest_path) })
}

// npm: production dependencies of the front end (what vite bundles).
const tree = JSON.parse(execSync('npm ls --omit=dev --all --long --json', options(join(root, 'web'))))
const dirs = new Set()
const walk = (deps = {}) => {
  for (const dep of Object.values(deps)) {
    if (dep.path && !dirs.has(dep.path)) {
      dirs.add(dep.path)
      const pkg = JSON.parse(readFileSync(join(dep.path, 'package.json'), 'utf8'))
      const license = typeof pkg.license === 'string' ? pkg.license : pkg.license?.type || ''
      const repo = (typeof pkg.repository === 'string' ? pkg.repository : pkg.repository?.url || '').replace(/^git\+|\.git$/g, '')
      packages.push({ name: pkg.name, version: pkg.version, license, repository: repo, dir: dep.path })
    }
    walk(dep.dependencies)
  }
}
walk(tree.dependencies)

// The same crate can be reached twice; list it once.
const unique = new Map(packages.map((p) => [`${p.name}@${p.version}`, p]))
packages.length = 0
packages.push(...unique.values())

const wanted = /^(licen[sc]e|copying|unlicense|notice|copyright)([-_.].*)?$/i
const texts = new Map() // normalised text -> { text, items }
const byRepo = new Map() // repository -> licence texts of its packages
const missing = []
for (const p of packages.sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version))) {
  const files = existsSync(p.dir)
    ? readdirSync(p.dir).filter((f) => wanted.test(f) && !/\.spdx$/i.test(f) && statSync(join(p.dir, f)).isFile() && statSync(join(p.dir, f)).size < 200_000)
    : []
  if (!files.length) missing.push(p)
  for (const file of files.sort()) {
    const text = readFileSync(join(p.dir, file), 'utf8').replace(/^﻿/, '').replace(/\r\n?/g, '\n').trim()
    if (!text) continue
    if (!texts.has(text)) texts.set(text, { text, items: [] })
    const item = `${p.name} ${p.version}${p.license ? ` (${p.license})` : ''}`
    const entry = texts.get(text)
    if (!entry.items.includes(item)) entry.items.push(item)
    if (p.repository) byRepo.set(p.repository, new Set([...(byRepo.get(p.repository) || []), entry]))
  }
}

// A package without its own licence file is listed under the texts of a sibling from the same
// repository, or under the shared MPL-2.0 text; the rest are named with the licence they declare.
const stillMissing = []
for (const p of missing) {
  const item = `${p.name} ${p.version}${p.license ? ` (${p.license})` : ''}`
  const siblings = p.repository && byRepo.get(p.repository)
  const mpl = p.license === 'MPL-2.0' && [...texts.values()].find((t) => t.text.includes('Mozilla Public License Version 2.0'))
  const targets = siblings ? [...siblings] : mpl ? [mpl] : []
  if (targets.length) targets.forEach((t) => t.items.includes(item) || t.items.push(item))
  else stillMissing.push(p)
}

const bar = '='.repeat(78)
let out = `Aetherlog third-party notices

Aetherlog contains the open-source components listed below. Each licence text
is printed once, followed by the packages that come with it.
${bar}
`
const sections = [...texts.values()].sort((a, b) => b.items.length - a.items.length || a.items[0].localeCompare(b.items[0]))
for (const s of sections) out += `\n${s.items.map((i) => `  ${i}`).join('\n')}\n\n${s.text}\n\n${bar}\n`
if (stillMissing.length) {
  out += `\nPackages that ship no licence file (licence as declared in their metadata):\n\n`
  out += stillMissing.map((p) => `  ${p.name} ${p.version} (${p.license || "unspecified"}) ${p.repository}`.trimEnd()).join('\n') + '\n'
}
writeFileSync(join(root, 'THIRD-PARTY-NOTICES.txt'), out)
console.log(`THIRD-PARTY-NOTICES.txt: ${packages.length} packages, ${sections.length} licence texts, ${stillMissing.length} without a licence file`)
