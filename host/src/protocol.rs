use serde::Deserialize;

pub const PROTOCOL_VERSION: u64 = 1;

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
    /// Root layout: `"system"` (default), `"light"`, or `"dark"`.
    #[serde(default)]
    pub theme: Option<String>,
    /// Checkbox: `"circle"` for a round toggle. Omitted is the square widget.
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub height: Option<f32>,
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub size: Option<f32>,
    #[serde(default)]
    pub flex: Option<f32>,
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
                .children
                .iter()
                .any(|child| child.contains_text(needle))
    }
}

#[derive(Debug, Clone)]
pub enum Cmd {
    Render,
    Callback {
        id: String,
        value: Option<String>,
        /// Set on text-field submit so the following tree can force-sync that field.
        seq: Option<u64>,
    },
    Reload,
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
    Tree(Node, Option<u64>),
    Error(String),
}
