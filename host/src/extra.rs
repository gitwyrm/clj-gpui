//! Product widgets that sit on the v6 protocol: dates, colors, charts,
//! markdown, virtual lists, settings fields, and dock panels.
//!
//! Overlay sheet/notification collection lives in `overlay`. Slot maps and
//! `RootView::render_node` arms stay in `renderer`.

use crate::mapping;
use crate::protocol::{self, Cmd, Item, Node};
use chrono::NaiveDate;
use gpui::{
    App, Axis, Context, Entity, EventEmitter, FocusHandle, Focusable, Hsla, IntoElement,
    ParentElement, Render, SharedString, Styled, Window, div, prelude::*, px, size,
};
use gpui_component::{
    ActiveTheme as _, Colorize as _, Placement, Side, VirtualListScrollHandle,
    calendar::Date,
    chart::{AreaChart, BarChart, CandlestickChart, LineChart, PieChart, RadarChart, SankeyChart},
    dock::{Panel as StyledPanel, PanelControl, PanelEvent},
    h_virtual_list,
    input::InputState,
    plot::shape::{BarAlignment, SankeyAlign, SankeyLink, SankeyValueScale},
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings},
    text::TextView,
    v_flex, v_virtual_list,
};
use gpui_kit as gpui;
use gpui_kit::component as gpui_component;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::mpsc;

pub fn parse_iso_date(text: &str) -> Option<NaiveDate> {
    let text = text.trim();
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(text, "%Y/%m/%d"))
        .ok()
}

pub fn format_iso_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

pub fn parse_hex_color(text: &str) -> Option<Hsla> {
    Hsla::parse_hex(text.trim()).ok()
}

pub fn format_hex_color(color: Hsla) -> String {
    color.to_hex()
}

pub fn color_from_node(node: &Node) -> Option<Hsla> {
    node.string_value()
        .or_else(|| node.text.clone())
        .and_then(|s| parse_hex_color(&s))
}

pub fn color_event_payload(color: Option<Hsla>) -> Value {
    match color.map(format_hex_color) {
        Some(hex) => json!(hex),
        None => Value::Null,
    }
}

/// How to sync a reused `ColorPickerState` with Clojure's controlled value.
///
/// gpui-component 0.5.1 `set_value` takes a concrete `Hsla` (`update_value`
/// is private). Clearing `Some(color)` → `None` recreates the state entity
/// so we do not fake a transparent/black swatch. Recreate does not emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSync {
    Keep,
    Set,
    RecreateClear,
}

pub fn color_sync(wanted: Option<Hsla>, current: Option<Hsla>) -> ColorSync {
    match (wanted, current) {
        (Some(wanted), Some(current)) if format_hex_color(wanted) == format_hex_color(current) => {
            ColorSync::Keep
        }
        (Some(_), _) => ColorSync::Set,
        (None, None) => ColorSync::Keep,
        (None, Some(_)) => ColorSync::RecreateClear,
    }
}

/// Text-field payload vs number-input payload for a reused `InputSlot`.
pub fn input_change_payload(as_number: bool, text: &str) -> Option<Value> {
    if as_number {
        number_from_input(text).map(|n| json!(n))
    } else {
        Some(json!(text))
    }
}

/// Kit `TextareaState` defaults `submit_on_enter` to false: Enter inserts a
/// newline and still emits `PressEnter`. Enable it when Clojure provides
/// `:on-submit` so Enter submits and Shift+Enter inserts a newline.
pub fn textarea_submit_on_enter(on_submit: Option<&str>) -> bool {
    on_submit.is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAlign {
    Start,
    Center,
    End,
}

/// Declarative `ui/table` column/cell alignment. `end` / `right` are Kit
/// `text_right`; omitted is start.
pub fn table_align_name(align: Option<&str>) -> TableAlign {
    match align.map(|s| s.to_ascii_lowercase()) {
        Some(s) if s == "end" || s == "right" => TableAlign::End,
        Some(s) if s == "center" => TableAlign::Center,
        _ => TableAlign::Start,
    }
}

pub fn table_align(item: &Item) -> TableAlign {
    table_align_name(item.align.as_deref())
}

pub fn table_align_node(node: &Node) -> TableAlign {
    table_align_name(node.align.as_deref())
}

/// Kit `Table::accessibility_label`. Empty / omitted is unset; a visible
/// `TableCaption` is not used as the accessible name.
pub fn table_accessibility_label(node: &Node) -> Option<&str> {
    node.accessibility_label
        .as_deref()
        .filter(|s| !s.is_empty())
}

/// Last row with `variant: "footer"` is Kit `TableFooter`; the rest is body.
pub fn split_table_footer(items: &[Item]) -> (&[Item], Option<&Item>) {
    match items.split_last() {
        Some((last, rest)) if last.variant.as_deref() == Some("footer") => (rest, Some(last)),
        _ => (items, None),
    }
}

/// Kit `Rating` value is `0..=max`. `:max` omitted is 5.
pub fn rating_max(node: &Node) -> usize {
    node.max.unwrap_or(5.0).round().clamp(1.0, 32.0) as usize
}

pub fn rating_value(node: &Node) -> usize {
    let max = rating_max(node);
    let raw = match &node.value {
        Some(Value::Number(n)) => n.as_u64().unwrap_or(0) as usize,
        Some(Value::String(s)) => s.parse().unwrap_or(0),
        _ => 0,
    };
    raw.min(max)
}

/// Arguments for `Rating::new().max(max).value(value)`.
///
/// Kit `Rating::value` clamps to the *current* max (default 5). Applying
/// `.value(8).max(10)` stores 5. Host must call `.max` first.
pub fn rating_max_then_value(node: &Node) -> (usize, usize) {
    let max = rating_max(node);
    (max, rating_value(node))
}

/// Stepper selected index from a wire id, falling back to a numeric index.
pub fn stepper_selected_index(items: &[Item], value: Option<&str>) -> usize {
    let Some(value) = value else {
        return 0;
    };
    if let Some(ix) = items.iter().position(|item| item.id_or_label() == value) {
        return ix;
    }
    value
        .parse::<usize>()
        .ok()
        .filter(|ix| *ix < items.len())
        .unwrap_or(0)
}

/// Combobox `:on-change` payload: a JSON array when `:multiple`, else one
/// id or `null`.
pub fn combobox_payload(multiple: bool, values: &[SharedString]) -> Value {
    if multiple {
        json!(values.iter().map(|v| v.to_string()).collect::<Vec<_>>())
    } else {
        values
            .first()
            .map(|v| json!(v.to_string()))
            .unwrap_or(Value::Null)
    }
}

/// Identity of combobox options. Used to skip `set_items` when Clojure
/// rerenders an unchanged collection.
pub fn combobox_fingerprint(items: &[Item]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    items.len().hash(&mut hasher);
    for item in items {
        item.id.hash(&mut hasher);
        item.label.hash(&mut hasher);
        item.text.hash(&mut hasher);
        item.disabled.hash(&mut hasher);
    }
    hasher.finish()
}

/// Whether a reused combobox slot should push items / selection into Kit.
///
/// `set_items` only replaces the delegate. Kit's cloned selection keeps
/// old labels and dropped ids unless `set_selected_values` rebuilds it,
/// so an item-collection change also sets `set_selected`.
///
/// `set_selected_values` clears the search query. Skip it when neither
/// the collection nor the controlled ids changed. After a native
/// `ComboboxEvent::Change`, the slot cache must already hold those ids
/// so a Clojure echo of the same selection is a no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComboboxSlotSync {
    pub set_items: bool,
    pub set_selected: bool,
}

pub fn combobox_slot_sync(
    prev_fingerprint: u64,
    next_fingerprint: u64,
    prev_selected: &[SharedString],
    next_selected: &[SharedString],
) -> ComboboxSlotSync {
    let set_items = prev_fingerprint != next_fingerprint;
    ComboboxSlotSync {
        set_items,
        set_selected: set_items || prev_selected != next_selected,
    }
}

pub fn date_from_value(value: &Option<Value>, range: bool) -> Date {
    match value {
        Some(Value::String(s)) => {
            let date = parse_iso_date(s);
            if range {
                Date::Range(date, None)
            } else {
                Date::Single(date)
            }
        }
        Some(Value::Array(items)) if items.len() >= 2 => {
            let start = items[0].as_str().and_then(parse_iso_date);
            let end = items[1].as_str().and_then(parse_iso_date);
            Date::Range(start, end)
        }
        Some(Value::Object(map)) => {
            let start = map
                .get("start")
                .and_then(|v| v.as_str())
                .and_then(parse_iso_date);
            let end = map
                .get("end")
                .and_then(|v| v.as_str())
                .and_then(parse_iso_date);
            Date::Range(start, end)
        }
        _ => {
            if range {
                Date::Range(None, None)
            } else {
                Date::Single(None)
            }
        }
    }
}

pub fn date_to_value(date: Date) -> Value {
    match date {
        Date::Single(Some(d)) => json!(format_iso_date(d)),
        Date::Single(None) => Value::Null,
        Date::Range(start, end) => json!([start.map(format_iso_date), end.map(format_iso_date)]),
    }
}

/// One chart datum. Series charts use `value` / `values`; candlesticks use
/// OHLC; sankey nodes use `id`/`label`/`color` and links live on the node.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartPoint {
    pub id: String,
    pub label: String,
    pub value: Option<f64>,
    pub values: Vec<f64>,
    pub color: Option<String>,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: Option<f64>,
    pub source: Option<String>,
    pub target: Option<String>,
}

impl ChartPoint {
    pub fn from_item(item: &Item) -> Self {
        let values = item_number_list(item);
        let value = item
            .number_value()
            .map(|n| n as f64)
            .or_else(|| values.first().copied());
        Self {
            id: item.id_or_label(),
            label: item.label_or_id(),
            value,
            values,
            color: item.color.clone(),
            open: item.open.map(|n| n as f64),
            high: item.high.map(|n| n as f64),
            low: item.low.map(|n| n as f64),
            close: item.close.map(|n| n as f64),
            source: item.source.clone(),
            target: item.target.clone(),
        }
    }

    pub fn series_y(&self) -> Option<f64> {
        self.value.or_else(|| self.values.first().copied())
    }

    pub fn has_ohlc(&self) -> bool {
        self.open.is_some() && self.high.is_some() && self.low.is_some() && self.close.is_some()
    }
}

fn json_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn item_number_list(item: &Item) -> Vec<f64> {
    match &item.values {
        Some(Value::Array(xs)) => xs.iter().filter_map(json_f64).collect(),
        _ => match &item.value {
            Some(Value::Array(xs)) => xs.iter().filter_map(json_f64).collect(),
            _ => Vec::new(),
        },
    }
}

pub fn chart_points(node: &Node) -> Vec<ChartPoint> {
    node.collection()
        .iter()
        .map(ChartPoint::from_item)
        .filter(|p| !p.label.is_empty() && p.series_y().is_some())
        .collect()
}

pub fn chart_kind(node: &Node) -> String {
    node.variant
        .as_deref()
        .map(crate::catalog::normalize)
        .unwrap_or_else(|| "line".into())
}

pub fn bar_alignment(node: &Node) -> BarAlignment {
    match node
        .alignment
        .as_deref()
        .map(crate::catalog::normalize)
        .as_deref()
    {
        Some("top") => BarAlignment::Top,
        Some("left") => BarAlignment::Left,
        Some("right") => BarAlignment::Right,
        _ => BarAlignment::Bottom,
    }
}

pub fn sankey_align(node: &Node) -> Option<SankeyAlign> {
    match node
        .node_align
        .as_deref()
        .map(crate::catalog::normalize)
        .as_deref()
    {
        Some("left") => Some(SankeyAlign::Left),
        Some("right") => Some(SankeyAlign::Right),
        Some("center") => Some(SankeyAlign::Center),
        Some("justify") => Some(SankeyAlign::Justify),
        _ => None,
    }
}

pub fn sankey_value_scale(node: &Node) -> Option<SankeyValueScale> {
    match node
        .value_scale
        .as_deref()
        .map(crate::catalog::normalize)
        .as_deref()
    {
        Some("sqrt") => Some(SankeyValueScale::Sqrt),
        Some("linear") => Some(SankeyValueScale::Linear),
        _ => None,
    }
}

pub fn sankey_nodes(node: &Node) -> Vec<ChartPoint> {
    node.collection()
        .iter()
        .map(ChartPoint::from_item)
        .filter(|p| !p.id.is_empty() || !p.label.is_empty())
        .collect()
}

pub fn sankey_links(nodes: &[ChartPoint], links: &[Item]) -> Vec<SankeyLink> {
    let index_of =
        |key: &str| -> Option<usize> { nodes.iter().position(|n| n.id == key || n.label == key) };
    links
        .iter()
        .filter_map(|link| {
            let src = link.source.as_deref().or(link.id.as_deref())?;
            let tgt = link.target.as_deref()?;
            let value = link.number_value()? as f64;
            Some(SankeyLink::new(index_of(src)?, index_of(tgt)?, value))
        })
        .collect()
}

fn format_chart_number(value: f64) -> String {
    if (value - value.round()).abs() < 0.001 {
        format!("{:.0}", value)
    } else {
        format!("{:.1}", value)
    }
}

fn point_fill(point: &ChartPoint, fallback: Hsla) -> Hsla {
    point
        .color
        .as_deref()
        .and_then(parse_hex_color)
        .unwrap_or(fallback)
}

/// Default chart viewport when Clojure omits `:width` / `:height` / `:size`
/// / `:flex 1`. Outer layout is applied by the caller (`viewport_sized`).
/// Horizontal bar charts grow with category count so cljdu-style
/// directory rows are not clipped at 180px.
pub fn chart_viewport(node: &Node) -> (f32, f32) {
    let width = node.width.or(node.size).unwrap_or(320.0);
    let height = node.height.or(node.size).unwrap_or_else(|| {
        if chart_kind(node) == "bar" && bar_alignment(node).is_horizontal() {
            let n = node.collection().len().max(1) as f32;
            (n * 28.0 + 40.0).max(180.0)
        } else {
            180.0
        }
    });
    (width, height)
}

/// Theme tokens for pie slices, in paint order. Slice `i` uses
/// `PIE_SLICE_TOKENS[i % PIE_SLICE_TOKENS.len()]`. Labels are not hashed.
const PIE_SLICE_TOKENS: [&str; 7] = [
    "chart_1", "chart_2", "chart_3", "chart_4", "chart_5", "warning", "danger",
];

/// Color token for pie slice `index`. The slice label does not affect this.
#[cfg(test)]
fn pie_slice_token(index: usize) -> &'static str {
    PIE_SLICE_TOKENS[index % PIE_SLICE_TOKENS.len()]
}

fn pie_palette(cx: &App) -> [Hsla; 7] {
    // Keep this in lockstep with `PIE_SLICE_TOKENS`.
    [
        cx.theme().chart_1,
        cx.theme().chart_2,
        cx.theme().chart_3,
        cx.theme().chart_4,
        cx.theme().chart_5,
        cx.theme().warning,
        cx.theme().danger,
    ]
}

pub fn paint_chart(node: &Node, key: &str, cx: &App) -> gpui::AnyElement {
    let points = chart_points(node);
    let (width, height) = chart_viewport(node);
    let kind = chart_kind(node);
    let stroke = cx.theme().chart_1;
    let fill = cx.theme().chart_2;
    let plot_id = SharedString::from(format!("{key}/plot"));
    let chart: gpui::AnyElement = match kind.as_str() {
        "bar" => {
            let mut bar = BarChart::new(points)
                .id(plot_id)
                .band(|p| p.label.clone())
                .value(|p| p.series_y().unwrap_or(0.0))
                .alignment(bar_alignment(node))
                .fill(move |p, _, _, _| point_fill(p, stroke));
            bar = bar
                .label_axis(node.label_axis.unwrap_or(true))
                .value_axis(node.value_axis.unwrap_or(false))
                .grid(node.grid.unwrap_or(true));
            if let Some(margin) = node.tick_margin {
                bar = bar.tick_margin(margin as usize);
            }
            if let Some(ticks) = node.value_tick_count {
                bar = bar.value_tick_count(ticks as usize);
            }
            if node.labels.unwrap_or(false) {
                bar = bar.label(|p| format_chart_number(p.series_y().unwrap_or(0.0)));
            }
            bar.into_any_element()
        }
        "area" => AreaChart::new(points)
            .x(|p| p.label.clone())
            .y(|p| p.series_y().unwrap_or(0.0))
            .stroke(stroke)
            .fill(fill)
            .into_any_element(),
        "pie" => {
            let palette = pie_palette(cx);
            let radius = width.min(height) * 0.42;
            let pie_data: Vec<(usize, f64, Option<String>)> = points
                .iter()
                .enumerate()
                .filter_map(|(ix, p)| Some((ix, p.series_y()?, p.color.clone())))
                .collect();
            PieChart::new(pie_data)
                .value(|p| p.1 as f32)
                .outer_radius(radius)
                .color(move |p| {
                    p.2.as_deref()
                        .and_then(parse_hex_color)
                        .unwrap_or(palette[p.0 % PIE_SLICE_TOKENS.len()])
                })
                .into_any_element()
        }
        "radar" => {
            let mut points = points;
            for point in &mut points {
                if point.values.is_empty() {
                    if let Some(v) = point.value {
                        point.values = vec![v];
                    }
                }
            }
            let n = points.iter().map(|p| p.values.len()).max().unwrap_or(0);
            let series_meta: Vec<(String, Option<Hsla>)> = node
                .series
                .iter()
                .map(|s| {
                    (
                        s.label_or_id(),
                        s.color.as_deref().and_then(parse_hex_color),
                    )
                })
                .collect();
            let palette = pie_palette(cx);
            let mut radar = RadarChart::new(points)
                .id(plot_id)
                .label(|p| p.label.clone());
            if let Some(max) = node.max {
                radar = radar.max_value(max as f64);
            }
            if node.dot {
                radar = radar.dot();
            }
            radar = radar.grid(node.grid.unwrap_or(true));
            for i in 0..n {
                radar = radar.value(move |p| p.values.get(i).copied().unwrap_or(0.0));
                let series_stroke = series_meta
                    .get(i)
                    .and_then(|(_, color)| *color)
                    .unwrap_or(palette[i % palette.len()]);
                radar = radar.stroke(series_stroke).name(
                    series_meta
                        .get(i)
                        .map(|(name, _)| name.clone())
                        .unwrap_or_default(),
                );
            }
            radar.into_any_element()
        }
        "candlestick" => {
            let candles: Vec<ChartPoint> = node
                .collection()
                .iter()
                .map(ChartPoint::from_item)
                .filter(|p| !p.label.is_empty() && p.has_ohlc())
                .collect();
            let mut chart = CandlestickChart::new(candles)
                .x(|p| p.label.clone())
                .open(|p| p.open.unwrap_or(0.0))
                .high(|p| p.high.unwrap_or(0.0))
                .low(|p| p.low.unwrap_or(0.0))
                .close(|p| p.close.unwrap_or(0.0));
            if let Some(margin) = node.tick_margin {
                chart = chart.tick_margin(margin as usize);
            }
            chart = chart.grid(node.grid.unwrap_or(true));
            chart.into_any_element()
        }
        "sankey" => {
            let nodes = sankey_nodes(node);
            let links = sankey_links(&nodes, &node.links);
            let fallback = cx.theme().chart_1;
            let colored = nodes.iter().any(|n| n.color.is_some());
            let mut chart = SankeyChart::new(nodes, links)
                .node_label(|n| n.label.clone().into())
                .value_label(|_, v| format_chart_number(v).into());
            if let Some(align) = sankey_align(node) {
                chart = chart.node_align(align);
            }
            if let Some(scale) = sankey_value_scale(node) {
                chart = chart.value_scale(scale);
            }
            if colored {
                chart = chart.node_color(move |n| point_fill(n, fallback));
            }
            chart.into_any_element()
        }
        _ => LineChart::new(points)
            .x(|p| p.label.clone())
            .y(|p| p.series_y().unwrap_or(0.0))
            .stroke(stroke)
            .dot()
            .into_any_element(),
    };
    // Inner chart fills the clj-gpui viewport wrapper (layout/style live
    // on the outer `viewport_sized` / panel wrap, not here).
    v_flex()
        .id(SharedString::from(key.to_string()))
        .size_full()
        .child(chart)
        .into_any_element()
}

pub fn paint_markdown(node: &Node, key: &str) -> gpui::AnyElement {
    let body = node
        .text
        .clone()
        .or_else(|| node.message.clone())
        .unwrap_or_default();
    let html = node
        .format
        .as_deref()
        .map(crate::catalog::normalize)
        .as_deref()
        == Some("html")
        || node.kind == "html";
    let mut view = if html {
        TextView::html(SharedString::from(key.to_string()), body)
    } else {
        TextView::markdown(SharedString::from(key.to_string()), body)
    };
    view = view.selectable(true);
    if node.height.is_some() || node.flex.unwrap_or(0.0) >= 1.0 {
        view = view.scrollable(true);
    }
    view.into_any_element()
}

#[derive(Clone)]
pub struct VirtualRow {
    pub id: String,
    pub label: String,
    pub height: f32,
}

pub struct VirtualListView {
    pub items: Vec<VirtualRow>,
    pub axis: Axis,
    pub selected: Option<String>,
    pub on_change: Option<String>,
    pub cmd_tx: mpsc::Sender<Cmd>,
    pub scroll_handle: VirtualListScrollHandle,
}

impl VirtualListView {
    pub fn from_node(node: &Node, cmd_tx: mpsc::Sender<Cmd>) -> Self {
        let items = node
            .collection()
            .iter()
            .map(|item| VirtualRow {
                id: item.id_or_label(),
                label: item.label_or_id(),
                height: item.height.unwrap_or(36.0).max(18.0),
            })
            .collect();
        Self {
            items,
            axis: mapping::parse_virtual_list_axis(node.orientation.as_deref()),
            selected: node.string_value(),
            on_change: node.on_change.clone(),
            cmd_tx,
            scroll_handle: VirtualListScrollHandle::new(),
        }
    }

    pub fn sync_from_node(&mut self, node: &Node, cmd_tx: mpsc::Sender<Cmd>) {
        let scroll_handle = self.scroll_handle.clone();
        *self = Self::from_node(node, cmd_tx);
        self.scroll_handle = scroll_handle;
    }

    fn paint_rows(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let vertical = self.axis == Axis::Vertical;
        let accent = cx.theme().accent;
        range
            .filter_map(|ix| self.items.get(ix).cloned())
            .map(|row| {
                let selected = self.selected.as_deref() == Some(row.id.as_str());
                let cmd_tx = self.cmd_tx.clone();
                let on_change = self.on_change.clone();
                let id = row.id.clone();
                div()
                    .id(SharedString::from(row.id.clone()))
                    .when(vertical, |this| this.w_full().h(px(row.height)))
                    .when(!vertical, |this| this.h_full().w(px(row.height)))
                    .px_2()
                    .flex()
                    .items_center()
                    .when(selected, |this| {
                        this.font_weight(gpui::FontWeight::SEMIBOLD).bg(accent)
                    })
                    .child(row.label)
                    .on_click(move |_, _, _| {
                        if let Some(callback) = on_change.clone() {
                            protocol::send_callbacks(
                                &cmd_tx,
                                vec![protocol::CallbackCall::with_value(
                                    callback,
                                    json!(id.clone()),
                                )],
                            );
                        }
                    })
                    .into_any_element()
            })
            .collect()
    }
}

impl Render for VirtualListView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sizes = Rc::new(
            self.items
                .iter()
                .map(|row| {
                    if self.axis == Axis::Vertical {
                        size(px(0.), px(row.height))
                    } else {
                        size(px(row.height), px(0.))
                    }
                })
                .collect(),
        );
        if self.axis == Axis::Vertical {
            v_virtual_list(cx.entity(), "vlist", sizes, Self::paint_rows)
                .track_scroll(&self.scroll_handle)
                .into_any_element()
        } else {
            h_virtual_list(cx.entity(), "hlist", sizes, Self::paint_rows)
                .track_scroll(&self.scroll_handle)
                .into_any_element()
        }
    }
}

/// Host-owned dock panel. `panel_name` is a stable serialize id; Clojure
/// titles distinguish tabs. Content is painted by `RootView` via a weak ref
/// after `RootView::render` returns, so slot retain has already run — dock
/// bodies use the static overlay painter plus markdown.
pub struct CljPanel {
    pub title: SharedString,
    pub live: Rc<RefCell<Node>>,
    pub path: String,
    pub emit: crate::overlay::ActionEmitter,
    focus: FocusHandle,
}

impl CljPanel {
    pub fn new(
        title: impl Into<SharedString>,
        live: Rc<RefCell<Node>>,
        path: String,
        emit: crate::overlay::ActionEmitter,
        focus: FocusHandle,
    ) -> Self {
        Self {
            title: title.into(),
            live,
            path,
            emit,
            focus,
        }
    }
}

impl EventEmitter<PanelEvent> for CljPanel {}

impl Focusable for CljPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for CljPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let node = self.live.borrow().clone();
        paint_panel_body(&node, &self.path, self.emit.clone(), window, cx)
    }
}

pub fn paint_panel_body(
    node: &Node,
    path: &str,
    emit: crate::overlay::ActionEmitter,
    _window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
    match node.kind.as_str() {
        "markdown" | "html" => paint_markdown(node, path),
        "chart" => {
            let (width, height) = chart_viewport(node);
            v_flex()
                .w(px(width))
                .h(px(height))
                .min_h_0()
                .child(paint_chart(node, path, cx))
                .into_any_element()
        }
        _ if !node.children.is_empty() => crate::overlay::paint_static(&node.children, emit, path),
        _ => crate::overlay::paint_static(std::slice::from_ref(node), emit, path),
    }
}

impl gpui::base::dock::Panel for CljPanel {
    fn panel_name(&self) -> &'static str {
        "clj-gpui-panel"
    }

    fn closable(&self, _: &App) -> bool {
        false
    }

    fn zoomable(&self, _: &App) -> bool {
        false
    }
}

impl StyledPanel for CljPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.title.clone()
    }

    fn zoom_control(&self, _: &App) -> Option<PanelControl> {
        None
    }
}

pub fn dock_side(item: &Item) -> &'static str {
    match item
        .side
        .as_deref()
        .map(crate::catalog::normalize)
        .as_deref()
    {
        Some("left") => "left",
        Some("right") => "right",
        Some("bottom") => "bottom",
        _ => "center",
    }
}

pub fn is_settings_field(item: &Item) -> bool {
    matches!(
        item.variant
            .as_deref()
            .map(crate::catalog::normalize)
            .as_deref(),
        Some("switch" | "checkbox" | "number" | "dropdown" | "select" | "input")
    )
}

/// Group wrapper: nested `:items` and no field `:variant`.
///
/// A `:variant :dropdown` / `:select` field also has option `:items`; that
/// stays a field. Do not treat `(seq :items)` alone as a group.
pub fn is_settings_group(item: &Item) -> bool {
    !item.items.is_empty() && !is_settings_field(item)
}

pub fn settings_groups(page: &Item) -> Vec<Item> {
    if page.items.iter().any(is_settings_group) {
        page.items
            .iter()
            .map(|row| {
                if is_settings_group(row) {
                    row.clone()
                } else {
                    Item {
                        items: vec![row.clone()],
                        ..Item::default()
                    }
                }
            })
            .collect()
    } else {
        vec![Item {
            label: Some(page.label_or_id()),
            items: page.items.clone(),
            ..Item::default()
        }]
    }
}

pub fn settings_pages(node: &Node, cmd_tx: &mpsc::Sender<Cmd>) -> Vec<SettingPage> {
    node.collection()
        .iter()
        .map(|page| {
            let title = page.label_or_id();
            let mut setting_page = SettingPage::new(title).resettable(false);
            for group in settings_groups(page) {
                let mut setting_group = SettingGroup::new();
                if let Some(label) = group.label.clone() {
                    setting_group = setting_group.title(label);
                }
                for field in group.items {
                    setting_group = setting_group.item(settings_field(&field, cmd_tx, node));
                }
                setting_page = setting_page.group(setting_group);
            }
            setting_page
        })
        .collect()
}

fn settings_field(field: &Item, cmd_tx: &mpsc::Sender<Cmd>, node: &Node) -> SettingItem {
    let title = field.label_or_id();
    let id = field.id_or_label();
    let kind = field
        .variant
        .as_deref()
        .map(crate::catalog::normalize)
        .unwrap_or_else(|| infer_settings_kind(field));
    let on_change = node.on_change.clone();
    let tx = cmd_tx.clone();
    let emit = move |value: Value| {
        if let Some(callback) = on_change.clone() {
            protocol::send_callbacks(
                &tx,
                vec![protocol::CallbackCall::with_value(
                    callback,
                    json!({"id": id.clone(), "value": value}),
                )],
            );
        }
    };
    match kind.as_str() {
        "switch" | "checkbox" => {
            let checked = field.checked.unwrap_or(false);
            if kind == "checkbox" {
                SettingItem::new(
                    title,
                    SettingField::checkbox(move |_| checked, move |v, _| emit(json!(v))),
                )
            } else {
                SettingItem::new(
                    title,
                    SettingField::switch(move |_| checked, move |v, _| emit(json!(v))),
                )
            }
        }
        "number" => {
            let n = field.number_value().unwrap_or(0.0) as f64;
            let options = NumberFieldOptions {
                min: field.min.unwrap_or(f32::MIN) as f64,
                max: field.max.unwrap_or(f32::MAX) as f64,
                step: field.step.unwrap_or(1.0).max(0.000_001) as f64,
            };
            SettingItem::new(
                title,
                SettingField::number_input(options, move |_| n, move |v, _| emit(json!(v))),
            )
        }
        "dropdown" | "select" => {
            let selected: SharedString = field.string_value().unwrap_or_default().into();
            let options: Vec<(SharedString, SharedString)> = field
                .items
                .iter()
                .map(|opt| {
                    (
                        SharedString::from(opt.id_or_label()),
                        SharedString::from(opt.label_or_id()),
                    )
                })
                .collect();
            SettingItem::new(
                title,
                SettingField::dropdown(
                    options,
                    move |_| selected.clone(),
                    move |v, _| emit(json!(v.to_string())),
                ),
            )
        }
        _ => {
            let text: SharedString = field
                .text
                .clone()
                .or_else(|| field.string_value())
                .unwrap_or_default()
                .into();
            SettingItem::new(
                title,
                SettingField::input(
                    move |_| text.clone(),
                    move |v, _| emit(json!(v.to_string())),
                ),
            )
        }
    }
}

fn infer_settings_kind(field: &Item) -> String {
    if field.checked.is_some() {
        "switch".into()
    } else if !field.items.is_empty() {
        "dropdown".into()
    } else if field.number_value().is_some() && field.text.is_none() {
        "number".into()
    } else {
        "input".into()
    }
}

pub fn build_settings(node: &Node, key: &str, cmd_tx: &mpsc::Sender<Cmd>) -> Settings {
    Settings::new(SharedString::from(key.to_string())).pages(settings_pages(node, cmd_tx))
}

pub fn parse_sheet_placement(node: &Node) -> Placement {
    mapping::parse_placement(
        node.placement
            .as_deref()
            .or(node.side.as_deref())
            .or(node.orientation.as_deref()),
        Placement::Right,
    )
}

pub fn parse_sidebar_side(node: &Node) -> Side {
    mapping::parse_side(
        node.side.as_deref().or(node.placement.as_deref()),
        Side::Left,
    )
}

pub fn otp_length(node: &Node) -> usize {
    let n = node.count.unwrap_or(0) as usize;
    if n == 0 { 6 } else { n.clamp(1, 12) }
}

pub fn editor_language(node: &Node) -> String {
    node.language
        .clone()
        .or_else(|| node.variant.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "text".into())
}

pub fn number_from_input(text: &str) -> Option<f64> {
    text.trim().parse().ok()
}

pub fn apply_number_step(current: f64, increment: bool, node: &Node) -> f64 {
    let step = node.step.unwrap_or(1.0).max(0.0) as f64;
    let min = node.min.map(|n| n as f64).unwrap_or(f64::MIN);
    let max = node.max.map(|n| n as f64).unwrap_or(f64::MAX);
    let next = if increment {
        current + step
    } else {
        current - step
    };
    next.clamp(min.min(max), min.max(max))
}

#[allow(dead_code)]
pub fn sync_input_text(
    state: &Entity<InputState>,
    wanted: &str,
    window: &mut Window,
    cx: &mut App,
) {
    let current = state.read(cx).value().to_string();
    if current != wanted {
        state.update(cx, |input, cx| {
            input.set_value(wanted.to_string(), window, cx);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_component::plot::shape::{BarAlignment, SankeyAlign, SankeyValueScale};
    use serde_json::json;

    #[test]
    fn iso_dates_round_trip() {
        let d = parse_iso_date("2026-09-02").unwrap();
        assert_eq!(format_iso_date(d), "2026-09-02");
        assert!(parse_iso_date("2026/09/02").is_some());
        assert!(parse_iso_date("nope").is_none());
    }

    #[test]
    fn textarea_submit_on_enter_follows_on_submit() {
        assert!(!textarea_submit_on_enter(None));
        assert!(textarea_submit_on_enter(Some("cb-submit")));
    }

    #[test]
    fn table_align_and_footer_split() {
        assert_eq!(
            table_align(&Item {
                align: Some("end".into()),
                ..Item::default()
            }),
            TableAlign::End
        );
        assert_eq!(
            table_align(&Item {
                align: Some("right".into()),
                ..Item::default()
            }),
            TableAlign::End
        );
        assert_eq!(
            table_align(&Item {
                align: Some("center".into()),
                ..Item::default()
            }),
            TableAlign::Center
        );
        assert_eq!(
            table_align_node(&Node {
                align: Some("end".into()),
                ..Node::default()
            }),
            TableAlign::End
        );
        assert_eq!(
            table_accessibility_label(&Node {
                accessibility_label: Some("Recent invoices".into()),
                ..Node::default()
            }),
            Some("Recent invoices")
        );
        assert_eq!(
            table_accessibility_label(&Node {
                accessibility_label: Some(String::new()),
                ..Node::default()
            }),
            None
        );
        assert_eq!(table_accessibility_label(&Node::default()), None);
        let items = vec![
            Item {
                id: Some("a".into()),
                cells: vec!["A".into()],
                ..Item::default()
            },
            Item {
                id: Some("footer".into()),
                variant: Some("footer".into()),
                cells: vec!["Total".into()],
                ..Item::default()
            },
        ];
        let (body, foot) = split_table_footer(&items);
        assert_eq!(body.len(), 1);
        assert_eq!(foot.unwrap().cells, vec!["Total".to_string()]);
    }

    #[test]
    fn rating_and_stepper_selection() {
        let rating: Node = serde_json::from_value(json!({
            "type": "rating",
            "value": 7,
            "max": 5
        }))
        .unwrap();
        assert_eq!(rating_max(&rating), 5);
        assert_eq!(rating_value(&rating), 5);
        // Kit default max is 5; `.value(8)` before `.max(10)` would clamp to 5.
        let high: Node = serde_json::from_value(json!({
            "type": "rating",
            "value": 8,
            "max": 10
        }))
        .unwrap();
        assert_eq!(rating_max_then_value(&high), (10, 8));
        let steps = vec![
            Item {
                id: Some("cart".into()),
                label: Some("Cart".into()),
                ..Item::default()
            },
            Item {
                id: Some("pay".into()),
                label: Some("Pay".into()),
                ..Item::default()
            },
        ];
        assert_eq!(stepper_selected_index(&steps, Some("pay")), 1);
        assert_eq!(stepper_selected_index(&steps, Some("0")), 0);
        assert_eq!(stepper_selected_index(&steps, None), 0);
    }

    #[test]
    fn combobox_payload_single_and_multiple() {
        let values = [SharedString::from("clj"), SharedString::from("rs")];
        assert_eq!(combobox_payload(false, &values), json!("clj"));
        assert_eq!(combobox_payload(false, &[]), Value::Null);
        assert_eq!(combobox_payload(true, &values), json!(["clj", "rs"]));
    }

    #[test]
    fn combobox_unrelated_rerender_skips_set_selected_values() {
        let sel = [SharedString::from("clj")];
        let items = vec![Item {
            id: Some("clj".into()),
            label: Some("Clojure".into()),
            ..Item::default()
        }];
        let fp = combobox_fingerprint(&items);
        let sync = combobox_slot_sync(fp, fp, &sel, &sel);
        assert!(!sync.set_items);
        assert!(
            !sync.set_selected,
            "set_selected_values clears Kit's search query"
        );
        let renamed = vec![Item {
            id: Some("clj".into()),
            label: Some("Clojure lang".into()),
            ..Item::default()
        }];
        let items_changed = combobox_slot_sync(fp, combobox_fingerprint(&renamed), &sel, &sel);
        assert!(items_changed.set_items);
        assert!(
            items_changed.set_selected,
            "set_items does not rebuild Kit's cloned selection; renamed/removed options need set_selected_values"
        );
        let dropped = vec![Item {
            id: Some("rs".into()),
            label: Some("Rust".into()),
            ..Item::default()
        }];
        let removed = combobox_slot_sync(fp, combobox_fingerprint(&dropped), &sel, &sel);
        assert!(removed.set_items);
        assert!(removed.set_selected);
        let next = [SharedString::from("rs")];
        let sel_only = combobox_slot_sync(fp, fp, &sel, &next);
        assert!(!sel_only.set_items);
        assert!(sel_only.set_selected);
        let echo = combobox_slot_sync(fp, fp, &next, &next);
        assert!(
            !echo.set_selected,
            "native Change cache matching a Clojure echo must not clear the query"
        );
    }

    #[test]
    fn date_value_range_and_single() {
        let single = date_from_value(&Some(json!("2026-09-02")), false);
        assert!(matches!(single, Date::Single(Some(_))));
        let range = date_from_value(&Some(json!(["2026-01-01", "2026-01-31"])), true);
        assert!(matches!(range, Date::Range(Some(_), Some(_))));
        let json = date_to_value(range);
        assert_eq!(json, json!(["2026-01-01", "2026-01-31"]));
    }

    #[test]
    fn chart_points_from_items() {
        let node: Node = serde_json::from_value(json!({
            "type": "chart",
            "items": [
                {"id": "a", "label": "A", "value": 1},
                {"id": "b", "label": "B", "value": 2.5}
            ]
        }))
        .unwrap();
        assert_eq!(
            chart_points(&node)
                .iter()
                .map(|p| (p.label.clone(), p.series_y()))
                .collect::<Vec<_>>(),
            vec![("A".into(), Some(1.0)), ("B".into(), Some(2.5))]
        );
    }

    #[test]
    fn bar_alignment_left_is_horizontal() {
        let node: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "bar",
            "alignment": "left"
        }))
        .unwrap();
        assert_eq!(bar_alignment(&node), BarAlignment::Left);
        assert!(bar_alignment(&node).is_horizontal());
        let omitted: Node =
            serde_json::from_value(json!({"type": "chart", "variant": "bar"})).unwrap();
        assert_eq!(bar_alignment(&omitted), BarAlignment::Bottom);
        assert!(!bar_alignment(&omitted).is_horizontal());
        let top: Node = serde_json::from_value(json!({
            "type": "chart",
            "alignment": "top"
        }))
        .unwrap();
        assert_eq!(bar_alignment(&top), BarAlignment::Top);
    }

    #[test]
    fn chart_kind_covers_kit_names() {
        let kind = |variant: &str| {
            let node: Node =
                serde_json::from_value(json!({"type": "chart", "variant": variant})).unwrap();
            chart_kind(&node)
        };
        assert_eq!(kind("bar"), "bar");
        assert_eq!(kind("radar"), "radar");
        assert_eq!(kind("candlestick"), "candlestick");
        assert_eq!(kind("sankey"), "sankey");
        assert_eq!(kind("line"), "line");
        let omitted: Node = serde_json::from_value(json!({"type": "chart"})).unwrap();
        assert_eq!(chart_kind(&omitted), "line");
    }

    #[test]
    fn radar_values_from_array_or_value() {
        let node: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "radar",
            "items": [
                {"label": "Speed", "values": [80, 60]},
                {"label": "Range", "value": [10, 20]},
                {"label": "Solo", "value": 5}
            ]
        }))
        .unwrap();
        let points = chart_points(&node);
        assert_eq!(points[0].values, vec![80.0, 60.0]);
        assert_eq!(points[1].values, vec![10.0, 20.0]);
        assert_eq!(points[2].series_y(), Some(5.0));
        assert!(points[2].values.is_empty());
    }

    #[test]
    fn candlestick_point_needs_ohlc() {
        let item: Item = serde_json::from_value(json!({
            "label": "Mon", "open": 100, "high": 110, "low": 95, "close": 105
        }))
        .unwrap();
        let p = ChartPoint::from_item(&item);
        assert!(p.has_ohlc());
        assert_eq!(p.open, Some(100.0));
        let skip: Item = serde_json::from_value(json!({"label": "Mon", "value": 10})).unwrap();
        assert!(!ChartPoint::from_item(&skip).has_ohlc());
    }

    #[test]
    fn sankey_links_resolve_ids() {
        let node: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "sankey",
            "items": [
                {"id": "rev", "label": "Revenue"},
                {"id": "cost", "label": "Cost"}
            ],
            "links": [
                {"source": "rev", "target": "cost", "value": 55},
                {"source": "missing", "target": "cost", "value": 1}
            ]
        }))
        .unwrap();
        let nodes = sankey_nodes(&node);
        let links = sankey_links(&nodes, &node.links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].source, 0);
        assert_eq!(links[0].target, 1);
        assert_eq!(links[0].value, 55.0);
        assert_eq!(sankey_align(&node), None);
        let aligned: Node = serde_json::from_value(json!({
            "type": "chart",
            "node-align": "left",
            "value-scale": "sqrt"
        }))
        .unwrap();
        assert_eq!(sankey_align(&aligned), Some(SankeyAlign::Left));
        assert_eq!(sankey_value_scale(&aligned), Some(SankeyValueScale::Sqrt));
    }

    #[test]
    fn pie_first_seven_indices_are_distinct_tokens() {
        let tokens: Vec<_> = (0..7).map(pie_slice_token).collect();
        assert_eq!(
            tokens,
            vec![
                "chart_1", "chart_2", "chart_3", "chart_4", "chart_5", "warning", "danger"
            ]
        );
        for i in 0..7 {
            for j in (i + 1)..7 {
                assert_ne!(
                    pie_slice_token(i),
                    pie_slice_token(j),
                    "indices {i} and {j}"
                );
            }
        }
    }

    #[test]
    fn pie_color_depends_on_index_not_label() {
        let color_for = |index: usize, _label: &str| pie_slice_token(index);
        assert_eq!(color_for(0, "flutter"), color_for(0, "Other"));
        assert_eq!(color_for(0, "flutter"), "chart_1");
        assert_ne!(color_for(0, "flutter"), color_for(1, "flutter"));
        assert_ne!(color_for(0, "flutter"), color_for(1, "Other"));
        assert_eq!(color_for(1, "Other"), "chart_2");
    }

    #[test]
    fn pie_index_colors_avoid_former_label_hash_collisions() {
        fn label_hash_bucket(label: &str) -> usize {
            label
                .bytes()
                .fold(0usize, |acc, b| acc.wrapping_add(b as usize))
                % 5
        }
        assert_eq!(
            label_hash_bucket("flutter"),
            label_hash_bucket("Other"),
            "these labels collided on the old 5-color hash"
        );
        assert_ne!(pie_slice_token(0), pie_slice_token(1));
        assert_eq!(pie_slice_token(0), "chart_1");
        assert_eq!(pie_slice_token(1), "chart_2");
    }

    #[test]
    fn hex_colors_parse() {
        assert!(parse_hex_color("#3366ff").is_some());
        assert!(parse_hex_color("not-a-color").is_none());
    }

    #[test]
    fn number_step_clamps() {
        let node: Node = serde_json::from_value(json!({
            "type": "number-input",
            "min": 0,
            "max": 10,
            "step": 3
        }))
        .unwrap();
        assert_eq!(apply_number_step(9.0, true, &node), 10.0);
        assert_eq!(apply_number_step(1.0, false, &node), 0.0);
    }

    #[test]
    fn otp_length_defaults_and_clamps() {
        let omitted: Node = serde_json::from_value(json!({"type": "otp-input"})).unwrap();
        let long: Node = serde_json::from_value(json!({"type": "otp-input", "count": 99})).unwrap();
        assert_eq!(otp_length(&omitted), 6);
        assert_eq!(otp_length(&long), 12);
    }

    #[test]
    fn dock_side_from_item() {
        let left: Item = serde_json::from_value(json!({"id": "files", "side": "left"})).unwrap();
        let center: Item = serde_json::from_value(json!({"id": "main"})).unwrap();
        let bottom: Item = serde_json::from_value(json!({"id": "log", "side": "bottom"})).unwrap();
        assert_eq!(dock_side(&left), "left");
        assert_eq!(dock_side(&center), "center");
        assert_eq!(dock_side(&bottom), "bottom");
    }

    #[test]
    fn virtual_list_omitted_orientation_is_vertical() {
        let node: Node = serde_json::from_value(json!({
            "type": "virtual-list",
            "items": [{"id": "a", "label": "A"}]
        }))
        .unwrap();
        let (tx, _rx) = mpsc::channel();
        let view = VirtualListView::from_node(&node, tx);
        assert_eq!(view.axis, Axis::Vertical);

        let horiz: Node = serde_json::from_value(json!({
            "type": "virtual-list",
            "orientation": "horizontal",
            "items": [{"id": "a", "label": "A"}]
        }))
        .unwrap();
        let (tx, _rx) = mpsc::channel();
        let view = VirtualListView::from_node(&horiz, tx);
        assert_eq!(view.axis, Axis::Horizontal);
    }

    #[test]
    fn virtual_list_sync_preserves_scroll_handle() {
        let first: Node = serde_json::from_value(json!({
            "type": "virtual-list",
            "items": [{"id": "a", "label": "A"}]
        }))
        .unwrap();
        let updated: Node = serde_json::from_value(json!({
            "type": "virtual-list",
            "items": [{"id": "a", "label": "Updated"}, {"id": "b", "label": "B"}],
            "value": "b"
        }))
        .unwrap();
        let (tx, _rx) = mpsc::channel();
        let mut view = VirtualListView::from_node(&first, tx.clone());
        let original_handle = view.scroll_handle.clone();
        original_handle.set_offset(gpui::point(px(-7.0), px(-31.0)));

        view.sync_from_node(&updated, tx);

        assert_eq!(
            view.scroll_handle.offset(),
            gpui::point(px(-7.0), px(-31.0))
        );
        view.scroll_handle
            .set_offset(gpui::point(px(-9.0), px(-42.0)));
        assert_eq!(original_handle.offset(), gpui::point(px(-9.0), px(-42.0)));
        assert_eq!(view.selected.as_deref(), Some("b"));
        assert_eq!(view.items.len(), 2);
    }

    #[test]
    fn color_sync_clear_set_and_replace() {
        let blue = parse_hex_color("#3366ff").unwrap();
        let pink = parse_hex_color("#ff00aa").unwrap();
        assert_eq!(color_sync(Some(blue), Some(blue)), ColorSync::Keep);
        assert_eq!(color_sync(Some(pink), Some(blue)), ColorSync::Set);
        assert_eq!(color_sync(Some(blue), None), ColorSync::Set);
        assert_eq!(color_sync(None, Some(blue)), ColorSync::RecreateClear);
        assert_eq!(color_sync(None, None), ColorSync::Keep);
        assert_eq!(color_event_payload(None), json!(null));
        assert_eq!(
            color_event_payload(Some(blue)),
            json!(format_hex_color(blue))
        );
        let empty: Node =
            serde_json::from_value(json!({"type": "color-picker", "value": null})).unwrap();
        assert_eq!(color_from_node(&empty), None);
        let set: Node =
            serde_json::from_value(json!({"type": "color-picker", "value": "#ff00aa"})).unwrap();
        assert_eq!(
            color_from_node(&set).map(format_hex_color),
            Some(format_hex_color(pink))
        );
    }

    #[test]
    fn number_then_text_payload_is_string() {
        assert_eq!(input_change_payload(true, "4"), Some(json!(4.0)));
        assert_eq!(input_change_payload(true, "abc"), None);
        assert_eq!(input_change_payload(false, "abc"), Some(json!("abc")));
        assert_eq!(input_change_payload(false, "4"), Some(json!("4")));
    }

    #[test]
    fn chart_viewport_uses_node_then_defaults() {
        let def: Node = serde_json::from_value(json!({"type": "chart"})).unwrap();
        assert_eq!(chart_viewport(&def), (320.0, 180.0));
        let sized: Node = serde_json::from_value(json!({
            "type": "chart",
            "width": 400.0,
            "height": 90.0
        }))
        .unwrap();
        assert_eq!(chart_viewport(&sized), (400.0, 90.0));
        let square: Node = serde_json::from_value(json!({"type": "chart", "size": 120.0})).unwrap();
        assert_eq!(chart_viewport(&square), (120.0, 120.0));
        let flex: Node = serde_json::from_value(json!({"type": "chart", "flex": 1.0})).unwrap();
        assert_eq!(
            chart_viewport(&flex),
            (320.0, 180.0),
            "flex is owned by the outer wrapper; inner pie radius still needs a fallback span"
        );
        let hbar: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "bar",
            "alignment": "left",
            "items": [
                {"label": "a", "value": 1},
                {"label": "b", "value": 2},
                {"label": "c", "value": 3},
                {"label": "d", "value": 4},
                {"label": "e", "value": 5},
                {"label": "f", "value": 6},
                {"label": "g", "value": 7},
                {"label": "h", "value": 8}
            ]
        }))
        .unwrap();
        assert_eq!(chart_viewport(&hbar), (320.0, 8.0 * 28.0 + 40.0));
    }

    fn settings_page(value: serde_json::Value) -> Item {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn settings_dropdown_items_are_fields_not_groups() {
        let page = settings_page(json!({
            "id": "general",
            "label": "General",
            "items": [{
                "id": "theme",
                "label": "Theme",
                "variant": "dropdown",
                "value": "dark",
                "items": [
                    {"id": "dark", "label": "Dark"},
                    {"id": "light", "label": "Light"}
                ]
            }]
        }));
        assert!(!is_settings_group(&page.items[0]));
        assert!(is_settings_field(&page.items[0]));
        let groups = settings_groups(&page);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items.len(), 1);
        assert_eq!(groups[0].items[0].id_or_label(), "theme");
        assert_eq!(groups[0].items[0].items[0].id_or_label(), "dark");
        assert_eq!(groups[0].items[0].items[1].id_or_label(), "light");
    }

    #[test]
    fn settings_grouped_dropdown_stays_inside_the_group() {
        let page = settings_page(json!({
            "id": "general",
            "label": "General",
            "items": [
                {
                    "label": "Appearance",
                    "items": [{
                        "id": "theme",
                        "label": "Theme",
                        "variant": "dropdown",
                        "value": "dark",
                        "items": [
                            {"id": "dark", "label": "Dark"},
                            {"id": "light", "label": "Light"}
                        ]
                    }]
                },
                {
                    "label": "Advanced",
                    "items": [{
                        "id": "debug",
                        "label": "Debug",
                        "variant": "switch",
                        "checked": false
                    }]
                }
            ]
        }));
        assert!(is_settings_group(&page.items[0]));
        assert!(is_settings_group(&page.items[1]));
        assert!(is_settings_field(&page.items[0].items[0]));
        let groups = settings_groups(&page);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].label.as_deref(), Some("Appearance"));
        assert_eq!(groups[0].items[0].id_or_label(), "theme");
        assert_eq!(groups[1].label.as_deref(), Some("Advanced"));
        assert_eq!(groups[1].items[0].id_or_label(), "debug");
    }

    #[test]
    fn unused_resizable_keys_are_dropped() {
        use std::collections::{HashMap, HashSet};
        let mut slots = HashMap::from([("split-a".to_string(), 1u8), ("split-b".to_string(), 2u8)]);
        let used = HashSet::from(["split-a".to_string()]);
        slots.retain(|key, _| used.contains(key));
        assert_eq!(slots.len(), 1);
        assert!(slots.contains_key("split-a"));
        assert!(!slots.contains_key("split-b"));
    }
}
