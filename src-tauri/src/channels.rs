//! Chat-log kinds (the low 7 bits of a record's code) as the game's log filters name them.
//!
//! Kinds come from the game's LogFilter sheet and Dalamud's XivChatType.  A kind
//! that is not listed is shown by number only; nothing is guessed for it.

use serde::Serialize;

pub const GROUPS: [(&str, &str); 4] = [("chat", "会話"), ("system", "システム"), ("event", "イベント"), ("battle", "戦闘")];

// Line layouts, {n} = sender, {m} = message (the in-game chat log style).
const NAMED: &str = "{n}: {m}";
const PLAIN: &str = "{m}";

#[derive(Serialize, Clone)]
pub struct Channel {
    pub id: u8,
    pub name: String,
    pub group: &'static str,
    pub color: &'static str,
    pub layout: String,
}

// kind, name, group, colour, layout
const FIXED: &[(u8, &str, &str, &str, &str)] = &[
    (3, "お知らせ", "system", "#ffd479", PLAIN),
    (10, "Say", "chat", "#f7f7f7", NAMED),
    (11, "Shout", "chat", "#ffa666", NAMED),
    (12, "Tell（送信）", "chat", "#ffb8e8", ">> {n}: {m}"),
    (13, "Tell（受信）", "chat", "#ffb8e8", "{n} >> {m}"),
    (14, "Party", "chat", "#66e5ff", "({n}) {m}"),
    (15, "Alliance", "chat", "#ff9c3a", "(({n})) {m}"),
    (24, "Free Company", "chat", "#abdbe5", "[FC]<{n}> {m}"),
    (27, "ビギナーチャンネル", "chat", "#d4ff7d", "[ビギナー]{n} {m}"),
    (28, "オリジナルエモート", "chat", "#bafff0", PLAIN),
    (29, "エモート", "chat", "#bafff0", PLAIN),
    (30, "Yell", "chat", "#ffff66", NAMED),
    (32, "クロスワールドパーティ", "chat", "#66e5ff", "({n}) {m}"),
    (36, "PvPチーム", "chat", "#abdbe5", "[PvP]<{n}> {m}"),
    (37, "CWLS 1", "chat", "#d4ff7d", "[CWLS1]<{n}> {m}"),
    (54, "武具投影", "system", "#ccccff", PLAIN),
    (55, "アラーム", "system", "#ccccff", PLAIN),
    (56, "エコー", "chat", "#cccccc", PLAIN),
    (57, "システム", "system", "#ccccff", PLAIN),
    (58, "戦闘システム", "system", "#ccccff", PLAIN),
    (59, "採集システム", "system", "#ccccff", PLAIN),
    (60, "エラー", "system", "#ff6b6b", PLAIN),
    (61, "NPC会話", "event", "#abd647", NAMED),
    (62, "戦利品", "event", "#9be3a8", PLAIN),
    (64, "成長・修得", "event", "#ffe27a", PLAIN),
    (65, "ロット", "event", "#9be3a8", PLAIN),
    (66, "製作", "event", "#9be3a8", PLAIN),
    (67, "採集", "event", "#9be3a8", PLAIN),
    (68, "NPCアナウンス", "event", "#abd647", NAMED),
    (69, "FCアナウンス", "system", "#abdbe5", PLAIN),
    (70, "FCログイン通知", "system", "#abdbe5", PLAIN),
    (71, "マーケット売上", "system", "#ffe27a", PLAIN),
    (72, "パーティ募集通知", "system", "#ccccff", PLAIN),
    (73, "マーカー設定", "system", "#ccccff", PLAIN),
    (74, "ダイス", "system", "#ccccff", PLAIN),
    (75, "ビギナーアナウンス", "system", "#d4ff7d", PLAIN),
    (76, "オーケストリオン", "system", "#ccccff", PLAIN),
    (77, "PvPチームアナウンス", "system", "#abdbe5", PLAIN),
    (78, "PvPチームログイン通知", "system", "#abdbe5", PLAIN),
    (79, "交流帳", "system", "#abdbe5", PLAIN),
    (41, "ダメージ", "battle", "#ffffff", PLAIN),
    (42, "ミス・回避", "battle", "#c8c8c8", PLAIN),
    (43, "アクション詠唱", "battle", "#d6d6ff", PLAIN),
    (44, "アイテム使用", "battle", "#d6d6ff", PLAIN),
    (45, "HP回復", "battle", "#a8ff9c", PLAIN),
    (46, "バフ付与", "battle", "#9ce0ff", PLAIN),
    (47, "デバフ付与", "battle", "#ff9c9c", PLAIN),
    (48, "バフ解除", "battle", "#9ce0ff", PLAIN),
    (49, "デバフ解除", "battle", "#ff9c9c", PLAIN),
];

/// Every known kind, for the viewer's filter panel.
pub fn catalog() -> &'static [Channel] {
    static CATALOG: std::sync::OnceLock<Vec<Channel>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(build)
}

fn build() -> Vec<Channel> {
    let mut all: Vec<Channel> = FIXED
        .iter()
        .map(|&(id, name, group, color, layout)| Channel { id, name: name.into(), group, color, layout: layout.into() })
        .collect();
    for n in 0..8u8 {
        all.push(Channel {
            id: 16 + n,
            name: format!("Linkshell {}", n + 1),
            group: "chat",
            color: "#d4ff7d",
            layout: format!("[{}]<{{n}}> {{m}}", n + 1),
        });
    }
    for n in 2..=8u8 {
        all.push(Channel {
            id: 99 + n,
            name: format!("CWLS {n}"),
            group: "chat",
            color: "#d4ff7d",
            layout: format!("[CWLS{n}]<{{n}}> {{m}}"),
        });
    }
    all.sort_by_key(|c| c.id);
    all
}

pub fn name_of(kind: u8) -> String {
    catalog().iter().find(|c| c.id == kind).map_or_else(|| format!("種別 {kind:02X}"), |c| c.name.clone())
}

pub fn fallback(kind: u8) -> Channel {
    Channel { id: kind, name: name_of(kind), group: "other", color: "#d8d2c0", layout: PLAIN.into() }
}
