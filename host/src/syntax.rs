use gpui_kit::component::highlighter::{LanguageConfig, LanguageRegistry};

pub fn init() {
    let highlights = format!(
        "{}\n{}",
        tree_sitter_clojure_orchard::HIGHLIGHTS_QUERY,
        include_str!("clojure-highlights.scm")
    );
    LanguageRegistry::singleton().register(
        "clojure",
        &LanguageConfig::new(
            "clojure",
            tree_sitter_clojure_orchard::LANGUAGE.into(),
            vec![],
            &highlights,
            "",
            "",
        ),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn clojure_highlighter_loads_without_plain_text_fallback() {
        super::init();
        let mut highlighter = gpui_kit::component::highlighter::SyntaxHighlighter::new("clojure");
        assert_eq!(highlighter.language().as_ref(), "clojure");
        let source = "(defn hi [] :ok)";
        highlighter.update(None, &source.into(), None);
        assert!(!highlighter.tree().unwrap().root_node().has_error());
        let theme = gpui_kit::component::highlighter::HighlightTheme::default_dark();
        let styles = highlighter.styles(&(0..source.len()), theme.as_ref());
        for token in ["defn", ":ok"] {
            let start = source.find(token).unwrap();
            assert!(
                styles.iter().any(|(range, style)| {
                    range.start == start
                        && range.end == start + token.len()
                        && style.color.is_some()
                }),
                "missing color for {token}"
            );
        }
    }
}
