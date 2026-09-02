use gpui_component::theme::ThemeSet;
use serde::Deserialize;
use serde_json::{json, Value};

pub const PROTOCOL_VERSION: u64 = 4;

/// Host → Clojure `callback` request. `value` is omitted when `None`.
/// JSON `null` is `Some(Value::Null)` so Clojure can call `(f nil)`.
pub fn callback_request(callback_id: impl Into<String>, value: Option<Value>) -> Value {
    let mut request = json!({
        "op": "callback",
        "callback-id": callback_id.into()
    });
    if let Some(value) = value {
        request["value"] = value;
    }
    request
}

/// Collection item for radios, select, tabs, breadcrumbs, accordion, etc.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Item {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub children: Vec<Node>,
    #[serde(default)]
    pub content: Option<Box<Node>>,
    #[serde(default, rename = "on-click")]
    pub on_click: Option<String>,
}

impl Item {
    pub fn id_or_label(&self) -> String {
        self.id
            .clone()
            .or_else(|| self.label.clone())
            .or_else(|| self.text.clone())
            .unwrap_or_default()
    }

    pub fn label_or_id(&self) -> String {
        self.label
            .clone()
            .or_else(|| self.text.clone())
            .or_else(|| self.id.clone())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Node {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub children: Vec<Node>,
    #[serde(default, rename = "on-click")]
    pub on_click: Option<String>,
    #[serde(default, rename = "on-change")]
    pub on_change: Option<String>,
    #[serde(default, rename = "on-submit")]
    pub on_submit: Option<String>,
    #[serde(default, rename = "on-double-click")]
    pub on_double_click: Option<String>,
    #[serde(default, rename = "on-blur")]
    pub on_blur: Option<String>,
    #[serde(default, rename = "on-escape")]
    pub on_escape: Option<String>,
    #[serde(default, rename = "on-close")]
    pub on_close: Option<String>,
    #[serde(default, rename = "on-copied")]
    pub on_copied: Option<String>,
    #[serde(default)]
    pub checked: Option<bool>,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub compact: bool,
    #[serde(default)]
    pub strikethrough: bool,
    #[serde(default)]
    pub shadow: bool,
    #[serde(default)]
    pub bg: Option<String>,
    #[serde(default)]
    pub border: Option<String>,
    #[serde(default, rename = "border-bottom")]
    pub border_bottom: Option<String>,
    #[serde(default)]
    pub align: Option<String>,
    #[serde(default)]
    pub justify: Option<String>,
    #[serde(default)]
    pub gap: Option<f32>,
    #[serde(default)]
    pub padding: Option<f32>,
    #[serde(default, rename = "font-size")]
    pub font_size: Option<f32>,
    #[serde(default, rename = "font-weight")]
    pub font_weight: Option<String>,
    #[serde(default, rename = "font-family")]
    pub font_family: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    /// Any node: `"system"` (default), `"light"`, `"dark"`, a gpui-component
    /// palette name such as `"Tokyo Night"`, or a custom ThemeSet / variant name.
    #[serde(default)]
    pub theme: Option<String>,
    /// Native window title. Omitted keeps `clj-gpui`.
    /// Also used as the title for `alert` and `group-box`.
    #[serde(default)]
    pub title: Option<String>,
    /// `window` (or any root): `"dev"` (default, nREPL footer) or `"app"` (no host chrome).
    #[serde(default)]
    pub chrome: Option<String>,
    #[serde(default, rename = "window-width")]
    pub window_width: Option<f32>,
    #[serde(default, rename = "window-height")]
    pub window_height: Option<f32>,
    /// Text field: request keyboard focus when true.
    #[serde(default)]
    pub focus: bool,
    /// Checkbox: `"circle"` for a round toggle. Omitted is the square widget.
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub height: Option<f32>,
    #[serde(default)]
    pub width: Option<f32>,
    /// Pixel size (square). Named control sizes use `control-size`.
    #[serde(default)]
    pub size: Option<f32>,
    #[serde(default)]
    pub flex: Option<f32>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub tooltip: Option<String>,
    /// Selected / numeric / string value. JSON number, string, or bool.
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub min: Option<f32>,
    #[serde(default)]
    pub max: Option<f32>,
    #[serde(default)]
    pub step: Option<f32>,
    #[serde(default)]
    pub orientation: Option<String>,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub options: Vec<Item>,
    #[serde(default)]
    pub href: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default, rename = "control-size")]
    pub control_size: Option<String>,
    #[serde(default)]
    pub count: Option<u64>,
    #[serde(default)]
    pub dot: bool,
    #[serde(default)]
    pub dashed: bool,
    #[serde(default)]
    pub outline: bool,
    #[serde(default)]
    pub searchable: bool,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default)]
    pub message: Option<String>,
}

impl Node {
    pub fn find_button(&self, text: &str) -> Option<&Node> {
        if self.kind == "button" && self.text.as_deref() == Some(text) {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|child| child.find_button(text))
    }

    pub fn contains_text(&self, needle: &str) -> bool {
        self.text
            .as_deref()
            .is_some_and(|text| text.contains(needle))
            || self
                .placeholder
                .as_deref()
                .is_some_and(|text| text.contains(needle))
            || self
                .title
                .as_deref()
                .is_some_and(|text| text.contains(needle))
            || self
                .message
                .as_deref()
                .is_some_and(|text| text.contains(needle))
            || self
                .children
                .iter()
                .any(|child| child.contains_text(needle))
            || self.items.iter().any(|item| item_contains(item, needle))
            || self.options.iter().any(|item| item_contains(item, needle))
    }

    pub fn collection(&self) -> &[Item] {
        if !self.options.is_empty() {
            &self.options
        } else {
            &self.items
        }
    }

    pub fn string_value(&self) -> Option<String> {
        match &self.value {
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Number(n)) => Some(n.to_string()),
            Some(Value::Bool(b)) => Some(b.to_string()),
            Some(Value::Null) | None => None,
            Some(other) => Some(other.to_string()),
        }
    }

    /// Selected ids. JSON arrays (accordion `:multiple`) stay as separate
    /// ids; a single string is one id. `null` / omitted is empty.
    pub fn string_values(&self) -> Vec<String> {
        match &self.value {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    Value::Bool(b) => Some(b.to_string()),
                    Value::Null => None,
                    other => Some(other.to_string()),
                })
                .collect(),
            Some(Value::Null) | None => Vec::new(),
            Some(_) => self.string_value().into_iter().collect(),
        }
    }

    pub fn number_value(&self) -> Option<f32> {
        match &self.value {
            Some(Value::Number(n)) => n.as_f64().map(|n| n as f32),
            Some(Value::String(s)) => s.parse().ok(),
            _ => None,
        }
    }
}

fn item_contains(item: &Item, needle: &str) -> bool {
    item.label
        .as_deref()
        .is_some_and(|text| text.contains(needle))
        || item
            .text
            .as_deref()
            .is_some_and(|text| text.contains(needle))
        || item.id.as_deref().is_some_and(|text| text.contains(needle))
        || item
            .content
            .as_ref()
            .is_some_and(|node| node.contains_text(needle))
        || item
            .children
            .iter()
            .any(|child| child.contains_text(needle))
}

#[derive(Debug, Clone)]
pub enum Cmd {
    Render,
    Callback {
        id: String,
        value: Option<Value>,
        /// Set on text-field submit so the following tree can force-sync that field.
        seq: Option<u64>,
    },
    Reload,
    DirectoryPicked {
        request_id: String,
        path: Option<String>,
        error: Option<String>,
        cancelled: bool,
    },
    Shutdown,
}

#[derive(Debug)]
pub enum HostEvent {
    Ready {
        nrepl_port: u16,
        #[allow(dead_code)]
        app: String,
    },
    /// `callback_seq` is `Some` when this tree was fetched right after that submit.
    /// `themes` is Clojure-registered ThemeSets from the render response.
    Tree(Node, Option<u64>, Vec<ThemeSet>),
    Error(String),
    PickDirectory {
        request_id: String,
        title: Option<String>,
    },
    RevealPath {
        path: String,
    },
    OpenPath {
        path: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decodes_v3_button_node() {
        let node: Node = serde_json::from_value(json!({
            "type": "button",
            "text": "+",
            "on-click": "cb-1",
            "primary": true
        }))
        .unwrap();
        assert_eq!(node.kind, "button");
        assert_eq!(node.text.as_deref(), Some("+"));
        assert_eq!(node.on_click.as_deref(), Some("cb-1"));
        assert!(node.primary);
    }

    #[test]
    fn decodes_slider_and_select_nodes() {
        let slider: Node = serde_json::from_value(json!({
            "type": "slider",
            "value": 42.5,
            "min": 0,
            "max": 100,
            "step": 0.5,
            "on-change": "cb-2",
            "orientation": "horizontal"
        }))
        .unwrap();
        assert_eq!(slider.kind, "slider");
        assert_eq!(slider.number_value(), Some(42.5));
        assert_eq!(slider.min, Some(0.0));
        assert_eq!(slider.max, Some(100.0));
        assert_eq!(slider.step, Some(0.5));

        let select: Node = serde_json::from_value(json!({
            "type": "select",
            "value": "clj",
            "placeholder": "Language",
            "options": [
                {"id": "clj", "label": "Clojure"},
                {"id": "rs", "label": "Rust"}
            ],
            "on-change": "cb-3",
            "searchable": true
        }))
        .unwrap();
        assert_eq!(select.string_value().as_deref(), Some("clj"));
        assert_eq!(select.collection().len(), 2);
        assert_eq!(select.collection()[0].id_or_label(), "clj");
        assert_eq!(select.collection()[1].label_or_id(), "Rust");
        assert!(select.searchable);
    }

    #[test]
    fn decodes_tabs_switch_and_alert() {
        let tabs: Node = serde_json::from_value(json!({
            "type": "tabs",
            "value": "advanced",
            "variant": "underline",
            "items": [
                {"id": "general", "label": "General"},
                {"id": "advanced", "label": "Advanced"}
            ],
            "on-change": "cb-4"
        }))
        .unwrap();
        assert_eq!(tabs.string_value().as_deref(), Some("advanced"));
        assert_eq!(tabs.variant.as_deref(), Some("underline"));

        let switch: Node = serde_json::from_value(json!({
            "type": "switch",
            "checked": true,
            "text": "Notify",
            "on-change": "cb-5",
            "disabled": false,
            "tooltip": "Enable notifications"
        }))
        .unwrap();
        assert_eq!(switch.checked, Some(true));
        assert_eq!(switch.tooltip.as_deref(), Some("Enable notifications"));

        let alert: Node = serde_json::from_value(json!({
            "type": "alert",
            "text": "Saved",
            "title": "Done",
            "variant": "success",
            "on-close": "cb-6"
        }))
        .unwrap();
        assert_eq!(alert.on_close.as_deref(), Some("cb-6"));
        assert_eq!(alert.title.as_deref(), Some("Done"));
    }

    #[test]
    fn callback_request_omits_or_encodes_json_values() {
        let omitted = callback_request("cb-1", None);
        assert_eq!(omitted["op"], "callback");
        assert_eq!(omitted["callback-id"], "cb-1");
        assert!(omitted.get("value").is_none());

        assert_eq!(callback_request("cb-2", Some(json!(true)))["value"], true);
        assert_eq!(callback_request("cb-3", Some(json!(36.5)))["value"], 36.5);
        assert_eq!(callback_request("cb-4", Some(json!("clj")))["value"], "clj");
        assert_eq!(
            callback_request("cb-5", Some(Value::Null))["value"],
            Value::Null
        );
        assert_eq!(
            callback_request("cb-6", Some(json!(["a", "b"])))["value"],
            json!(["a", "b"])
        );
    }

    #[test]
    fn decodes_accordion_with_nested_content() {
        let node: Node = serde_json::from_value(json!({
            "type": "accordion",
            "value": "audio",
            "items": [{
                "id": "audio",
                "label": "Audio",
                "content": {"type": "label", "text": "Speakers"}
            }]
        }))
        .unwrap();
        assert_eq!(node.string_value().as_deref(), Some("audio"));
        assert_eq!(node.collection()[0].id_or_label(), "audio");
        assert!(node.contains_text("Speakers"));
        assert_eq!(PROTOCOL_VERSION, 4);
    }

    #[test]
    fn accordion_multiple_value_is_a_json_array() {
        let node: Node = serde_json::from_value(json!({
            "type": "accordion",
            "value": ["audio", "audio,advanced"],
            "multiple": true,
            "items": [
                {"id": "audio", "label": "Audio"},
                {"id": "audio,advanced", "label": "Mixed"}
            ]
        }))
        .unwrap();
        assert_eq!(
            node.string_values(),
            vec!["audio".to_string(), "audio,advanced".to_string()]
        );
        assert!(node.multiple);
    }

    #[test]
    fn json_null_value_is_not_the_string_null() {
        let node: Node = serde_json::from_value(json!({
            "type": "select",
            "value": null,
            "options": [{"id": "clj", "label": "Clojure"}]
        }))
        .unwrap();
        assert_eq!(node.string_value(), None);
        assert!(node.string_values().is_empty());
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let node: Node = serde_json::from_value(json!({
            "type": "label",
            "text": "Hi",
            "future-field": {"nested": true}
        }))
        .unwrap();
        assert_eq!(node.kind, "label");
        assert_eq!(node.text.as_deref(), Some("Hi"));
    }
}
