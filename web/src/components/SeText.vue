<script setup>
import { computed } from 'vue'
import { glyphFor, iconFor } from '../glyphs.js'

const props = defineProps({ spans: { type: Array, default: () => [] }, query: { type: String, default: '' } })

const PIECE = /([-])|(<se\.\d+>)/g

function textStyle(span) {
  const style = {}
  if (span.c) style['--span-color'] = span.c
  if (span.e) style.textShadow = `0 0 2px ${span.e}, 0 0 4px ${span.e}, 1px 1px 1px #000`
  if (span.b) style.fontWeight = '700'
  if (span.i) style.fontStyle = 'italic'
  return style
}

function plainRuns(text, query) {
  if (!query) return [{ text }]
  const runs = []
  const lower = text.toLowerCase()
  const needle = query.toLowerCase()
  let at = 0
  for (let hit = lower.indexOf(needle); hit >= 0; hit = lower.indexOf(needle, at)) {
    if (hit > at) runs.push({ text: text.slice(at, hit) })
    runs.push({ text: text.slice(hit, hit + needle.length), hit: true })
    at = hit + needle.length
  }
  if (at < text.length) runs.push({ text: text.slice(at) })
  return runs
}

const parts = computed(() => {
  const out = []
  for (const span of props.spans) {
    if (span.k === 'icon') {
      out.push({ type: 'glyph', glyph: iconFor(span.a), style: {} })
    } else if (span.k === 'at') {
      out.push({ type: 'at', text: span.s, unresolved: !!span.u })
    } else if (span.s === '\n') {
      out.push({ type: 'br' })
    } else {
      const style = textStyle(span)
      const cls = span.l ? `link link-${span.l}` : ''
      const push = (text) => out.push(...plainRuns(text, props.query).map((r) => ({ type: 'text', style, cls, ...r })))
      let last = 0
      for (const m of span.s.matchAll(PIECE)) {
        if (m.index > last) push(span.s.slice(last, m.index))
        if (m[1]) out.push({ type: 'glyph', glyph: glyphFor(m[1]), style })
        else out.push({ type: 'se', text: m[2].replace(/\D/g, ''), style })
        last = m.index + m[0].length
      }
      if (last < span.s.length) push(span.s.slice(last))
    }
  }
  return out
})
</script>

<template>
  <span class="se">
    <template v-for="(p, i) in parts" :key="i">
      <br v-if="p.type === 'br'" />
      <span v-else-if="p.type === 'at'" class="at" :class="{ unresolved: p.unresolved }">
        <i class="at-bracket">⇒</i>{{ p.text }}<i class="at-bracket">⇐</i>
      </span>
      <span v-else-if="p.type === 'glyph'" class="gl" :class="p.glyph.c" :style="p.style" :title="p.glyph.title">{{ p.glyph.t }}</span>
      <span v-else-if="p.type === 'se'" class="gl se-sound" :style="p.style" :title="`サウンドエフェクト ${p.text}`">♪{{ p.text }}</span>
      <mark v-else-if="p.hit" :class="p.cls" :style="p.style">{{ p.text }}</mark>
      <span v-else :class="p.cls" :style="p.style">{{ p.text }}</span>
    </template>
  </span>
</template>
