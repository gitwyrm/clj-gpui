//! gpui-component named themes for Clojure `:theme`.
//!
//! Appearance values (`system`, `light`, `dark`) are not palettes. Everything
//! else is looked up by the `name` field in a theme JSON set: bundled files
//! under `host/themes/`, then `CLJ_GPUI_THEMES` / a local `themes/` directory,
//! then ThemeRegistry (Default Light / Default Dark).

use gpui::App;
use gpui_component::theme::{Theme, ThemeConfig, ThemeRegistry, ThemeSet};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;

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

struct Catalog {
    by_norm: HashMap<String, ThemeConfig>,
}

impl Catalog {
    fn bundled() -> Self {
        let mut catalog = Self {
            by_norm: HashMap::new(),
        };
        for json in BUNDLED_JSON {
            catalog.load_json(json, "bundled");
        }
        catalog
    }

    fn load_json(&mut self, json: &str, origin: &str) {
        match serde_json::from_str::<ThemeSet>(json) {
            Ok(set) => {
                for theme in set.themes {
                    self.insert(theme);
                }
            }
            Err(err) => eprintln!("[host] failed to parse {origin} theme JSON: {err}"),
        }
    }

    fn insert(&mut self, theme: ThemeConfig) {
        let key = normalize(theme.name.as_ref());
        if key.is_empty() {
            return;
        }
        self.by_norm.insert(key, theme);
    }

    fn get(&self, name: &str) -> Option<Rc<ThemeConfig>> {
        self.by_norm.get(&normalize(name)).cloned().map(Rc::new)
    }
}

fn bundled_catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(Catalog::bundled)
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

fn load_dir(dir: &Path) -> Vec<ThemeConfig> {
    let mut themes = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return themes;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(json) => match serde_json::from_str::<ThemeSet>(&json) {
                Ok(set) => themes.extend(set.themes),
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
    themes
}

fn from_registry(name: &str, cx: &App) -> Option<Rc<ThemeConfig>> {
    ThemeRegistry::global(cx)
        .themes()
        .iter()
        .find(|(key, _)| names_equal(key, name))
        .map(|(_, theme)| Rc::clone(theme))
}

/// Look up a gpui-component theme by its `name` (case, spaces, `-`, `_` ignored).
pub fn lookup(name: &str, cx: &App) -> Option<Rc<ThemeConfig>> {
    for dir in extra_dirs() {
        for theme in load_dir(&dir) {
            if names_equal(theme.name.as_ref(), name) {
                return Some(Rc::new(theme));
            }
        }
    }
    if let Some(theme) = bundled_catalog().get(name) {
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
    fn kebab_and_underscores_match_display_names() {
        let catalog = Catalog::bundled();
        assert_eq!(
            catalog.get("tokyo-night").unwrap().name.as_ref(),
            "Tokyo Night"
        );
        assert_eq!(
            catalog.get("Tokyo_Night").unwrap().name.as_ref(),
            "Tokyo Night"
        );
        assert_eq!(
            catalog.get("catppuccin mocha").unwrap().name.as_ref(),
            "Catppuccin Mocha"
        );
        assert_eq!(
            catalog.get("macos-classic-dark").unwrap().name.as_ref(),
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
}
