---
name: ffxiv-log-format
description: Use when working with FFXIV binary chat logs (.log), including the file layout, SeString control sequences (0x02..0x03), auto-translate phrases, bitmap icons and private-use glyphs, and chat channel kinds (low 7 bits of the code). Consult it when implementing, fixing or verifying log parsing, and to avoid re-analysing the logs. For the tool's layout, build steps and entry files use the aethelog-tool skill.
---

# FFXIV binary log format

Verified against real logs (450,000 lines, 463 files). **Do not re-analyse the game logs; start here.** This skill covers the format only; where the code lives and how the tool is built is in the `aethelog-tool` skill. If the implementation disagrees with this note, the implementation is right.

## 1. File and record layout

- First 8 bytes: LE u32 `first` and `last` (record numbers, `last` exclusive). `count = last - first`.
- Next `count * 4` bytes: the **cumulative end offset** of each record (u32 LE, cumulative, not deltas).
- The body starts at `8 + count * 4`. Record i is `body[prev_end:end_i]`.
- Anything after the last offset is **stale data from earlier writes**. Ignore it (report it as trailing bytes).
- Record: `unix seconds (u32) + code (u32) + 0x1F + sender + 0x1F + message`.
- Channel kind = `code & 0x7F`. The upper bits are target/source flags and are **not** part of the kind name (keep the raw code separately).
- The 0x1F separator also occurs inside link payloads, so locate it while skipping each `0x02 .. 0x03` control sequence.
- Location: `<character>/log/*.log`; the character is the grandparent folder name of the file.

## 2. SeString

Control sequence: `0x02 kind packed_len payload 0x03` (payload length = `packed_len`).

- **Never frame on the closing 0x03 alone**; 0x03 can occur inside a payload. Always read by length.
- Packed int:
  - marker `1..0xCF` gives the value `marker - 1`.
  - marker `0xF0..0xFE`: the following bytes are selected by the bit mask `marker + 1` (bits 8, 4, 2, 1 = most to least significant) and form the value from raw bytes (RGB and so on).
  - `0xE9` (global colour / gnum) and `0xEC` (reset the colour stack) are expressions. Only `0xEC` (reset) is interpreted; other expressions are ignored.
- Plain text writes auto-translate phrases as `⇒phrase⇐`, keeps newlines and drops control characters (< 0x20).

| Code | Name | Handling |
|---|---|---|
| 0x10 | newline | `\n` |
| 0x12 | icon (BitmapFontIcon id) | 88 = CrossWorld, 117/118/119 = physical/magical/special damage |
| 0x13 | text colour | `F6` + 3 RGB bytes sets it, `EC` resets, `E9 xx` is a global colour and is ignored |
| 0x14 / 0x15 | edge colour / shadow colour | edge is read like the text colour, shadow is ignored |
| 0x19 / 0x1A | bold / italic | value 1 turns it on |
| 0x1D | NBSP | space |
| 0x1F | hard hyphen | `-` |
| 0x27 | link | first byte is the type: 1 player, 3 item, 6 achievement, 8 Party Finder, 9 status, 0xCF = end. **The displayed text follows the link as ordinary text, not inside it** |
| 0x2E | auto-translate | chapter 3 |
| 0x48 / 0x49 | ColorType / EdgeColorType | UIColor row id (0 = reset); 500/501 are the orange/black link markers |
| ignored | 0x16, 0x17, 0x18, 0x1B, 0x1C, 0x1E (Icon2) | appearance only / unsupported |
| other | | shown as `[control:XX]` |

## 3. Auto-translate (0x2E)

Payload = `[group: 1 raw byte][key: packed int]`. **The group is not packed**; reading it as packed shifts everything by one.

| Group | Table | Meaning of key |
|---|---|---|
| < 49 | Completion sheet `text` | the **`#` row id**, not the Key column |
| >= 49 | lookup table for that group | the row id in that table |

- Examples: group 2, key 227 is "よろしくお願いします！"; group 15, key 1258 is "不滅隊"; group 24, key 2045 is a content phrase.
- Lookup tables: 49 Mount, 50 ClassJob, 51 PlaceName, 56 Action, 58 BuddyAction, 63 PvP Action and others (49 to 72).
- Resolution: if the group has a lookup table, use the row id there; otherwise use the Completion `#` with the key.
- Pitfalls: mistaking the key for the Key column, and reading the group as packed.

## 4. Private-use glyphs (U+E0xx, appear raw in text)

In the log they are plain UTF-8 private-use characters that depend on the game's own font, so a viewer must substitute other symbols.

| Code point | Meaning |
|---|---|
| E040 / E041 | auto-translate open/close brackets (like ⇒⇐) |
| E05B / E05C | debuff / buff |
| E05D | cross-world |
| E06F | arrow |
| E0BB | link marker |
| E090-E097 | circled numbers (party member number) |
| E071-E08A | boxed A-Z |
| E0E0-E0E9 | numbers with an edge |

- Sender names: the name text follows the link payload; the world name follows the CrossWorld icon (0x12 / 88).
- Auto-translate phrases are surrounded by E040/E041 around the 0x2E sequence. Plain text replaces them with `⇒⇐`, but E040/E041 may remain in the text.

## 5. Channel kinds (`code & 0x7F`)

**Never guess a name for an unknown kind; show `kind XX` (hex).** About 90% of real log lines are battle kinds 41-49.

| Kind (decimal) | Content | Group |
|---|---|---|
| 10 Say, 11 Shout, 12/13 Tell sent/received, 14 Party, 15 Alliance, 30 Yell | chat | chat |
| 16-23 | Linkshell 1-8 (`16 + n`) | chat |
| 24 FC, 27 Beginner, 28/29 emotes, 32 cross-world party, 36 PvP team, 37 CWLS1, 56 Echo | chat | chat |
| 101-107 | CWLS 2-8 (`99 + n`) | chat |
| 3 notice, 54 glamour, 55 alarm, 57 system, 58 battle system, 59 gathering system, 60 error | system | system |
| 69-79 FC announcements/logins, market sales, recruitment notices, markers, dice, orchestrion, contacts and so on | system | system |
| 61 NPC dialogue, 62 loot, 64 progress/learning, 65 lot, 66 crafting, 67 gathering, 68 NPC announcements | event | event |
| 41 damage, 42 miss, 43 cast, 44 item, 45 HP recovery, 46 buff applied, 47 debuff applied, 48 buff removed, 49 debuff removed | battle | battle |

Note: this table is **decimal**. Where hex is used (for example `0E` = 14 = Party) convert. Names, colours and line layouts live in the implementation's channel catalog (see the `aethelog-tool` skill), which is authoritative.

## 6. Pitfalls and verification

- Searching raw bytes with a regex or substring for `02 2E` and the like gives many false positives (accidental matches inside payloads and text). **Tokenise the control sequences first**, then count.
- The Windows console (cp932) garbles Japanese; set UTF-8 explicitly when printing results.
- Do not mix the upper bits into the kind: use `code & 0x7F`.
- Do not search for the 0x1F sender/message separator naively; it occurs inside link payloads.
- Verification metrics: number of read errors, number of text warnings, number of files with data after the offset table, and counts per kind.
- Parsing is read-only; never modify the original logs.
- **The log never contains your own character name.** Senders on the same world have no world name appended (other worlds have the world name concatenated after the name), and a private-use party-number glyph may precede a name. Any name inference rests on these facts and stays a guess.

## 7. Unknown or unsupported

- Icon2 (0x1E) is ignored, so those icons are missing.
- Link internals (world id, item id / HQ, the player's server and so on) are not parsed; only the type byte is used.
- Colour expressions such as `E9` (global colour / gnum) and other expressions are ignored.
- Private-use glyphs: the meaning of some, such as `E057`, is unidentified (very rare in real logs).
- Names of unknown channel kinds are undetermined.
- De-duplicating the same conversation across characters, battle analysis such as DPS, and recovering overwritten logs are out of scope.
