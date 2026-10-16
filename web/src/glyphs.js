// FFXIV private-use glyphs (U+E0xx) and bitmap-font icons, drawn with CSS/Unicode
// stand-ins: the game's own font cannot be redistributed.
const glyphs = new Map()
const set = (code, glyph) => glyphs.set(code, glyph)

for (let i = 0; i < 10; i++) set(0xe060 + i, { t: String(i), c: 'digit' })
for (let i = 0; i < 32; i++) set(0xe08f + i, { t: String(i), c: 'box' })
for (let i = 0; i < 26; i++) set(0xe071 + i, { t: String.fromCharCode(65 + i), c: 'box' })
for (let i = 0; i < 10; i++) set(0xe0e0 + i, { t: String(i), c: 'box solid' })
for (let i = 1; i <= 9; i++) set(0xe0b0 + i, { t: String(i), c: 'box inst' })
;['I', 'II', 'III', 'IV', 'V', 'VI'].forEach((t, i) => set(0xe0c1 + i, { t, c: 'box' }))
Object.entries({
  0xe020: 'あ', 0xe021: 'ア', 0xe022: 'A', 0xe023: 'ｱ', 0xe024: 'a',
  0xe040: { t: '⇒', c: 'at-bracket' }, 0xe041: { t: '⇐', c: 'at-bracket' },
  0xe031: '⏱', 0xe033: 'IL', 0xe034: '🌱', 0xe03a: 'EL', 0xe03b: { t: '投', c: 'box' },
  0xe04e: { t: '染', c: 'box' }, 0xe03c: { t: 'HQ', c: 'box hq' }, 0xe03d: { t: '◈', c: 'plain' },
  0xe03e: '⚄', 0xe042: '⬡', 0xe043: '⊘', 0xe048: 'ヒ', 0xe049: { t: 'ギル', c: 'box' },
  0xe04a: '○', 0xe04b: '□', 0xe04c: '✕', 0xe04d: '△',
  0xe05b: { t: '－', c: 'box debuff' }, 0xe05c: { t: '＋', c: 'box buff' }, 0xe05d: '✈',
  0xe06a: 'Lv', 0xe06b: 'Lv', 0xe06c: 'Niv', 0xe06d: 'AM', 0xe06e: 'PM', 0xe06f: { t: '→', c: 'arrow' },
  0xe035: '▼', 0xe070: { t: '?', c: 'box' }, 0xe0af: { t: '+', c: 'box' }, 0xe0c0: { t: '★', c: 'box' },
  0xe0bb: { t: '◈', c: 'linkmark' }, 0xe0bc: { t: 'EXP', c: 'plain' }, 0xe0bd: { t: 'EXP', c: 'plain' },
  0xe0be: '⇄', 0xe0bf: '↻', 0xe0ba: { t: '＊', c: 'box inst' },
}).forEach(([k, v]) => set(Number(k), typeof v === 'string' ? { t: v, c: 'plain' } : v))
for (let i = 0; i < 4; i++) {
  ;['LT', 'ST', 'ET'].forEach((t, j) => set(0xe0d0 + i * 3 + j, { t, c: 'box time' }))
}

export function glyphFor(ch) {
  const code = ch.codePointAt(0)
  if (code < 0xe000 || code > 0xf8ff) return null
  return glyphs.get(code) || { t: '▯', c: 'unknown', title: `U+${code.toString(16).toUpperCase()}` }
}

const icons = {
  CrossWorld: { t: '✈', c: 'crossworld' },
  DamagePhysical: { t: '⚔', c: 'dmg-phys' },
  DamageMagical: { t: '✦', c: 'dmg-magic' },
  DamageSpecial: { t: '◆', c: 'dmg-special' },
  Tank: { t: '🛡', c: 'role-tank' },
  Healer: { t: '✚', c: 'role-healer' },
  DPS: { t: '⚔', c: 'role-dps' },
  Alarm: { t: '⏰', c: 'plain' },
  Warning: { t: '⚠', c: 'plain' },
  ArrowUp: { t: '▲', c: 'plain' },
  ArrowDown: { t: '▼', c: 'plain' },
}

export const iconFor = (name) => icons[name] || { t: '◇', c: 'plain', title: name }
