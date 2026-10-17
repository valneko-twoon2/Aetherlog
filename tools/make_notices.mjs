// Writes the third-party licence notices of everything that ends up in the exe (Rust crates for
// Windows, and the npm packages the front end bundles):
//   THIRD-PARTY-NOTICES.txt   shipped next to the exe, each licence text printed once
//   src-tauri/notices.json    compiled into the exe for the licence window: packages with
//                             their homepage and indexes into the shared list of texts
// Run it through tools\make_notices.ps1; tools\publish.ps1 does so before every build.
// With --check nothing is written: it exits 1 when the files are out of date (.githooks/pre-commit).
import { execFileSync, execSync } from 'node:child_process'
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const check = process.argv.includes('--check')
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const options = (cwd) => ({ cwd, maxBuffer: 1 << 29, encoding: 'utf8' })

// Only https addresses are ever handed to the browser.
const webAddress = (url) => {
  let u = String(url || '').trim().replace(/^git\+/, '').replace(/\.git$/, '')
  u = u.replace(/^(git|ssh):\/\/(git@)?/, 'https://').replace(/^git@github\.com:/, 'https://github.com/').replace(/^github:/, 'https://github.com/')
  if (/^[\w.-]+\/[\w.-]+$/.test(u)) u = `https://github.com/${u}`
  return u.startsWith('https://') ? u : ''
}

const packages = [] // { name, version, license, repository, homepage, dir }

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
  const repository = webAddress(p.repository)
  packages.push({ name: p.name, version: p.version, license: p.license || '', repository, homepage: webAddress(p.homepage) || repository, dir: dirname(p.manifest_path) })
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
      const repository = webAddress(typeof pkg.repository === 'string' ? pkg.repository : pkg.repository?.url)
      packages.push({ name: pkg.name, version: pkg.version, license, repository, homepage: webAddress(pkg.homepage) || repository, dir: dep.path })
    }
    walk(dep.dependencies)
  }
}
walk(tree.dependencies)

// The same crate can be reached twice; list it once.
const unique = new Map(packages.map((p) => [`${p.name}@${p.version}`, p]))
packages.length = 0
packages.push(...unique.values())
packages.sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()) || a.version.localeCompare(b.version, 'en', { numeric: true }))

const wanted = /^(licen[sc]e|copying|unlicense|notice|copyright)([-_.].*)?$/i
const texts = [] // distinct licence texts
const textIndex = new Map()
const intern = (text) => {
  if (!textIndex.has(text)) textIndex.set(text, texts.push(text) - 1)
  return textIndex.get(text)
}
const used = new Map() // package -> Set of text indexes
const byRepo = new Map() // repository -> texts of its packages
for (const p of packages) {
  const files = existsSync(p.dir)
    ? readdirSync(p.dir).filter((f) => wanted.test(f) && !/\.spdx$/i.test(f) && statSync(join(p.dir, f)).isFile() && statSync(join(p.dir, f)).size < 200_000)
    : []
  const own = new Set()
  for (const file of files.sort()) {
    const text = readFileSync(join(p.dir, file), 'utf8').replace(/^﻿/, '').replace(/\r\n?/g, '\n').trim()
    if (text) own.add(intern(text))
  }
  used.set(p, own)
  if (own.size && p.repository) byRepo.set(p.repository, new Set([...(byRepo.get(p.repository) || []), ...own]))
}

// A package without its own licence file borrows the texts of a sibling from the same repository,
// or the shared MPL-2.0 text; the rest are listed with the licence they declare.
const mplText = texts.findIndex((t) => t.includes('Mozilla Public License Version 2.0'))
const withoutText = []
for (const p of packages) {
  const own = used.get(p)
  if (own.size) continue
  const borrowed = (p.repository && byRepo.get(p.repository)) || (p.license === 'MPL-2.0' && mplText >= 0 ? [mplText] : [])
  if (borrowed.size || borrowed.length) borrowed.forEach((i) => own.add(i))
  else withoutText.push(p)
}

// ---- THIRD-PARTY-NOTICES.txt: each text once, with the packages that come with it.
const label = (p) => `${p.name} ${p.version}${p.license ? ` (${p.license})` : ''}`
const bar = '='.repeat(78)
let out = `Aethelog third-party notices

Aethelog contains the open-source components listed below. Each licence text
is printed once, followed by the packages that come with it.
${bar}
`
const sections = texts.map((text, index) => ({ text, items: packages.filter((p) => used.get(p).has(index)).map(label) }))
sections.sort((a, b) => b.items.length - a.items.length || a.items[0].localeCompare(b.items[0]))
for (const s of sections) out += `\n${s.items.map((i) => `  ${i}`).join('\n')}\n\n${s.text}\n\n${bar}\n`
if (withoutText.length) {
  out += `\nPackages that ship no licence file (licence as declared in their metadata):\n\n`
  out += withoutText.map((p) => `  ${label(p)} ${p.repository}`.trimEnd()).join('\n') + '\n'
}
const outdated = []
const write = (path, content) => {
  const file = join(root, path)
  const current = existsSync(file) ? readFileSync(file, 'utf8').replaceAll('\r\n', '\n') : null
  if (current === content) return
  if (check) outdated.push(path)
  else writeFileSync(file, content)
}
write('THIRD-PARTY-NOTICES.txt', out)

// ---- src-tauri/notices.json: what the licence window lists.
const json = {
  packages: packages.map((p) => ({ n: p.name, v: p.version, l: p.license, h: p.homepage, t: [...used.get(p)].sort((a, b) => a - b) })),
  texts,
}
write('src-tauri/notices.json', JSON.stringify(json))
if (outdated.length) {
  console.error(`Out of date: ${outdated.join(', ')}\nThe dependencies changed. Run tools/make_notices.ps1 and stage both files.`)
  process.exit(1)
}
console.log(`${packages.length} packages, ${texts.length} licence texts, ${withoutText.length} without a licence file${check ? ' (up to date)' : ''}`)
