//! Game lookup tables (auto-translate phrases, UI colours, bitmap icon names).
//!
//! They hold game text, so they are not shipped: on start the app downloads the public
//! sheets once, converts them and keeps the result in `userdata\dict`.  Every lookup
//! degrades to a placeholder while the tables are missing.

use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

const SHEETS: &str = "https://raw.githubusercontent.com/xivapi/ffxiv-datamining/master/csv/ja/";
const ICON_ENUM: &str = "https://raw.githubusercontent.com/goatcorp/Dalamud/master/Dalamud/Game/Text/SeStringHandling/BitmapFontIcon.cs";
const NAME_COLUMNS: [&str; 5] = ["Name", "Singular", "Masculine", "Command", "Text"];
const FILES: [&str; 3] = ["autotranslate.json", "bitmapicons.json", "uicolor.json"];
/// One sheet is about 15 MB; anything far beyond that is not what we asked for.
const MAX_DOWNLOAD: u64 = 64 * 1024 * 1024;

#[derive(Deserialize, Default)]
pub struct AutoTranslate {
    #[serde(default)]
    pub text: HashMap<u32, String>,
    #[serde(default)]
    pub lookup: HashMap<u32, HashMap<u32, String>>,
}

#[derive(Default)]
pub struct Dict {
    pub auto: AutoTranslate,
    pub colors: HashMap<u32, String>,
    pub icons: HashMap<u32, String>,
}

fn slot() -> &'static RwLock<Arc<Dict>> {
    static SLOT: OnceLock<RwLock<Arc<Dict>>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(Arc::new(Dict::default())))
}

/// The tables currently in use (empty until they are loaded or downloaded).
pub fn current() -> Arc<Dict> {
    slot().read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Where the tables stand: "ready", "downloading" or "error" (with the reason); "" before start-up decides.
pub fn status() -> (&'static str, String) {
    status_slot().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn set_status(status: &'static str, message: impl Into<String>) {
    *status_slot().lock().unwrap_or_else(|e| e.into_inner()) = (status, message.into());
}

fn status_slot() -> &'static Mutex<(&'static str, String)> {
    static STATUS: OnceLock<Mutex<(&'static str, String)>> = OnceLock::new();
    STATUS.get_or_init(|| Mutex::new(("", String::new())))
}

pub fn install(dict: Dict) {
    *slot().write().unwrap_or_else(|e| e.into_inner()) = Arc::new(dict);
}

fn parse<T: for<'a> Deserialize<'a> + Default>(raw: &str) -> T {
    serde_json::from_str(raw).unwrap_or_default()
}

/// Reads the saved tables from `dir`; `None` unless all three files are there.
pub fn load(dir: &Path) -> Option<Dict> {
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
    Some(Dict {
        auto: parse(&read(FILES[0])?),
        icons: parse(&read(FILES[1])?),
        colors: parse(&read(FILES[2])?),
    })
}

/// Downloads the sheets, converts them and saves the result in `dir`.
pub fn download(dir: &Path) -> Result<Dict, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .user_agent("Aethelog")
        .build();
    let fetch = |url: &str| -> Result<String, String> {
        let response = agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_DOWNLOAD)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{url}: {e}"))?;
        String::from_utf8(bytes).map_err(|_| format!("{url}: UTF-8 ではありません"))
    };
    let (auto, colors, icons) = build(|sheet| fetch(&format!("{SHEETS}{sheet}.csv")), || fetch(ICON_ENUM))?;

    // Write to temporary names first so a failure never leaves a half-written set behind.
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let outputs = [
        (FILES[0], serde_json::to_string(&auto)),
        (FILES[1], serde_json::to_string(&icons)),
        (FILES[2], serde_json::to_string(&colors)),
    ];
    let mut staged = Vec::new();
    for (name, json) in outputs {
        let temp = dir.join(format!("{name}.tmp"));
        std::fs::write(&temp, json.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        staged.push((temp, dir.join(name)));
    }
    for (temp, target) in staged {
        std::fs::rename(&temp, &target).map_err(|e| e.to_string())?;
    }
    load(dir).ok_or_else(|| "辞書を保存できませんでした".to_owned())
}

#[derive(serde::Serialize)]
struct AutoOut {
    text: BTreeMap<u32, String>,
    lookup: BTreeMap<u32, BTreeMap<u32, String>>,
}

/// Pure conversion: `sheet` returns a CSV by name, `icon_enum` the Dalamud enum source.
fn build(
    sheet: impl Fn(&str) -> Result<String, String>,
    icon_enum: impl Fn() -> Result<String, String>,
) -> Result<(AutoOut, BTreeMap<u32, String>, BTreeMap<u32, String>), String> {
    let completion = Sheet::new(&sheet("Completion")?, "Completion")?;
    let mut text = BTreeMap::new();
    let mut wanted: BTreeMap<u32, (String, Vec<(u32, u32)>)> = BTreeMap::new();
    for row in &completion.rows {
        let group: u32 = completion.cell(row, "Group").parse().unwrap_or(0);
        let table = completion.cell(row, "LookupTable").trim();
        if table.is_empty() || table == "@" {
            let phrase = completion.cell(row, "Text");
            if let (false, Ok(id)) = (phrase.is_empty(), completion.cell(row, "#").parse::<u32>()) {
                text.insert(id, phrase.to_owned());
            }
        } else {
            wanted.insert(group, parse_lookup(table)?);
        }
    }

    let mut lookup = BTreeMap::new();
    let mut cache: HashMap<String, Sheet> = HashMap::new();
    for (group, (name, spans)) in wanted {
        if !cache.contains_key(&name) {
            cache.insert(name.clone(), Sheet::new(&sheet(&name)?, &name)?);
        }
        let data = &cache[&name];
        let mut names = BTreeMap::new();
        if let Some(column) = NAME_COLUMNS.iter().find(|c| data.index.contains_key(**c)) {
            for row in &data.rows {
                let Ok(id) = data.cell(row, "#").parse::<u32>() else { continue };
                let value = data.cell(row, column);
                let inside = spans.is_empty() || spans.iter().any(|&(lo, hi)| id >= lo && id <= hi);
                if inside && !value.is_empty() {
                    names.insert(id, value.to_owned());
                }
            }
        }
        lookup.insert(group, names);
    }

    // UI colours: the "Dark" column holds 0xRRGGBBAA.
    let ui = Sheet::new(&sheet("UIColor")?, "UIColor")?;
    let mut colors = BTreeMap::new();
    for row in &ui.rows {
        if let (Ok(id), Ok(value)) = (ui.cell(row, "#").parse::<u32>(), ui.cell(row, "Dark").parse::<u64>()) {
            colors.insert(id, format!("#{:06X}", value >> 8));
        }
    }

    // Icon names from the enum source: "Name = 12," -> id 12.
    let mut icons = BTreeMap::new();
    for line in icon_enum()?.lines() {
        let Some((name, rest)) = line.trim().split_once('=') else { continue };
        let name = name.trim();
        let Some(digits) = rest.trim().strip_suffix(',') else { continue };
        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            if let Ok(id) = digits.trim().parse::<u32>() {
                icons.insert(id, name.to_owned());
            }
        }
    }
    if text.is_empty() || colors.is_empty() || icons.is_empty() {
        return Err("取得した辞書が空でした（取得元の形式が変わった可能性があります）".into());
    }
    Ok((AutoOut { text, lookup }, colors, icons))
}

/// `Action[1-5,9-9]` -> sheet name and id ranges; other options are ignored.
fn parse_lookup(table: &str) -> Result<(String, Vec<(u32, u32)>), String> {
    let bad = || format!("Unexpected lookup table: {table}");
    let (name, options) = match table.split_once('[') {
        Some((name, rest)) => (name, rest.strip_suffix(']').ok_or_else(bad)?),
        None => (table, ""),
    };
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(bad());
    }
    let spans = options
        .split(',')
        .filter_map(|part| {
            let (lo, hi) = part.trim().split_once('-')?;
            Some((lo.trim().parse().ok()?, hi.trim().parse().ok()?))
        })
        .collect();
    Ok((name.to_owned(), spans))
}

struct Sheet {
    index: HashMap<String, usize>,
    rows: Vec<Vec<String>>,
}

impl Sheet {
    fn new(csv: &str, name: &str) -> Result<Sheet, String> {
        let mut rows = parse_csv(csv);
        if rows.is_empty() {
            return Err(format!("{name}: 空のシートです"));
        }
        let header = rows.remove(0);
        let index = header.into_iter().enumerate().map(|(i, h)| (h, i)).collect();
        Ok(Sheet { index, rows })
    }

    fn cell<'a>(&self, row: &'a [String], column: &str) -> &'a str {
        self.index.get(column).and_then(|&i| row.get(i)).map_or("", String::as_str)
    }
}

fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cell.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                cell.push(c);
            }
        } else if c == '"' {
            quoted = true;
        } else if c == ',' {
            row.push(std::mem::take(&mut cell));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut cell));
            rows.push(std::mem::take(&mut row));
        } else {
            cell.push(c);
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_handles_quotes_and_crlf() {
        let rows = parse_csv("a,b\r\n1,\"x,\"\"y\"\"\nz\"\r\n");
        assert_eq!(rows, vec![vec!["a", "b"], vec!["1", "x,\"y\"\nz"]]);
    }

    #[test]
    fn lookup_spec_with_and_without_ranges() {
        assert_eq!(parse_lookup("Action[1-5,9-9]").unwrap(), ("Action".to_owned(), vec![(1, 5), (9, 9)]));
        assert_eq!(parse_lookup("Weather").unwrap(), ("Weather".to_owned(), vec![]));
        assert!(parse_lookup("A b").is_err());
    }

    #[test]
    fn builds_tables_from_sheets() {
        let sheet = |name: &str| {
            Ok(match name {
                "Completion" => "#,Text,GroupTitle,LookupTable,Group,Key\n100,Title,,@,1,0\n101,Hello,,,1,1\n200,,,Weather[2-3],7,0\n",
                "Weather" => "#,Name\n1,Clear\n2,Fog\n3,Rain\n4,Snow\n",
                "UIColor" => "#,Dark\n0,0\n500,4286257919\n",
                other => return Err(other.to_owned()),
            }
            .to_owned())
        };
        let icons = || Ok("enum X {\n    None = 0,\n    CrossWorld = 88,\n}\n".to_owned());
        let (auto, colors, icons) = build(sheet, icons).unwrap();
        assert_eq!(auto.text.get(&101).map(String::as_str), Some("Hello"));
        assert_eq!(auto.lookup[&7].len(), 2);
        assert_eq!(auto.lookup[&7][&2], "Fog");
        assert_eq!(colors[&500], "#FF7B1A");
        assert_eq!(icons[&88], "CrossWorld");
    }

    /// Compares the conversion with the tables the PowerShell script made from the same
    /// cached sheets (`tools\.cache`, `data\`).  Needs both; run with --ignored.
    #[test]
    #[ignore]
    fn matches_the_script_output() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let cache = root.join("tools/.cache");
        let sheet = |name: &str| std::fs::read_to_string(cache.join(format!("{name}.csv"))).map_err(|e| e.to_string());
        let icons = || std::fs::read_to_string(cache.join("Bitmap.cs")).map_err(|e| e.to_string());
        let (auto, colors, icons) = build(sheet, icons).unwrap();
        let expected = load(&root.join("data")).unwrap();
        let ours: HashMap<u32, HashMap<u32, String>> =
            auto.lookup.into_iter().map(|(k, v)| (k, v.into_iter().collect())).collect();
        assert_eq!(ours, expected.auto.lookup);
        assert_eq!(auto.text.into_iter().collect::<HashMap<_, _>>(), expected.auto.text);
        assert_eq!(colors.into_iter().collect::<HashMap<_, _>>(), expected.colors);
        assert_eq!(icons.into_iter().collect::<HashMap<_, _>>(), expected.icons);
    }

    /// Downloads the real sheets into a temporary folder (needs the network); run with --ignored.
    #[test]
    #[ignore]
    fn downloads_the_real_tables() {
        let dir = std::env::temp_dir().join("aethelog-dict-test");
        let _ = std::fs::remove_dir_all(&dir);
        let fresh = download(&dir).unwrap();
        assert!(fresh.auto.text.len() > 100 && !fresh.colors.is_empty() && !fresh.icons.is_empty());
        assert!(load(&dir).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
