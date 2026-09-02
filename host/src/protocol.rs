use gpui_component::theme::ThemeSet;
use serde::Deserialize;
use serde_json::{json, Value};

pub const PROTOCOL_VERSION: u64 = 5;

/// Host → Clojure `callback` request. `value` is omitted when `None`.
/// JSON `null` is `Some(Value::Null)` so Clojure can call `(f nil)`.
///
/// `defer_render` is set on every item of a multi-callback native action,
/// including the last. Clojure keeps the callback transaction open so an
/// r/atom watch cannot enqueue `request-render` (and reset ids) before
/// the remaining callbacks run — or in the gap before the following
/// `"render"` RPC, which is what clears the hold. Singles omit the flag.
pub fn callback_request(callback_id: impl Into<String>, value: Option<Value>) -> Value {
    callback_rpc(callback_id, value, false)
}

pub fn callback_rpc(
    callback_id: impl Into<String>,
    value: Option<Value>,
    defer_render: bool,
) -> Value {
    let mut request = json!({
        "op": "callback",
        "callback-id": callback_id.into()
    });
    if let Some(value) = value {
        request["value"] = value;
    }
    if defer_render {
        request["defer-render"] = json!(true);
    }
    request
}

/// One Clojure callback captured for a native user action.
#[derive(Debug, Clone, PartialEq)]
pub struct CallbackCall {
    pub id: String,
    pub value: Option<Value>,
}

impl CallbackCall {
    pub fn fire(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            value: None,
        }
    }

    pub fn with_value(id: impl Into<String>, value: Value) -> Self {
        Self {
            id: id.into(),
            value: Some(value),
        }
    }
}

/// `true` on every callback RPC in a multi-callback batch, including the
/// last. A following `"render"` RPC clears Clojure's hold. A single
/// callback keeps the previous one-RPC contract (no `defer-render`).
pub fn defer_render_flags(count: usize) -> Vec<bool> {
    vec![count > 1; count]
}

pub fn send_callbacks(tx: &std::sync::mpsc::Sender<Cmd>, calls: Vec<CallbackCall>) {
    send_callbacks_seq(tx, calls, None);
}

pub fn send_callbacks_seq(
    tx: &std::sync::mpsc::Sender<Cmd>,
    calls: Vec<CallbackCall>,
    seq: Option<u64>,
) {
    let calls: Vec<CallbackCall> = calls
        .into_iter()
        .filter(|call| !call.id.is_empty())
        .collect();
    match calls.len() {
        0 => {}
        1 => {
            let call = calls.into_iter().next().unwrap();
            let _ = tx.send(Cmd::Callback {
                id: call.id,
                value: call.value,
                seq,
            });
        }
        _ => {
            let _ = tx.send(Cmd::CallbackBatch {
                callbacks: calls,
                seq,
            });
        }
    }
}

/// List click / Enter: `:on-change` then `:on-confirm`, same row id.
pub fn list_activation_calls(
    on_change: Option<String>,
    on_confirm: Option<String>,
    row_id: impl Into<String>,
) -> Vec<CallbackCall> {
    let row_id = row_id.into();
    let mut calls = Vec::new();
    if let Some(id) = on_change {
        calls.push(CallbackCall::with_value(id, json!(row_id.clone())));
    }
    if let Some(id) = on_confirm {
        calls.push(CallbackCall::with_value(id, json!(row_id)));
    }
    calls
}

/// Table double-click: `:on-change` then `:on-confirm`, same row id.
/// Same payload shape as list activation; crate emits SelectRow then
/// DoubleClickedRow from one `on_row_left_click`.
pub fn table_activation_calls(
    on_change: Option<String>,
    on_confirm: Option<String>,
    row_id: impl Into<String>,
) -> Vec<CallbackCall> {
    list_activation_calls(on_change, on_confirm, row_id)
}

/// Coalesce gpui-component 0.5.1 table `SelectRow` + optional
/// `DoubleClickedRow` from one `on_row_left_click`.
///
/// Crate order in `TableState::on_row_left_click`:
/// `set_selected_row` (always `cx.emit(SelectRow)` then `cx.notify()`),
/// then if `click_count() == 2`, `cx.emit(DoubleClickedRow)`.
/// `Context::emit` queues `Effect::Emit`. A count-1 click is only
/// `SelectRow`. A count-2 click is `SelectRow` then `DoubleClickedRow`
/// from that same call. The host records the select and flushes a lone
/// `:on-change` on the **next GPUI frame** so a same-click
/// `DoubleClickedRow` can consume it first and send one
/// `:on-change` + `:on-confirm` batch. No timers or debounce windows.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TableClickCoalesce {
    pending_row: Option<usize>,
    frame_flush_scheduled: bool,
}

impl TableClickCoalesce {
    /// Returns whether the caller should schedule a next-frame
    /// single-select flush. `false` when suppressed or a frame flush
    /// is already pending for this table.
    pub fn on_select_row(&mut self, row_ix: usize, suppress: bool) -> bool {
        if suppress {
            self.pending_row = None;
            return false;
        }
        self.pending_row = Some(row_ix);
        if self.frame_flush_scheduled {
            return false;
        }
        self.frame_flush_scheduled = true;
        true
    }

    /// `true` when `SelectRow` for this row is already pending, so the
    /// activation batch should include `:on-change`. Consumes the pending
    /// row so a next-frame single-select flush is a no-op. A double-click
    /// for a different row leaves the pending select in place.
    pub fn on_double_clicked_row(&mut self, row_ix: usize) -> bool {
        if self.pending_row == Some(row_ix) {
            self.pending_row = None;
            true
        } else {
            false
        }
    }

    pub fn take_pending_select(&mut self) -> Option<usize> {
        self.frame_flush_scheduled = false;
        self.pending_row.take()
    }
}

/// Menu row: item `:on-click` (0-arg) then menu `:on-change` (item id).
pub fn menu_selection_calls(
    item_click: Option<String>,
    on_change: Option<String>,
    item_id: impl Into<String>,
) -> Vec<CallbackCall> {
    let mut calls = Vec::new();
    if let Some(id) = item_click {
        calls.push(CallbackCall::fire(id));
    }
    if let Some(id) = on_change {
        calls.push(CallbackCall::with_value(id, json!(item_id.into())));
    }
    calls
}

/// Dialog OK or Cancel chain flushed from `on_close`.
/// OK: on-ok, on-close, on-open-change false.
/// Cancel: on-cancel, on-close, on-open-change false.
///
/// Overlay accumulates the same order from crate `on_ok`/`on_cancel` then
/// `on_close`; this helper is the documented sequence for tests.
#[cfg_attr(not(test), allow(dead_code))]
pub fn dialog_action_calls(
    first: Option<String>,
    on_close: Option<String>,
    on_open_change: Option<String>,
) -> Vec<CallbackCall> {
    let mut calls = Vec::new();
    if let Some(id) = first {
        calls.push(CallbackCall::fire(id));
    }
    if let Some(id) = on_close {
        calls.push(CallbackCall::fire(id));
    }
    if let Some(id) = on_open_change {
        calls.push(CallbackCall::with_value(id, json!(false)));
    }
    calls
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
    /// `description-list` item column span. `0` / omitted is 1.
    #[serde(default)]
    pub span: u32,
    /// Nested items for menus and trees.
    #[serde(default)]
    pub items: Vec<Item>,
    /// Table row cells (one string per column). Empty falls back to `label`.
    #[serde(default)]
    pub cells: Vec<String>,
    /// Menu separator row. Also accepted as id `"-"`.
    #[serde(default)]
    pub separator: bool,
    /// Table column width in pixels; tree/menu unused.
    #[serde(default)]
    pub width: Option<f32>,
    /// Menu item check mark; tree unused.
    #[serde(default)]
    pub checked: Option<bool>,
    /// Menu item icon (kebab name).
    #[serde(default)]
    pub icon: Option<String>,
    /// Tree item expanded on first paint.
    #[serde(default)]
    pub expanded: bool,
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

    pub fn is_separator(&self) -> bool {
        self.separator
            || self.id.as_deref() == Some("-")
            || self.label.as_deref() == Some("-")
            || self.text.as_deref() == Some("-")
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
    /// `description-list` grid columns (1–10). Omitted is 1, not the crate's 3.
    #[serde(default)]
    pub columns: Option<u32>,
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
    /// Controlled overlay/popover open flag (`:open?` on the Clojure side).
    #[serde(default)]
    pub open: Option<bool>,
    #[serde(default, rename = "on-ok")]
    pub on_ok: Option<String>,
    #[serde(default, rename = "on-cancel")]
    pub on_cancel: Option<String>,
    #[serde(default, rename = "on-confirm")]
    pub on_confirm: Option<String>,
    #[serde(default, rename = "on-open-change")]
    pub on_open_change: Option<String>,
    /// Dialog: click the dimmed overlay to dismiss. Default true.
    /// Crate `confirm()` / `alert()` turn this off; the host restores the default.
    #[serde(default, rename = "overlay-closable")]
    pub overlay_closable: Option<bool>,
    /// Popover / dropdown-menu trigger node (usually a `button`).
    #[serde(default)]
    pub trigger: Option<Box<Node>>,
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
            || self
                .trigger
                .as_ref()
                .is_some_and(|node| node.contains_text(needle))
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
        || item.items.iter().any(|child| item_contains(child, needle))
        || item.cells.iter().any(|cell| cell.contains(needle))
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
    /// Several callbacks from one native action. The worker invokes them
    /// against the same Clojure registry generation, then fetches one tree.
    CallbackBatch {
        callbacks: Vec<CallbackCall>,
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
        let deferred = callback_rpc("cb-7", None, true);
        assert_eq!(deferred["defer-render"], true);
        assert!(callback_request("cb-8", None).get("defer-render").is_none());
    }

    #[test]
    fn defer_render_every_item_of_a_multi_callback_batch() {
        assert_eq!(defer_render_flags(0), Vec::<bool>::new());
        assert_eq!(defer_render_flags(1), vec![false]);
        assert_eq!(defer_render_flags(3), vec![true, true, true]);
    }

    #[test]
    fn send_callbacks_batches_multi_and_skips_empty() {
        let (tx, rx) = std::sync::mpsc::channel();
        send_callbacks(&tx, vec![]);
        assert!(rx.try_recv().is_err());

        send_callbacks(&tx, vec![CallbackCall::fire("cb-1")]);
        match rx.recv().unwrap() {
            Cmd::Callback { id, value, seq } => {
                assert_eq!(id, "cb-1");
                assert!(value.is_none());
                assert!(seq.is_none());
            }
            other => panic!("{other:?}"),
        }

        send_callbacks(
            &tx,
            vec![
                CallbackCall::fire("cb-1"),
                CallbackCall::with_value("cb-2", json!(false)),
            ],
        );
        match rx.recv().unwrap() {
            Cmd::CallbackBatch { callbacks, seq } => {
                assert_eq!(callbacks.len(), 2);
                assert!(seq.is_none());
                assert_eq!(callbacks[0], CallbackCall::fire("cb-1"));
                assert_eq!(callbacks[1].value, Some(json!(false)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn native_action_helpers_preserve_order_and_payloads() {
        let list = list_activation_calls(Some("cb-12".into()), Some("cb-13".into()), "alpha");
        assert_eq!(list[0].id, "cb-12");
        assert_eq!(list[1].id, "cb-13");
        assert_eq!(list[0].value, Some(json!("alpha")));
        assert_eq!(list[1].value, Some(json!("alpha")));

        let ok = dialog_action_calls(
            Some("cb-ok".into()),
            Some("cb-close".into()),
            Some("cb-open".into()),
        );
        assert_eq!(
            ok.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["cb-ok", "cb-close", "cb-open"]
        );
        assert_eq!(ok[2].value, Some(json!(false)));
        assert_eq!(ok[0].value, None);

        let cancel = dialog_action_calls(Some("cb-cancel".into()), Some("cb-close".into()), None);
        assert_eq!(cancel[0].id, "cb-cancel");
        assert_eq!(cancel[1].id, "cb-close");

        let menu = menu_selection_calls(Some("cb-item".into()), Some("cb-menu".into()), "copy");
        assert_eq!(menu[0], CallbackCall::fire("cb-item"));
        assert_eq!(menu[1], CallbackCall::with_value("cb-menu", json!("copy")));

        let table = table_activation_calls(Some("cb-12".into()), Some("cb-13".into()), "ada");
        assert_eq!(
            table,
            list_activation_calls(Some("cb-12".into()), Some("cb-13".into()), "ada")
        );
        assert!(table_activation_calls(None, None, "ada").is_empty());
        assert_eq!(
            table_activation_calls(Some("cb-12".into()), None, "ada").len(),
            1
        );
        assert_eq!(
            table_activation_calls(None, Some("cb-13".into()), "ada").len(),
            1
        );
    }

    #[test]
    fn table_click_coalesce_batches_double_click_not_single_or_suppress() {
        let mut c = TableClickCoalesce::default();
        assert!(c.on_select_row(1, false));
        assert_eq!(c.take_pending_select(), Some(1));
        assert!(c.take_pending_select().is_none());

        assert!(!c.on_select_row(2, true));
        assert!(c.take_pending_select().is_none());

        assert!(c.on_select_row(0, false));
        assert!(c.on_double_clicked_row(0));
        assert!(c.take_pending_select().is_none());

        assert!(c.on_select_row(1, false));
        assert!(!c.on_double_clicked_row(2));
        assert_eq!(
            c.take_pending_select(),
            Some(1),
            "mismatched DoubleClickedRow must not drop the pending select"
        );

        // A Defer that runs before DoubleClickedRow splits one native
        // click into two cmds (the generation-crossing bug). Nested
        // defer lets DoubleClickedRow consume first.
        let mut premature = TableClickCoalesce::default();
        assert!(premature.on_select_row(0, false));
        assert_eq!(premature.take_pending_select(), Some(0));
        assert!(
            !premature.on_double_clicked_row(0),
            "too-early flush leaves confirm as a second standalone callback"
        );
        let mut scheduled = TableClickCoalesce::default();
        assert!(scheduled.on_select_row(0, false));
        assert!(
            !scheduled.on_select_row(0, false),
            "second SelectRow before the frame flush must not stack another callback"
        );
        assert!(scheduled.on_double_clicked_row(0));
        assert!(scheduled.take_pending_select().is_none());

        let (tx, rx) = std::sync::mpsc::channel();
        send_callbacks(
            &tx,
            table_activation_calls(Some("cb-12".into()), Some("cb-13".into()), "grace"),
        );
        match rx.recv().unwrap() {
            Cmd::CallbackBatch { callbacks, .. } => {
                assert_eq!(callbacks[0].id, "cb-12");
                assert_eq!(callbacks[1].id, "cb-13");
                assert_eq!(callbacks[0].value, Some(json!("grace")));
            }
            other => panic!("{other:?}"),
        }
        send_callbacks(
            &tx,
            table_activation_calls(Some("cb-12".into()), None, "grace"),
        );
        match rx.recv().unwrap() {
            Cmd::Callback { id, .. } => assert_eq!(id, "cb-12"),
            other => panic!("{other:?}"),
        }
        send_callbacks(
            &tx,
            table_activation_calls(None, Some("cb-13".into()), "grace"),
        );
        match rx.recv().unwrap() {
            Cmd::Callback { id, .. } => assert_eq!(id, "cb-13"),
            other => panic!("{other:?}"),
        }
        send_callbacks(&tx, table_activation_calls(None, None, "grace"));
        assert!(rx.try_recv().is_err());
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
        assert_eq!(PROTOCOL_VERSION, 5);
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
    fn decodes_description_list_columns_and_span() {
        let node: Node = serde_json::from_value(json!({
            "type": "description-list",
            "orientation": "horizontal",
            "columns": 2,
            "items": [
                {"label": "Host", "text": "GPUI", "span": 2},
                {"label": "UI", "text": "clj-gpui"}
            ]
        }))
        .unwrap();
        assert_eq!(node.orientation.as_deref(), Some("horizontal"));
        assert_eq!(node.columns, Some(2));
        assert_eq!(node.collection()[0].span, 2);
        assert_eq!(node.collection()[1].span, 0);
        let omitted: Node = serde_json::from_value(json!({
            "type": "description-list",
            "items": [{"label": "Host", "text": "GPUI"}]
        }))
        .unwrap();
        assert_eq!(omitted.columns, None);
        assert_eq!(omitted.orientation, None);
        assert_eq!(omitted.collection()[0].span, 0);
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

    #[test]
    fn decodes_v5_overlay_and_row_nodes() {
        let dialog: Node = serde_json::from_value(json!({
            "type": "dialog",
            "open": true,
            "title": "Delete?",
            "variant": "confirm",
            "on-close": "cb-1",
            "on-ok": "cb-2",
            "on-cancel": "cb-3",
            "children": [{"type": "label", "text": "Undo?"}]
        }))
        .unwrap();
        assert_eq!(dialog.kind, "dialog");
        assert_eq!(dialog.open, Some(true));
        assert_eq!(dialog.on_ok.as_deref(), Some("cb-2"));
        assert_eq!(dialog.on_cancel.as_deref(), Some("cb-3"));
        assert_eq!(dialog.overlay_closable, None);
        assert!(dialog.contains_text("Undo?"));

        let popover: Node = serde_json::from_value(json!({
            "type": "popover",
            "open": false,
            "on-open-change": "cb-4",
            "trigger": {"type": "button", "text": "More"},
            "children": [{"type": "label", "text": "Hint"}]
        }))
        .unwrap();
        assert_eq!(popover.open, Some(false));
        assert_eq!(
            popover.trigger.as_ref().map(|n| n.kind.as_str()),
            Some("button")
        );
        assert!(popover.contains_text("Hint"));
        assert!(popover.contains_text("More"));

        let list: Node = serde_json::from_value(json!({
            "type": "list",
            "value": "alpha",
            "searchable": true,
            "on-change": "cb-5",
            "on-confirm": "cb-6",
            "items": [{"id": "alpha", "label": "Alpha"}, {"id": "beta", "label": "Beta"}]
        }))
        .unwrap();
        assert_eq!(list.string_value().as_deref(), Some("alpha"));
        assert!(list.searchable);
        assert_eq!(list.on_confirm.as_deref(), Some("cb-6"));

        let table: Node = serde_json::from_value(json!({
            "type": "table",
            "value": "ada",
            "options": [{"id": "name", "label": "Name", "width": 120}],
            "items": [{"id": "ada", "cells": ["Ada", "Clojure"]}]
        }))
        .unwrap();
        assert_eq!(table.options[0].width, Some(120.0));
        assert_eq!(table.items[0].cells, vec!["Ada", "Clojure"]);
        // `columns` stays the description-list u32; table columns live in `options`.
        assert_eq!(table.columns, None);
        assert!(table.contains_text("Ada"));

        let tree: Node = serde_json::from_value(json!({
            "type": "tree",
            "value": "lib",
            "items": [{
                "id": "src",
                "label": "src",
                "expanded": true,
                "items": [{"id": "lib", "label": "lib.rs"}]
            }]
        }))
        .unwrap();
        assert!(tree.items[0].expanded);
        assert_eq!(tree.items[0].items[0].id_or_label(), "lib");
        assert!(tree.contains_text("lib.rs"));
        assert_eq!(PROTOCOL_VERSION, 5);
    }

    #[test]
    fn menu_separator_and_nested_items() {
        let node: Node = serde_json::from_value(json!({
            "type": "dropdown-menu",
            "items": [
                {"id": "copy", "label": "Copy"},
                {"separator": true},
                {
                    "id": "more",
                    "label": "More",
                    "items": [{"id": "paste", "label": "Paste"}]
                }
            ]
        }))
        .unwrap();
        assert!(node.items[1].is_separator());
        assert_eq!(node.items[2].items[0].id_or_label(), "paste");
    }
}
