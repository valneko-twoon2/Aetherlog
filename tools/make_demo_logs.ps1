<#
.SYNOPSIS
  Writes a small, entirely fictional FFXIV-style log folder for screenshots and demos.

.DESCRIPTION
  Every character name below is invented. The files follow the real binary layout
  (header, cumulative end offsets, records with SeString control sequences), so the
  app renders them exactly like real logs.

  powershell -File tools\make_demo_logs.ps1 [-OutDir docs\demo-logs]
#>
param(
    # Where the character folder is created (default: docs\demo-logs).
    [string]$OutDir
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) { $OutDir = Join-Path $root 'docs\demo-logs' }

$Folder = 'FFXIV_CHR00400000DEC0DE01'   # a made-up character folder (valid hex id)
$Self = 'Lumi Starfall'
$World = 'Ifrit'
$utf8 = New-Object System.Text.UTF8Encoding($false)

# Chat kinds (low 7 bits of the code).
$SAY = 10; $SHOUT = 11; $TELL_OUT = 12; $PARTY = 14; $FC = 24; $LS1 = 16; $EMOTE = 29; $SYSTEM = 57; $LOOT = 62; $PROGRESS = 64
$DAMAGE = 41; $CAST = 43; $HEAL = 45; $BUFF = 46

# Concatenates byte arrays.
function Join-Bytes {
    $m = New-Object System.IO.MemoryStream
    foreach ($a in $args) { if ($a -and $a.Length) { $m.Write($a, 0, $a.Length) } }
    , $m.ToArray()
}

function Text([string]$value) { , $utf8.GetBytes($value) }

# The game's packed integer.
function Packed([int]$value) {
    if ($value -lt 0xCF) { return , ([byte[]]@($value + 1)) }
    $parts = @((($value -shr 24) -band 255), (($value -shr 16) -band 255), (($value -shr 8) -band 255), ($value -band 255))
    $mask = 0
    $bits = 8, 4, 2, 1
    $body = New-Object System.Collections.Generic.List[byte]
    for ($i = 0; $i -lt 4; $i++) {
        if ($parts[$i]) { $mask += $bits[$i]; $body.Add([byte]$parts[$i]) }
    }
    , (Join-Bytes ([byte[]]@(0xEF + $mask)) $body.ToArray())
}

function Macro([int]$kind, [byte[]]$payload = @()) {
    , (Join-Bytes ([byte[]]@(2, $kind)) (Packed $payload.Length) $payload ([byte[]]@(3)))
}

function Color([int[]]$rgb) { , (Macro 0x13 ([byte[]]@(0xF6, $rgb[0], $rgb[1], $rgb[2]))) }   # no component may be 0
function ColorReset { , (Macro 0x13 ([byte[]]@(0xEC))) }

# Auto-translate phrase: a bare group byte, then the packed key.
function Phrase([int]$group, [int]$key) { , (Macro 0x2E (Join-Bytes ([byte[]]@($group)) (Packed $key))) }

function Glyph([int]$number) { , (Text ([char]::ConvertFromUtf32(0xE090 + $number - 1))) }

# Sender field: optional party-number glyph, link around the name, world after a cross-world icon.
function Player([string]$name, [string]$world, [int]$number = 0) {
    $out = [byte[]]@()
    if ($number) { $out = Glyph $number }
    $out = Join-Bytes $out (Macro 0x27 ([byte[]]@(1, 0x11, 0x46, 1, 0xFF, 1))) (Text $name) (Macro 0x27 ([byte[]]@(0xCF, 1, 1, 1, 0xFF, 1)))
    if ($world) { $out = Join-Bytes $out (Macro 0x12 (Packed 88)) (Text $world) }
    , $out
}

# The player's own name: no world, no link, only the party number glyph.
function Own([string]$name, [int]$number) { , (Join-Bytes (Glyph $number) (Text $name)) }

function Record($stamp, [int]$kind, [byte[]]$sender, [byte[]]$message) {
    $head = New-Object System.IO.MemoryStream
    $w = New-Object System.IO.BinaryWriter($head)
    $w.Write([uint32]$stamp.ToUnixTimeSeconds())
    $w.Write([uint32]$kind)
    $w.Flush()
    , (Join-Bytes $head.ToArray() ([byte[]]@(0x1F)) $sender ([byte[]]@(0x1F)) $message)
}

function Write-Log([string]$path, [int]$first, $records) {
    $m = New-Object System.IO.MemoryStream
    $w = New-Object System.IO.BinaryWriter($m)
    $w.Write([uint32]$first)
    $w.Write([uint32]($first + $records.Count))
    $total = 0
    foreach ($r in $records) { $total += $r.Length; $w.Write([uint32]$total) }
    foreach ($r in $records) { $w.Write($r) }
    $w.Flush()
    New-Item -ItemType Directory -Force (Split-Path -Parent $path) | Out-Null
    [System.IO.File]::WriteAllBytes($path, $m.ToArray())
}

function At([int]$day, [int]$hour, [int]$minute) {
    New-Object System.DateTimeOffset(2026, 6, $day, $hour, $minute, 0, [TimeSpan]::FromHours(9))
}

# One evening: a party run with greetings, callouts, loot and some battle noise.
function Session([int]$day, [int]$startHour, $mates, [int]$lines) {
    $rows = New-Object System.Collections.Generic.List[object]
    $script:clock = At $day $startHour 2
    function Tick([int]$seconds = 7) { $script:clock = $script:clock.AddSeconds($seconds); $script:clock }
    $none = [byte[]]@()
    $self1 = Own $Self 1

    $rows.Add((Record (Tick) $SYSTEM $none (Text ($World + ' へようこそ！'))))
    $rows.Add((Record (Tick) $SYSTEM $none (Text ($mates[0][0] + ' のパーティに参加した。'))))
    for ($i = 0; $i -lt $mates.Count; $i++) {
        $rows.Add((Record (Tick) $PARTY (Player $mates[$i][0] $mates[$i][1] ($i + 2)) (Phrase 2 227)))
    }
    $rows.Add((Record (Tick) $PARTY $self1 (Phrase 2 227)))
    $rows.Add((Record (Tick) $PARTY (Player $mates[0][0] $mates[0][1] 2) (Text '準備できたら始めましょう。')))
    $rows.Add((Record (Tick) $PARTY $self1 (Phrase 2 823)))
    $chat = if ($day % 2) { Text '今日の黒衣森は天気がいいですね。' } else { Text '宝の地図、一緒に行ける人いませんか？' }
    $rows.Add((Record (Tick 20) $SAY (Player $mates[1][0] $mates[1][1]) $chat))
    $rows.Add((Record (Tick) $SHOUT (Player $mates[2][0] $mates[2][1]) (Join-Bytes (Color 255, 154, 26) (Text 'カララント安売り中！ ') (ColorReset) (Text '露店をのぞいてみてね。'))))
    $rows.Add((Record (Tick) $FC (Player $mates[1][0] $mates[1][1]) (Text ('おかえり、' + $Self.Split(' ')[0] + 'さん！'))))
    $rows.Add((Record (Tick) $TELL_OUT (Player $mates[2][0] $mates[2][1]) (Text 'カララントの情報ありがとう！')))
    $rows.Add((Record (Tick) $LS1 (Player $mates[0][0] $mates[0][1]) (Text '今夜の零式は21時からです。')))
    $rows.Add((Record (Tick) $EMOTE $none (Text ($mates[1][0] + 'はお辞儀した。'))))
    $battle = @(
        @($CAST, 'あなたは「ファイガ」を唱えた。'),
        @($DAMAGE, 'あなたの「ファイガ」 エンシェントゴーレムに4,212ダメージ。'),
        @($HEAL, ($mates[0][0] + 'のHPを3,180回復した。')),
        @($BUFF, 'あなたに「迅速魔」の効果。'),
        @($DAMAGE, ($mates[1][0] + 'の「デスペラード」 エンシェントゴーレムに5,880ダメージ。')),
        @($CAST, ($mates[2][0] + 'は「ケアルガ」を唱えた。'))
    )
    for ($index = 0; $index -lt $lines; $index++) {
        $b = $battle[$index % $battle.Count]
        $rows.Add((Record (Tick 3) $b[0] $none (Text $b[1])))
        if ($index % 9 -eq 4) {
            $who = $mates[$index % $mates.Count]
            $key = if ($index % 2) { 812 } else { 829 }
            $rows.Add((Record (Tick) $PARTY (Player $who[0] $who[1] (2 + $index % $mates.Count)) (Phrase 2 $key)))
        }
        if ($index % 11 -eq 6) { $rows.Add((Record (Tick) $PARTY $self1 (Phrase 2 216))) }
    }
    $rows.Add((Record (Tick 5) $LOOT $none (Join-Bytes (Color 126, 255, 160) (Text '「黒衣森のブナ材」を手に入れた。'))))
    $rows.Add((Record (Tick) $PROGRESS $none (Text '「黒魔道士」のレベルが91に上がった！')))
    $rows.Add((Record (Tick) $PARTY (Player $mates[0][0] $mates[0][1] 2) (Phrase 2 226)))
    $rows.Add((Record (Tick) $PARTY $self1 (Phrase 2 228)))
    , $rows
}

$crews = @(
    @(@('Kaito Windrider', 'Tiamat'), @('Mina Ashgrove', 'Ifrit'), @('Rin Duskbloom', 'Garuda')),
    @(@('Toma Brightwater', 'Ifrit'), @('Sora Emberfall', 'Carbuncle'), @('Nia Frostvale', 'Ifrit')),
    @(@('Kaito Windrider', 'Tiamat'), @('Hana Silverleaf', 'Ifrit'), @('Rin Duskbloom', 'Garuda')),
    @(@('Mina Ashgrove', 'Ifrit'), @('Toma Brightwater', 'Ifrit'), @('Sora Emberfall', 'Carbuncle')),
    @(@('Kaito Windrider', 'Tiamat'), @('Nia Frostvale', 'Ifrit'), @('Hana Silverleaf', 'Ifrit'))
)
$first = 0
for ($index = 0; $index -lt $crews.Count; $index++) {
    $rows = Session (23 + $index) 20 $crews[$index] (18 + $index * 2)
    Write-Log (Join-Path $OutDir "$Folder\log\$('{0:D8}' -f $index).log") $first $rows
    $first += $rows.Count
}
Write-Host "wrote $($crews.Count) files to $(Join-Path $OutDir "$Folder\log")"
