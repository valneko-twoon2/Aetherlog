<script setup>
import { computed } from 'vue'
import SeText from './SeText.vue'
import { openPage } from '../lodestone.js'

const props = defineProps({
  row: Object,
  channel: Object,
  query: String,
})

const time = computed(() => {
  // Log times are shown in JST regardless of the browser's time zone.
  const d = new Date((props.row.t + 9 * 3600) * 1000)
  const p = (n) => String(n).padStart(2, '0')
  return `${p(d.getUTCHours())}:${p(d.getUTCMinutes())}:${p(d.getUTCSeconds())}`
})

// Kinds where the sender is a player; an unlinked name there is your own character.
const PLAYER_KINDS = new Set([10, 11, 13, 14, 15, 24, 27, 30, 37, ...Array.from({ length: 8 }, (_, i) => 16 + i), ...Array.from({ length: 7 }, (_, i) => 101 + i)])
const NOISE = new RegExp(String.raw`[\ue000-\uf8ff\s]`, 'g')

const GLYPHS = new RegExp(String.raw`[\ue000-\uf8ff]`, 'g')
const VISIBLE = new RegExp(String.raw`[^\ue000-\uf8ff\s]`)

const mine = computed(() => {
  const who = props.row.who
  return PLAYER_KINDS.has(props.row.k) && who.length > 0 && !who.some((s) => s.l) && who.some((s) => s.s && VISIBLE.test(s.s))
})

// Name and world of a linked sender, or null (system lines, NPCs, your own name).
const linked = computed(() => mine.value || props.row.who.some((s) => s.l === 1))
const playerOf = () => {
  let name = ''
  let world = ''
  if (mine.value) {
    // Your own name is plain text without a world.
    name = props.row.who.map((s) => s.s || '').join('').replace(NOISE, ' ').trim()
    return name ? { name, world } : null
  }
  for (const s of props.row.who) {
    if (s.l === 1) name += s.s
    else if (name && s.s) world += s.s.replace(NOISE, '')
  }
  // Game-font symbols (party numbers and the like) sometimes sit inside the link.
  name = name.replace(GLYPHS, '').trim()
  return name ? { name, world } : null
}

const click = (e) => {
  const p = linked.value && (e.ctrlKey || e.metaKey) && playerOf()
  if (p) openPage(p)
}

// Split a layout such as "({n}) {m}" into literal text and the two slots.
const pieces = computed(() => {
  const layout = props.row.who.length ? props.channel.layout : '{m}'
  return layout.split(/(\{n\}|\{m\})/).filter(Boolean)
})
</script>

<template>
  <div class="line" :class="{ warn: row.w, mine }" :style="{ '--channel-color': channel.color }">
    <span class="ts">[{{ time }}]</span>
    <span class="text">
      <template v-for="(piece, i) in pieces" :key="i">
        <span v-if="piece === '{n}'" class="who" :class="{ mine, player: linked }" @click="click">
          <SeText :spans="row.who" :query="query" />
        </span>
        <SeText v-else-if="piece === '{m}'" :spans="row.msg" :query="query" />
        <template v-else>{{ piece }}</template>
      </template>
    </span>
  </div>
</template>
