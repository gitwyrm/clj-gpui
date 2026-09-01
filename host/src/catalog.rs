//! gpui-component named themes for Clojure `:theme`.
//!
//! Appearance values (`system`, `light`, `dark`) are not palettes. Everything
//! else is looked up by ThemeConfig `name` or ThemeSet family `name`.
//!
//! Precedence (first match wins):
//! 1. ThemeSets Clojure sent on the render response (`:themes`)
//! 2. `CLJ_GPUI_THEMES` then `./themes` (JSON files, cached by mtime)
//! 3. bundled gpui-component JSON
//! 4. ThemeRegistry (Default Light / Default Dark)

use gpui::App;
use gpui_component::theme::{Theme, ThemeConfig, ThemeMode, ThemeRegistry, ThemeSet};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

const BUNDLED_JSON: &[&str] = &[
    include_str!("../themes/adventure.json"),
    include_str!("../themes/alduin.json"),
    include_str!("../themes/ayu.json"),
    include_str!("../themes/catppuccin.json"),
    include_str!("../themes/everforest.json"),
    include_str!("../themes/fahrenheit.json"),
    include_str!("../themes/flexoki.json"),
    include_str!("../themes/gruvbox.json"),
    include_str!("../themes/harper.json"),
    include_str!("../themes/hybrid.json"),
    include_str!("../themes/jellybeans.json"),
    include_str!("../themes/kibble.json"),
    include_str!("../themes/macos-classic.json"),
    include_str!("../themes/matrix.json"),
    include_str!("../themes/mellifluous.json"),
    include_str!("../themes/molokai.json"),
    include_str!("../themes/solarized.json"),
    include_str!("../themes/spaceduck.json"),
    include_str!("../themes/tokyonight.json"),
    include_str!("../themes/twilight.json"),
];

pub fn normalize(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .replace(['_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn names_equal(a: impl AsRef<str>, b: impl AsRef<str>) -> bool {
    normalize(a.as_ref()) == normalize(b.as_ref())
}

pub fn is_appearance(pref: &str) -> bool {
    matches!(normalize(pref).as_str(), "" | "system" | "light" | "dark")
}

pub fn pick_from_set(set: &ThemeSet, mode: ThemeMode) -> Option<&ThemeConfig> {
    let want_dark = mode.is_dark();
    set.themes
        .iter()
        .find(|theme| theme.mode.is_dark() == want_dark)
        .or_else(|| set.themes.first())
}

struct Catalog {
    by_norm: HashMap<String, ThemeConfig>,
    sets: HashMap<String, ThemeSet>,
}

impl Catalog {
    fn new() -> Self {
        Self {
            by_norm: HashMap::new(),
            sets: HashMap::new(),
        }
    }

    fn bundled() -> Self {
        let mut catalog = Self::new();
        for json in BUNDLED_JSON {
            catalog.load_json(json, "bundled");
        }
        catalog
    }

    fn load_json(&mut self, json: &str, origin: &str) {
        match serde_json::from_str::<ThemeSet>(json) {
            Ok(set) => self.insert_set(set),
            Err(err) => eprintln!("[host] failed to parse {origin} theme JSON: {err}"),
        }
    }

    fn insert_set(&mut self, set: ThemeSet) {
        for theme in &set.themes {
            let key = normalize(theme.name.as_ref());
            if !key.is_empty() {
                self.by_norm.entry(key).or_insert_with(|| theme.clone());
            }
        }
        let set_key = normalize(set.name.as_ref());
        if !set_key.is_empty() {
            self.sets.entry(set_key).or_insert(set);
        }
    }

    fn get_variant(&self, name: &str) -> Option<Rc<ThemeConfig>> {
        self.by_norm.get(&normalize(name)).cloned().map(Rc::new)
    }

    fn get_family(&self, name: &str, mode: ThemeMode) -> Option<Rc<ThemeConfig>> {
        let set = self.sets.get(&normalize(name))?;
        pick_from_set(set, mode).cloned().map(Rc::new)
    }
}

fn bundled_catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(Catalog::bundled)
}

struct ClojureCatalog {
    inner: Catalog,
}

fn clojure_catalog() -> &'static Mutex<ClojureCatalog> {
    static CATALOG: OnceLock<Mutex<ClojureCatalog>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        Mutex::new(ClojureCatalog {
            inner: Catalog::new(),
        })
    })
}

/// Replace Clojure-registered ThemeSets (the `:themes` array on a render).
pub fn install_clojure_sets(sets: Vec<ThemeSet>) {
    let mut catalog = Catalog::new();
    for set in sets {
        catalog.insert_set(set);
    }
    if let Ok(mut slot) = clojure_catalog().lock() {
        slot.inner = catalog;
    }
}

pub fn theme_sets_from_value(value: Option<&Value>) -> Vec<ThemeSet> {
    let Some(value) = value else {
        return Vec::new();
    };
    let Some(items) = value.as_array() else {
        eprintln!("[host] ignored :themes (not an array)");
        return Vec::new();
    };
    let mut sets = Vec::new();
    for (index, item) in items.iter().enumerate() {
        match serde_json::from_value::<ThemeSet>(item.clone()) {
            Ok(set) if set.themes.is_empty() => {
                eprintln!("[host] ignored empty Clojure theme set at {index}");
            }
            Ok(set) => sets.push(set),
            Err(err) => {
                eprintln!("[host] ignored invalid Clojure theme set [{index}]: {err}");
            }
        }
    }
    sets
}

fn extra_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(path) = std::env::var("CLJ_GPUI_THEMES") {
        let path = PathBuf::from(path);
        if path.is_dir() {
            dirs.push(path);
        }
    }
    let cwd = PathBuf::from("themes");
    if cwd.is_dir() {
        dirs.push(cwd);
    }
    dirs
}

fn file_mtime(path: &Path) -> u64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

#[derive(Clone)]
struct DirSnapshot {
    fingerprint: Vec<(PathBuf, u64)>,
    sets: Vec<ThemeSet>,
    loads: usize,
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    files.sort();
    files
}

fn fingerprint(files: &[PathBuf]) -> Vec<(PathBuf, u64)> {
    files
        .iter()
        .map(|path| (path.clone(), file_mtime(path)))
        .collect()
}

fn load_sets_from_files(files: &[PathBuf]) -> Vec<ThemeSet> {
    let mut sets = Vec::new();
    for path in files {
        match std::fs::read_to_string(path) {
            Ok(json) => match serde_json::from_str::<ThemeSet>(&json) {
                Ok(set) if set.themes.is_empty() => {
                    eprintln!("[host] ignored empty theme file {}", path.display());
                }
                Ok(set) => sets.push(set),
                Err(err) => {
                    eprintln!(
                        "[host] ignored invalid theme file {}: {err}",
                        path.display()
                    );
                }
            },
            Err(err) => {
                eprintln!("[host] could not read theme file {}: {err}", path.display());
            }
        }
    }
    sets
}

fn dir_cache() -> &'static Mutex<HashMap<PathBuf, DirSnapshot>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, DirSnapshot>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached_dir_sets(dir: &Path) -> Vec<ThemeSet> {
    let files = json_files(dir);
    let fp = fingerprint(&files);
    if let Ok(cache) = dir_cache().lock() {
        if let Some(snap) = cache.get(dir) {
            if snap.fingerprint == fp {
                return snap.sets.clone();
            }
        }
    }
    let sets = load_sets_from_files(&files);
    if let Ok(mut cache) = dir_cache().lock() {
        let loads = cache.get(dir).map(|snap| snap.loads).unwrap_or(0) + 1;
        cache.insert(
            dir.to_path_buf(),
            DirSnapshot {
                fingerprint: fp,
                sets: sets.clone(),
                loads,
            },
        );
    }
    sets
}

#[cfg(test)]
pub fn dir_load_count(dir: &Path) -> usize {
    dir_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(dir).map(|snap| snap.loads))
        .unwrap_or(0)
}

fn from_registry(name: &str, cx: &App) -> Option<Rc<ThemeConfig>> {
    ThemeRegistry::global(cx)
        .themes()
        .iter()
        .find(|(key, _)| names_equal(key, name))
        .map(|(_, theme)| Rc::clone(theme))
}

fn lookup_in_sets(sets: &[ThemeSet], name: &str, mode: ThemeMode) -> Option<Rc<ThemeConfig>> {
    for set in sets {
        for theme in &set.themes {
            if names_equal(theme.name.as_ref(), name) {
                return Some(Rc::new(theme.clone()));
            }
        }
    }
    for set in sets {
        if names_equal(set.name.as_ref(), name) {
            return pick_from_set(set, mode).cloned().map(Rc::new);
        }
    }
    None
}

/// Look up a gpui-component theme by variant or ThemeSet family name.
pub fn lookup(name: &str, mode: ThemeMode, cx: &App) -> Option<Rc<ThemeConfig>> {
    if let Ok(slot) = clojure_catalog().lock() {
        if let Some(theme) = slot.inner.get_variant(name) {
            return Some(theme);
        }
        if let Some(theme) = slot.inner.get_family(name, mode) {
            return Some(theme);
        }
    }
    for dir in extra_dirs() {
        let sets = cached_dir_sets(&dir);
        if let Some(theme) = lookup_in_sets(&sets, name, mode) {
            return Some(theme);
        }
    }
    let bundled = bundled_catalog();
    if let Some(theme) = bundled.get_variant(name) {
        return Some(theme);
    }
    if let Some(theme) = bundled.get_family(name, mode) {
        return Some(theme);
    }
    from_registry(name, cx)
}

pub fn reset_default_palettes(cx: &mut App) {
    let (light, dark) = {
        let registry = ThemeRegistry::global(cx);
        (
            registry.default_light_theme().clone(),
            registry.default_dark_theme().clone(),
        )
    };
    let theme = Theme::global_mut(cx);
    theme.light_theme = light;
    theme.dark_theme = dark;
}

#[cfg(test)]
pub fn bundled_names() -> Vec<String> {
    let mut names: Vec<String> = BUNDLED_JSON
        .iter()
        .filter_map(|json| serde_json::from_str::<ThemeSet>(json).ok())
        .flat_map(|set| set.themes.into_iter().map(|theme| theme.name.to_string()))
        .collect();
    names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::time::Duration;

    static UNIQUE_DIR: AtomicU64 = AtomicU64::new(0);

    fn unique_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "clj-gpui-themes-{}-{}",
            std::process::id(),
            UNIQUE_DIR.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn bundled_json_parses_and_includes_tokyo_night() {
        for json in BUNDLED_JSON {
            serde_json::from_str::<ThemeSet>(json).expect("bundled theme JSON should parse");
        }
        let names = bundled_names();
        assert!(
            names.iter().any(|name| name == "Tokyo Night"),
            "bundled themes: {names:?}"
        );
        assert!(names.iter().any(|name| name == "Ayu Light"));
        assert_eq!(names.len(), 35);
    }

    #[test]
    fn name_normalization_collapses_hyphens_underscores_and_space() {
        assert_eq!(normalize("My Theme"), "my theme");
        assert_eq!(normalize("my-theme"), "my theme");
        assert_eq!(normalize("my_theme"), "my theme");
        assert_eq!(normalize("  My   Theme  "), "my theme");
        assert_eq!(normalize("Tokyo_Night"), "tokyo night");
        assert_eq!(normalize("Catppuccin Violet"), "catppuccin violet");
        assert_eq!(normalize("catppuccin-violet"), "catppuccin violet");
        assert_eq!(is_appearance("Light"), true);
        assert_eq!(is_appearance("  SYSTEM  "), true);
        assert_eq!(is_appearance("my-theme"), false);
    }

    #[test]
    fn kebab_and_underscores_match_display_names() {
        let catalog = Catalog::bundled();
        assert_eq!(
            catalog.get_variant("tokyo-night").unwrap().name.as_ref(),
            "Tokyo Night"
        );
        assert_eq!(
            catalog.get_variant("Tokyo_Night").unwrap().name.as_ref(),
            "Tokyo Night"
        );
        assert_eq!(
            catalog
                .get_variant("catppuccin mocha")
                .unwrap()
                .name
                .as_ref(),
            "Catppuccin Mocha"
        );
        assert_eq!(
            catalog
                .get_variant("macos-classic-dark")
                .unwrap()
                .name
                .as_ref(),
            "macOS Classic Dark"
        );
    }

    #[test]
    fn appearance_keywords_are_not_palettes() {
        assert!(is_appearance("system"));
        assert!(is_appearance("Light"));
        assert!(is_appearance("DARK"));
        assert!(!is_appearance("Tokyo Night"));
        assert!(!is_appearance("Default Dark"));
    }

    #[test]
    fn family_name_picks_light_or_dark_member() {
        let mut catalog = Catalog::new();
        catalog.insert_set(
            serde_json::from_value(serde_json::json!({
                "name": "Catppuccin Violet",
                "themes": [
                    {"name": "Catppuccin Violet Light", "mode": "light", "colors": {"background": "#eff1f5"}},
                    {"name": "Catppuccin Violet Dark", "mode": "dark", "colors": {"background": "#1e1e2e"}}
                ]
            }))
            .unwrap(),
        );
        let light = catalog
            .get_family("catppuccin-violet", ThemeMode::Light)
            .unwrap();
        let dark = catalog
            .get_family("Catppuccin Violet", ThemeMode::Dark)
            .unwrap();
        assert_eq!(light.name.as_ref(), "Catppuccin Violet Light");
        assert_eq!(dark.name.as_ref(), "Catppuccin Violet Dark");
    }

    #[test]
    fn invalid_theme_set_json_is_skipped() {
        let sets = theme_sets_from_value(Some(&serde_json::json!([
            {"name": "Bad"},
            {
                "name": "Ok",
                "themes": [{"name": "Ok Dark", "mode": "dark", "colors": {"background": "#111111"}}]
            }
        ])));
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].name.as_ref(), "Ok");
    }

    #[test]
    fn clojure_sets_first_insert_wins_on_duplicate_variant() {
        let mut catalog = Catalog::new();
        catalog.insert_set(
            serde_json::from_value(serde_json::json!({
                "name": "Override",
                "themes": [{
                    "name": "Tokyo Night",
                    "mode": "dark",
                    "colors": {"background": "#ff00ff"}
                }]
            }))
            .unwrap(),
        );
        catalog.insert_set(
            serde_json::from_value(serde_json::json!({
                "name": "Later",
                "themes": [{
                    "name": "Tokyo Night",
                    "mode": "dark",
                    "colors": {"background": "#00ff00"}
                }]
            }))
            .unwrap(),
        );
        let theme = catalog.get_variant("tokyo-night").unwrap();
        assert_eq!(
            theme.colors.background.as_ref().map(|s| s.as_ref()),
            Some("#ff00ff")
        );
    }

    #[test]
    fn json_dir_cache_does_not_reparse_unchanged_files() {
        let dir = unique_dir();
        let file = dir.join("custom.json");
        fs::write(
            &file,
            r##"{"name":"Cache","themes":[{"name":"Cache Dark","mode":"dark","colors":{"background":"#010101"}}]}"##,
        )
        .unwrap();
        let _ = cached_dir_sets(&dir);
        assert_eq!(dir_load_count(&dir), 1);
        let _ = cached_dir_sets(&dir);
        let _ = cached_dir_sets(&dir);
        assert_eq!(dir_load_count(&dir), 1, "unchanged files must not reparse");
        thread::sleep(Duration::from_millis(50));
        fs::write(
            &file,
            r##"{"name":"Cache","themes":[{"name":"Cache Dark","mode":"dark","colors":{"background":"#020202"}}]}"##,
        )
        .unwrap();
        let _ = cached_dir_sets(&dir);
        assert!(dir_load_count(&dir) > 1, "mtime change must reload");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_files_are_sorted_so_duplicates_are_stable() {
        let dir = unique_dir();
        fs::write(
            dir.join("b.json"),
            r##"{"name":"B","themes":[{"name":"Dup","mode":"dark","colors":{"background":"#0000bb"}}]}"##,
        )
        .unwrap();
        fs::write(
            dir.join("a.json"),
            r##"{"name":"A","themes":[{"name":"Dup","mode":"dark","colors":{"background":"#0000aa"}}]}"##,
        )
        .unwrap();
        let sets = cached_dir_sets(&dir);
        let theme = lookup_in_sets(&sets, "Dup", ThemeMode::Dark).unwrap();
        assert_eq!(
            theme.colors.background.as_ref().map(|s| s.as_ref()),
            Some("#0000aa")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_file_with_empty_or_invalid_content_is_skipped() {
        let dir = unique_dir();
        fs::write(dir.join("bad.json"), "{not json").unwrap();
        fs::write(dir.join("empty.json"), r#"{"name":"Empty","themes":[]}"#).unwrap();
        fs::write(
            dir.join("ok.json"),
            r##"{"name":"Ok","themes":[{"name":"Ok Dark","mode":"dark","colors":{"background":"#111111"}}]}"##,
        )
        .unwrap();
        let sets = cached_dir_sets(&dir);
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].name.as_ref(), "Ok");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn example_catppuccin_violet_json_parses_if_present() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/themes/catppuccin-violet/themes/catppuccin-violet.json");
        if !path.is_file() {
            return;
        }
        let json = fs::read_to_string(&path).unwrap();
        let set: ThemeSet = serde_json::from_str(&json).expect("Catppuccin Violet JSON");
        assert_eq!(set.name.as_ref(), "Catppuccin Violet");
        assert_eq!(set.themes.len(), 2);
        assert_eq!(set.themes[0].name.as_ref(), "Catppuccin Violet Light");
        assert_eq!(set.themes[0].mode, ThemeMode::Light);
        assert_eq!(set.themes[1].name.as_ref(), "Catppuccin Violet Dark");
        assert_eq!(set.themes[1].mode, ThemeMode::Dark);
        assert_eq!(
            set.themes[1].colors.primary.as_ref().map(|s| s.as_ref()),
            Some("#cba6f7")
        );
        assert_eq!(
            set.themes[0].colors.background.as_ref().map(|s| s.as_ref()),
            Some("#eff1f5")
        );
    }
}
