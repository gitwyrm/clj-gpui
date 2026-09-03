use gpui_component::theme::ThemeSet;
use gpui_kit::component as gpui_component;
use serde::Deserialize;
use serde_json::{Value, json};

pub const PROTOCOL_VERSION: u64 = 10;

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

/// Combobox pick / popover close: `:on-change` then `:on-confirm`, same
/// payload (one id, a JSON array when `:multiple`, or `null`).
pub fn combobox_activation_calls(
    on_change: Option<String>,
    on_confirm: Option<String>,
    payload: Value,
) -> Vec<CallbackCall> {
    let mut calls = Vec::new();
    if let Some(id) = on_change {
        calls.push(CallbackCall::with_value(id, payload.clone()));
    }
    if let Some(id) = on_confirm {
        calls.push(CallbackCall::with_value(id, payload));
    }
    calls
}

/// Coalesce gpui-component 0.5.1 table `SelectRow` + optional
/// `DoubleClickedRow` from one `on_row_left_click`.
///
/// Crate order in `TableState::on_row_left_click`:
/// `set_selected_row` (always `cx.emit(SelectRow)` then `cx.notify()`),
/// then if `click_count() == 2`, `cx.emit(DoubleClickedRow)`.
/// `Context::emit` only queues `Effect::Emit`; subscribers run later in
/// `flush_effects` FIFO. After that call returns the queue is:
/// `Emit(SelectRow)`, `Notify`, then maybe `Emit(DoubleClickedRow)`.
///
/// The host records the select and schedules `Effect::Defer` (`cx.defer_in`)
/// to flush a lone `:on-change`. Defer is pushed behind those already-queued
/// emits, so a same-click `DoubleClickedRow` runs first, consumes the
/// pending row, and sends one `:on-change` + `:on-confirm` batch. A
/// count-1 click has no second emit; the deferred flush sends `:on-change`.
/// No timers, sleeps, or debounce windows.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TableClickCoalesce {
    pending_row: Option<usize>,
    flush_scheduled: bool,
}

impl TableClickCoalesce {
    /// Returns whether the caller should schedule a deferred single-select
    /// flush. `false` when suppressed or a flush is already pending.
    pub fn on_select_row(&mut self, row_ix: usize, suppress: bool) -> bool {
        if suppress {
            self.pending_row = None;
            return false;
        }
        self.pending_row = Some(row_ix);
        if self.flush_scheduled {
            return false;
        }
        self.flush_scheduled = true;
        true
    }

    /// `true` when `SelectRow` for this row is already pending, so the
    /// activation batch should include `:on-change`. Consumes the pending
    /// row so the deferred single-select flush is a no-op. A double-click
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
        self.flush_scheduled = false;
        self.pending_row.take()
    }
}

/// Coalesce Kit `ComboboxEvent::Change` + optional `ComboboxEvent::Confirm`
/// from one user action.
///
/// Single-select pick: Kit emits Change then Confirm from the same
/// `toggle` (`should_close = changed && !multiple`). `Context::emit`
/// only queues `Effect::Emit`; subscribers run later in `flush_effects`
/// FIFO. Record Change and schedule `Effect::Defer` (`cx.defer_in`) to
/// flush a lone `:on-change`. Defer is pushed behind those already-queued
/// emits, so a same-action Confirm runs first, consumes the pending
/// payload, and sends one `:on-change` + `:on-confirm` batch. Confirm
/// without Change (dismiss / close) has no pending payload. Multiple
/// mode Change (popover stays open) has no same-tick Confirm; the
/// deferred flush sends `:on-change` alone.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ComboboxActivationCoalesce {
    pending: Option<Value>,
    flush_scheduled: bool,
}

impl ComboboxActivationCoalesce {
    /// Returns whether the caller should schedule a deferred change-only
    /// flush. `false` when a flush is already pending.
    pub fn on_change(&mut self, payload: Value) -> bool {
        self.pending = Some(payload);
        if self.flush_scheduled {
            return false;
        }
        self.flush_scheduled = true;
        true
    }

    /// Consumes a pending Change so the deferred flush is a no-op. The
    /// returned payload (when present) is `:on-change` in the Confirm batch.
    pub fn on_confirm(&mut self) -> Option<Value> {
        self.pending.take()
    }

    pub fn take_pending_change(&mut self) -> Option<Value> {
        self.flush_scheduled = false;
        self.pending.take()
    }
}

/// Coalesce `InputEvent::Change` across one GPUI effect flush **and**
/// across the Clojure callback round-trip.
///
/// gpui-component `InputState::undo` / `redo` applies every history item
/// in a version group, and each `replace_text_in_range` emits `Change`.
/// Fast typing does the same across consecutive frames. Sending one
/// `Cmd::Callback` per emit lets the bridge `export-tree` after the
/// first, so later callbacks are unknown ids (`cb-N` is monotonic).
///
/// Record the latest value, schedule at most one deferred flush, and
/// after that send stay `in_flight` until the next tree assigns a new
/// `:on-change` id. Further edits only update `pending`; the refreshed
/// id flushes the final string. Same idea as `TableClickCoalesce`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InputChangeCoalesce {
    pending: Option<String>,
    flush_scheduled: bool,
    in_flight: bool,
}

impl InputChangeCoalesce {
    /// Returns whether the caller should schedule a deferred flush.
    pub fn on_change(&mut self, value: String) -> bool {
        self.pending = Some(value);
        if self.flush_scheduled || self.in_flight {
            return false;
        }
        self.flush_scheduled = true;
        true
    }

    /// Take the value to send. Marks the slot in-flight so another
    /// callback is not scheduled until `on_ids_refreshed`.
    pub fn take_pending(&mut self) -> Option<String> {
        self.flush_scheduled = false;
        let value = self.pending.take()?;
        self.in_flight = true;
        Some(value)
    }

    /// New export assigned a fresh `:on-change` id. Returns whether to
    /// schedule a flush for edits that arrived during the round-trip.
    pub fn on_ids_refreshed(&mut self) -> bool {
        self.in_flight = false;
        if self.pending.is_some() && !self.flush_scheduled {
            self.flush_scheduled = true;
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.pending = None;
        self.flush_scheduled = false;
        self.in_flight = false;
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
    /// Declarative `table` shorthand columns may still send this; the
    /// Clojure expander copies it onto the header cell only.
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
    /// Table column / cell text alignment (`start` / `center` / `end`).
    #[serde(default)]
    pub align: Option<String>,
    /// Menu item check mark; tree unused.
    #[serde(default)]
    pub checked: Option<bool>,
    /// Menu item icon (kebab name).
    #[serde(default)]
    pub icon: Option<String>,
    /// Tree item expanded on first paint.
    #[serde(default)]
    pub expanded: bool,
    /// Chart y / settings field / numeric cell. JSON number, string, or bool.
    #[serde(default)]
    pub value: Option<Value>,
    /// Virtual-list row height in pixels.
    #[serde(default)]
    pub height: Option<f32>,
    /// Dock panel side: `left`, `right`, `bottom`, `center`.
    #[serde(default)]
    pub side: Option<String>,
    /// Settings field type (`switch`, `input`, `number`, `dropdown`).
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub min: Option<f32>,
    #[serde(default)]
    pub max: Option<f32>,
    #[serde(default)]
    pub step: Option<f32>,
    /// Chart item fill (hex). Bar / pie / sankey node.
    #[serde(default)]
    pub color: Option<String>,
    /// Radar series values when a dimension has more than one number.
    /// A JSON array on `value` is also accepted.
    #[serde(default)]
    pub values: Option<Value>,
    /// Candlestick OHLC. Omitted on other items.
    #[serde(default)]
    pub open: Option<f32>,
    #[serde(default)]
    pub high: Option<f32>,
    #[serde(default)]
    pub low: Option<f32>,
    #[serde(default)]
    pub close: Option<f32>,
    /// Sankey link endpoints (node id or label).
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    /// Area / radar series fill (hex). Distinct from `color` (stroke / bar fill).
    #[serde(default)]
    pub fill: Option<String>,
    /// Line / area series stroke style: `natural`, `linear`, `step-after`.
    #[serde(default, rename = "stroke-style")]
    pub stroke_style: Option<String>,
    /// Sankey custom label lines. When any node sets this, Kit `.labels()` wins.
    #[serde(default, rename = "label-lines")]
    pub label_lines: Vec<ChartLabelLine>,
}

/// One line of a custom Sankey node label (`SankeyLabel`).
#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
pub struct ChartLabelLine {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default, rename = "font-size")]
    pub font_size: Option<f32>,
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

    pub fn number_value(&self) -> Option<f32> {
        match &self.value {
            Some(Value::Number(n)) => n.as_f64().map(|n| n as f32),
            Some(Value::String(s)) => s.parse().ok(),
            _ => self.text.as_deref().and_then(|s| s.parse().ok()),
        }
    }

    pub fn string_value(&self) -> Option<String> {
        match &self.value {
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Number(n)) => Some(n.to_string()),
            Some(Value::Bool(b)) => Some(b.to_string()),
            Some(Value::Null) | None => self.text.clone(),
            Some(other) => Some(other.to_string()),
        }
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
    /// `window` (or any root): `"dev"` (default, nREPL footer + fps HUD)
    /// or `"app"` (no host chrome).
    #[serde(default)]
    pub chrome: Option<String>,
    #[serde(default, rename = "window-width")]
    pub window_width: Option<f32>,
    #[serde(default, rename = "window-height")]
    pub window_height: Option<f32>,
    /// Text input: request keyboard focus when true.
    #[serde(default)]
    pub focus: bool,
    /// Checkbox: `"circle"` for a round toggle. Omitted is the square widget.
    #[serde(default)]
    pub shape: Option<String>,
    /// Textarea visible rows. Omitted is 3.
    #[serde(default)]
    pub rows: Option<u32>,
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
    /// `confirm` dialogs follow Kit (not overlay-closable unless set).
    /// `alert-dialog` never dismisses on backdrop.
    #[serde(default, rename = "overlay-closable")]
    pub overlay_closable: Option<bool>,
    /// Popover / dropdown-menu trigger node (usually a `button`).
    #[serde(default)]
    pub trigger: Option<Box<Node>>,
    /// Sheet slide edge: `left`, `right`, `top`, `bottom`.
    #[serde(default)]
    pub placement: Option<String>,
    /// Notification auto-hide (default true).
    #[serde(default)]
    pub autohide: Option<bool>,
    /// Code editor highlighter language (`rust`, `clojure`, …).
    #[serde(default)]
    pub language: Option<String>,
    /// OTP masked cells.
    #[serde(default)]
    pub masked: bool,
    /// Sidebar collapsed chrome.
    #[serde(default)]
    pub collapsed: bool,
    /// Sidebar / dock side: `left` or `right`.
    #[serde(default)]
    pub side: Option<String>,
    /// Date display format, or markdown vs `html`.
    #[serde(default)]
    pub format: Option<String>,
    /// Date picker range mode.
    #[serde(default)]
    pub range: bool,
    /// Sheet footer node.
    #[serde(default)]
    pub footer: Option<Box<Node>>,
    /// `table-head` / `table-cell` Kit `col_span`. `0` / omitted is 1.
    #[serde(default)]
    pub span: u32,
    /// Kit `Table::accessibility_label`. Caption is visible text and is
    /// not used as the accessible name.
    #[serde(default, rename = "accessibility-label")]
    pub accessibility_label: Option<String>,
    /// `BarChart` alignment: `bottom` (default), `top`, `left`, `right`.
    /// `left` is horizontal bars growing right (ncdu / cljdu).
    #[serde(default)]
    pub alignment: Option<String>,
    /// Chart: show band-axis labels. Default true.
    #[serde(default, rename = "label-axis")]
    pub label_axis: Option<bool>,
    /// Chart: show value-axis tick labels. Default false.
    #[serde(default, rename = "value-axis")]
    pub value_axis: Option<bool>,
    /// Chart: stride over band-axis category labels. Default 1.
    #[serde(default, rename = "tick-margin")]
    pub tick_margin: Option<u32>,
    /// Chart: value-axis intervals. Default 4.
    #[serde(default, rename = "value-tick-count")]
    pub value_tick_count: Option<u32>,
    /// Chart: grid lines. Default true.
    #[serde(default)]
    pub grid: Option<bool>,
    /// Bar chart: paint numeric labels on bars. Default false.
    #[serde(default)]
    pub labels: Option<bool>,
    /// Sankey links. Nodes stay on `items`.
    #[serde(default)]
    pub links: Vec<Item>,
    /// Radar / area series names, colors, fills, in value-index order.
    #[serde(default)]
    pub series: Vec<Item>,
    /// Sankey `SankeyAlign`: `justify` (default), `left`, `right`, `center`.
    #[serde(default, rename = "node-align")]
    pub node_align: Option<String>,
    /// Sankey `SankeyValueScale`: `linear` (default) or `sqrt`.
    #[serde(default, rename = "value-scale")]
    pub value_scale: Option<String>,
    /// Chart tooltip series name (`LineChart` / `BarChart` `.name()`).
    #[serde(default)]
    pub name: Option<String>,
    /// Line / area stroke color (hex). Not layout `:color`.
    #[serde(default)]
    pub stroke: Option<String>,
    /// Line / area stroke style: `natural` (default), `linear`, `step-after`.
    #[serde(default, rename = "stroke-style")]
    pub stroke_style: Option<String>,
    /// Line / area / candlestick x-axis. Default true. Not bar `label-axis`.
    #[serde(default, rename = "x-axis")]
    pub x_axis: Option<bool>,
    /// Bar uniform corner radius in pixels. `corner-radii` wins when both set.
    #[serde(default, rename = "corner-radius")]
    pub corner_radius: Option<f32>,
    /// Bar `Corners`: number (uniform) or `{top-left, top-right, bottom-right, bottom-left}`.
    #[serde(default, rename = "corner-radii")]
    pub corner_radii: Option<Value>,
    /// Bar `fill_gradient`: `true` / `"bar"` per-bar, `"chart"` chart-wide, or two stops.
    #[serde(default, rename = "fill-gradient")]
    pub fill_gradient: Option<Value>,
    /// Bar fill-gradient helper when `fill-gradient` is `true`: `bar` (default) or `chart`.
    #[serde(default, rename = "fill-gradient-mode")]
    pub fill_gradient_mode: Option<String>,
    /// Pie inner radius in pixels (donut). Kit default 0.
    #[serde(default, rename = "inner-radius")]
    pub inner_radius: Option<f32>,
    /// Pie / radar outer radius in pixels. Omitted uses Kit's height×0.4 default.
    #[serde(default, rename = "outer-radius")]
    pub outer_radius: Option<f32>,
    /// Pie pad angle.
    #[serde(default, rename = "pad-angle")]
    pub pad_angle: Option<f32>,
    /// Pie / radar / sankey label color (hex).
    #[serde(default, rename = "label-color")]
    pub label_color: Option<String>,
    /// Pie label leader-line color (hex).
    #[serde(default, rename = "label-line-color")]
    pub label_line_color: Option<String>,
    /// Pie / radar / sankey label gap in pixels.
    #[serde(default, rename = "label-gap")]
    pub label_gap: Option<f32>,
    /// Radar concentric grid rings. Kit default 4; Kit clamps to ≥1.
    #[serde(default, rename = "grid-levels")]
    pub grid_levels: Option<u32>,
    /// Candlestick body width as a fraction of the band. Kit default 0.8.
    #[serde(default, rename = "body-width-ratio")]
    pub body_width_ratio: Option<f32>,
    /// Sankey node rectangle width. Kit default 10.
    #[serde(default, rename = "node-width")]
    pub node_width: Option<f32>,
    /// Sankey vertical gap between nodes in a column. Kit default 16.
    #[serde(default, rename = "node-padding")]
    pub node_padding: Option<f32>,
    /// Sankey relaxation passes. Kit default 6.
    #[serde(default)]
    pub iterations: Option<u32>,
    /// Sankey node corner radius in pixels. Kit default 0.
    #[serde(default, rename = "node-corner-radius")]
    pub node_corner_radius: Option<f32>,
    /// Sankey link ribbon opacity. Kit default 0.3.
    #[serde(default, rename = "link-opacity")]
    pub link_opacity: Option<f32>,
    /// Sankey minimum ribbon thickness. Kit default 1.
    #[serde(default, rename = "min-link-width")]
    pub min_link_width: Option<f32>,
    /// Sankey name labels. Default true (convenience; Kit draws none unless set).
    #[serde(default, rename = "node-label")]
    pub node_label: Option<bool>,
    /// Sankey value labels. Default true (convenience; Kit draws none unless set).
    #[serde(default, rename = "value-label")]
    pub value_label: Option<bool>,
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
            || self.links.iter().any(|item| item_contains(item, needle))
            || self.series.iter().any(|item| item_contains(item, needle))
            || self
                .trigger
                .as_ref()
                .is_some_and(|node| node.contains_text(needle))
            || self
                .footer
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
        /// Set on input submit so the following tree can force-sync that field.
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
    PreviewCaptured {
        request_id: String,
        png: Option<String>,
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
    CapturePreview {
        request_id: String,
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

        let combo =
            combobox_activation_calls(Some("cb-12".into()), Some("cb-13".into()), json!("clj"));
        assert_eq!(combo[0].id, "cb-12");
        assert_eq!(combo[1].id, "cb-13");
        assert_eq!(combo[0].value, Some(json!("clj")));
        assert_eq!(combo[1].value, Some(json!("clj")));
        assert!(combobox_activation_calls(None, None, json!("clj")).is_empty());
        assert_eq!(
            combobox_activation_calls(Some("cb-12".into()), None, json!(["clj", "rs"])).len(),
            1
        );
    }

    #[test]
    fn combobox_activation_coalesce_batches_confirm_after_change() {
        let mut c = ComboboxActivationCoalesce::default();
        assert!(c.on_change(json!("clj")));
        assert!(
            !c.on_change(json!("clj")),
            "second Change must not stack another defer"
        );
        let pending = c.on_confirm();
        assert_eq!(pending, Some(json!("clj")));
        assert!(
            c.take_pending_change().is_none(),
            "Confirm consumes Change so the deferred flush is a no-op"
        );

        let mut change_only = ComboboxActivationCoalesce::default();
        assert!(change_only.on_change(json!("rs")));
        assert_eq!(change_only.take_pending_change(), Some(json!("rs")));

        let mut confirm_only = ComboboxActivationCoalesce::default();
        assert!(confirm_only.on_confirm().is_none());
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

        // A flush that runs before DoubleClickedRow splits one native
        // click into two cmds (the generation-crossing bug). Defer is
        // queued behind the already-pushed DoubleClickedRow emit.
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
            "second SelectRow before the deferred flush must not stack another callback"
        );
        assert!(scheduled.on_double_clicked_row(0));
        assert!(scheduled.take_pending_select().is_none());
        assert!(
            scheduled.on_select_row(1, false),
            "deferred flush must clear the schedule so a later click can fire"
        );

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
    fn input_change_coalesce_keeps_latest_value_and_one_flush() {
        let mut c = InputChangeCoalesce::default();
        assert!(c.on_change("a".into()));
        assert!(
            !c.on_change("ab".into()),
            "grouped undo emits one Change per history item; only the first schedules a flush"
        );
        assert!(!c.on_change("".into()));
        assert_eq!(c.take_pending(), Some("".into()));
        assert!(c.take_pending().is_none());
        assert!(
            !c.on_change("x".into()),
            "after send, further Changes wait for a new callback id"
        );
        assert!(
            c.on_ids_refreshed(),
            "tree apply with leftover text schedules one flush on the new id"
        );
        assert_eq!(c.take_pending(), Some("x".into()));
        assert!(
            !c.on_ids_refreshed(),
            "a tree with no pending edits must not emit again"
        );
        assert!(
            c.on_change("y".into()),
            "after ids refresh with no leftover, a later Change can schedule"
        );
        assert_eq!(c.take_pending(), Some("y".into()));
    }

    #[test]
    fn input_change_coalesce_clear_unsticks_in_flight() {
        let mut c = InputChangeCoalesce::default();
        assert!(c.on_change("a".into()));
        assert_eq!(c.take_pending(), Some("a".into()));
        c.clear();
        assert!(
            c.on_change("b".into()),
            "clear drops in-flight so a new edit can flush"
        );
        assert_eq!(c.take_pending(), Some("b".into()));
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
        assert_eq!(PROTOCOL_VERSION, 10);
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
    fn decodes_table_cell_span_on_the_node() {
        let node: Node = serde_json::from_value(json!({
            "type": "table-cell",
            "span": 2,
            "align": "end",
            "children": [{"type": "label", "text": "Total"}]
        }))
        .unwrap();
        assert_eq!(node.span, 2);
        assert_eq!(node.align.as_deref(), Some("end"));
        assert_eq!(node.children[0].text.as_deref(), Some("Total"));
        let omitted: Node = serde_json::from_value(json!({
            "type": "table-head",
            "children": [{"type": "label", "text": "Name"}]
        }))
        .unwrap();
        assert_eq!(omitted.span, 0);
    }

    #[test]
    fn decodes_table_accessibility_label() {
        let node: Node = serde_json::from_value(json!({
            "type": "table",
            "accessibility-label": "Recent invoices",
            "children": [{"type": "table-caption", "children": [{"type": "label", "text": "Invoices"}]}]
        }))
        .unwrap();
        assert_eq!(node.accessibility_label.as_deref(), Some("Recent invoices"));
        let omitted: Node = serde_json::from_value(json!({"type": "table"})).unwrap();
        assert_eq!(omitted.accessibility_label, None);
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
            "type": "data-table",
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
        assert_eq!(PROTOCOL_VERSION, 10);
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

    #[test]
    fn decodes_v6_product_nodes() {
        let sheet: Node = serde_json::from_value(json!({
            "type": "sheet",
            "open": true,
            "placement": "left",
            "title": "Inspect",
            "footer": {"type": "button", "text": "Done"}
        }))
        .unwrap();
        assert_eq!(sheet.placement.as_deref(), Some("left"));
        assert!(sheet.contains_text("Done"));

        let note: Node = serde_json::from_value(json!({
            "type": "notification",
            "variant": "success",
            "title": "Saved",
            "message": "ok",
            "autohide": false
        }))
        .unwrap();
        assert_eq!(note.autohide, Some(false));
        assert_eq!(note.message.as_deref(), Some("ok"));

        let chart: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "line",
            "items": [{"id": "a", "label": "A", "value": 3.5}]
        }))
        .unwrap();
        assert_eq!(chart.items[0].number_value(), Some(3.5));

        let hbar: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "bar",
            "alignment": "left",
            "labels": true,
            "value-axis": true,
            "items": [{"id": "src", "label": "src", "value": 412, "color": "#3366ff"}]
        }))
        .unwrap();
        assert_eq!(hbar.alignment.as_deref(), Some("left"));
        assert_eq!(hbar.labels, Some(true));
        assert_eq!(hbar.value_axis, Some(true));
        assert_eq!(hbar.items[0].color.as_deref(), Some("#3366ff"));

        let candle: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "candlestick",
            "items": [{"label": "Mon", "open": 100, "high": 110, "low": 95, "close": 105}]
        }))
        .unwrap();
        assert_eq!(candle.items[0].open, Some(100.0));
        assert_eq!(candle.items[0].close, Some(105.0));

        let sankey: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "sankey",
            "node-align": "left",
            "value-scale": "sqrt",
            "items": [{"id": "rev", "label": "Revenue"}],
            "links": [{"source": "rev", "target": "cost", "value": 55}]
        }))
        .unwrap();
        assert_eq!(sankey.node_align.as_deref(), Some("left"));
        assert_eq!(sankey.value_scale.as_deref(), Some("sqrt"));
        assert_eq!(sankey.links[0].source.as_deref(), Some("rev"));
        assert_eq!(sankey.links[0].number_value(), Some(55.0));

        let radar: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "radar",
            "items": [{"label": "Speed", "values": [80, 60]}],
            "series": [{"id": "desktop", "label": "Desktop"}]
        }))
        .unwrap();
        assert!(radar.items[0].values.as_ref().unwrap().is_array());
        assert_eq!(radar.series[0].label_or_id(), "Desktop");

        let date: Node = serde_json::from_value(json!({
            "type": "date-picker",
            "value": "2026-09-02",
            "range": true
        }))
        .unwrap();
        assert!(date.range);
        assert_eq!(date.string_value().as_deref(), Some("2026-09-02"));

        let table: Node = serde_json::from_value(json!({
            "type": "table",
            "text": "Invoices",
            "options": [{"label": "Amount", "align": "end", "width": 80.0}],
            "items": [
                {"cells": ["Ada", "$250"]},
                {"cells": ["Total"], "variant": "footer"}
            ]
        }))
        .unwrap();
        assert_eq!(table.text.as_deref(), Some("Invoices"));
        assert_eq!(table.options[0].align.as_deref(), Some("end"));
        assert_eq!(table.items[1].variant.as_deref(), Some("footer"));
    }
}
