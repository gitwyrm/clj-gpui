use serde::Deserialize;

pub const PROTOCOL_VERSION: u64 = 1;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Node {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub children: Vec<Node>,
    #[serde(default, rename = "on-click")]
    pub on_click: Option<String>,
    #[serde(default, rename = "on-change")]
    #[allow(dead_code)]
    pub on_change: Option<String>,
    #[serde(default)]
    pub checked: Option<bool>,
    #[serde(default)]
    pub gap: Option<f32>,
    #[serde(default)]
    pub padding: Option<f32>,
    #[serde(default, rename = "font-size")]
    pub font_size: Option<f32>,
    #[serde(default, rename = "font-weight")]
    pub font_weight: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
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
        self.children.iter().find_map(|child| child.find_button(text))
    }

    pub fn contains_text(&self, needle: &str) -> bool {
        self.text
            .as_deref()
            .is_some_and(|text| text.contains(needle))
            || self.children.iter().any(|child| child.contains_text(needle))
    }
}

#[derive(Debug)]
pub enum Cmd {
    Render,
    Callback(String),
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
    Tree(Node),
    Error(String),
}
