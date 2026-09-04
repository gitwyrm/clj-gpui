//! Product widgets that sit on the v6 protocol: dates, colors, charts,
//! markdown, virtual lists, settings fields, and dock panels.
//!
//! Overlay sheet/notification collection lives in `overlay`. Slot maps and
//! `RootView::render_node` arms stay in `renderer`.

use crate::mapping;
use crate::protocol::{self, ChartLabelLine, Cmd, Item, Node};
use chrono::NaiveDate;
use gpui::{
    App, Axis, Context, Corners, Entity, EventEmitter, FocusHandle, Focusable, Hsla, IntoElement,
    ParentElement, Render, SharedString, Styled, Window, div, linear_color_stop, prelude::*, px,
    size,
};
use gpui_component::{
    ActiveTheme as _, Colorize as _, Placement, Side, VirtualListScrollHandle,
    calendar::Date,
    chart::{
        AreaChart, BarChart, CandlestickChart, LineChart, PieChart, RadarChart, RadarLabel,
        SankeyChart, SankeyLabel,
    },
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

fn finite_usize(value: Option<f32>) -> Option<usize> {
    value
        .filter(|n| n.is_finite())
        .map(|n| n.round().max(0.0) as usize)
}

/// Pagination current page. Kit default is 1; Kit clamps to ≥1.
pub fn pagination_current_page(node: &Node) -> usize {
    finite_usize(node.number_value()).unwrap_or(1)
}

/// Pagination total pages. Kit default is 1; Kit clamps to ≥1.
pub fn pagination_total_pages(node: &Node) -> usize {
    finite_usize(node.total).unwrap_or(1)
}

/// Pagination visible page buttons. `None` leaves Kit's default (5).
pub fn pagination_visible_pages(node: &Node) -> Option<usize> {
    finite_usize(node.visible_pages)
}

/// ProgressCircle percentage. Kit `.value()` clamps to 0..=100.
pub fn progress_circle_value(node: &Node) -> f32 {
    node.number_value().filter(|n| n.is_finite()).unwrap_or(0.0)
}

/// ShimmerText duration in seconds. `None` leaves Kit's 2s default.
/// Negative / non-finite values are omitted (`Duration` cannot represent them).
pub fn shimmer_duration_secs(node: &Node) -> Option<f32> {
    node.duration.filter(|n| n.is_finite() && *n >= 0.0)
}

/// HoverCard open/close delay in seconds. `None` leaves Kit's 0.6s / 0.3s.
pub fn hover_card_delay_secs(value: Option<f32>) -> Option<f32> {
    value.filter(|n| n.is_finite() && *n >= 0.0)
}

/// Avatar image source. Empty / omitted is initials or the placeholder icon.
#[cfg(test)]
pub fn avatar_src(node: &Node) -> Option<&str> {
    node.src.as_deref().filter(|s| !s.is_empty())
}

/// AvatarGroup visible count. `None` leaves Kit's 3.
#[cfg(test)]
pub fn avatar_group_limit(node: &Node) -> Option<usize> {
    node.limit
        .filter(|n| n.is_finite())
        .map(|n| n.round().max(0.0) as usize)
}

/// ShimmerText highlight half-width. `spread-px` wins when both are set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShimmerSpreadSpec {
    Relative(f32),
    Absolute(f32),
}

pub fn shimmer_spread(node: &Node) -> Option<ShimmerSpreadSpec> {
    if let Some(px) = node.spread_px.filter(|n| n.is_finite()) {
        Some(ShimmerSpreadSpec::Absolute(px))
    } else {
        node.spread
            .filter(|n| n.is_finite())
            .map(ShimmerSpreadSpec::Relative)
    }
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

/// One selectable row in a Select dropdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectLeaf {
    pub id: String,
    pub label: String,
    pub disabled: bool,
    pub display: Option<String>,
}

/// Kit `SelectGroup`: a named section of [`SelectLeaf`] rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectSection {
    pub title: String,
    pub items: Vec<SelectLeaf>,
}

pub fn select_leaf_from_item(item: &Item) -> SelectLeaf {
    SelectLeaf {
        id: item.id_or_label(),
        label: item.label_or_id(),
        disabled: item.disabled,
        display: item
            .display
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    }
}

/// Nested `:items` on any option means Kit `SearchableVec<SelectGroup<_>>`.
pub fn select_is_grouped(items: &[Item]) -> bool {
    items.iter().any(|item| !item.items.is_empty())
}

/// Build Kit sections. Consecutive ungrouped options share one untitled
/// section so mixed lists still get an `IndexPath` section+row.
pub fn select_sections(items: &[Item]) -> Vec<SelectSection> {
    let mut sections = Vec::new();
    let mut untitled: Vec<SelectLeaf> = Vec::new();
    fn flush(untitled: &mut Vec<SelectLeaf>, sections: &mut Vec<SelectSection>) {
        if !untitled.is_empty() {
            sections.push(SelectSection {
                title: String::new(),
                items: std::mem::take(untitled),
            });
        }
    }
    for item in items {
        if item.items.is_empty() {
            untitled.push(select_leaf_from_item(item));
        } else {
            flush(&mut untitled, &mut sections);
            sections.push(SelectSection {
                title: item.label_or_id(),
                items: item.items.iter().map(select_leaf_from_item).collect(),
            });
        }
    }
    flush(&mut untitled, &mut sections);
    sections
}

fn hash_select_item(hasher: &mut impl std::hash::Hasher, item: &Item) {
    use std::hash::Hash;
    item.id.hash(hasher);
    item.label.hash(hasher);
    item.text.hash(hasher);
    item.display.hash(hasher);
    item.disabled.hash(hasher);
    item.items.len().hash(hasher);
    for child in &item.items {
        hash_select_item(hasher, child);
    }
}

/// Identity of select options, including `SelectGroup` children.
pub fn select_fingerprint(items: &[Item]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    items.len().hash(&mut hasher);
    for item in items {
        hash_select_item(&mut hasher, item);
    }
    hasher.finish()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectSlotSync {
    /// Option identity changed. Recreate `SelectState` so the search
    /// input and `matched_items` agree — do not call Kit `set_items`
    /// on a live searchable state (that installs an unfiltered
    /// delegate while leaving the query text).
    pub set_items: bool,
    pub set_selected: bool,
}

/// How a live Select slot should apply Clojure's next tree.
///
/// Unrelated rerenders ([`SelectLiveSync::Leave`]) must not touch Kit
/// state, including after a native Confirm whose echo matches the
/// cached id (`set_selected_value` would clear an in-progress query).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectLiveSync {
    Leave,
    /// Controlled id changed against the same collection. Use Kit
    /// `SelectState::set_selected_value` (or `set_selected_index(None)`
    /// when clearing). Never feed a full-list [`select_index`] into a
    /// filtered `SearchableVec` — matched row/section indexes can
    /// differ from the unfiltered collection.
    SetValue,
    /// Option fingerprint changed. Recreate the `SelectState` entity.
    Rebuild,
}

/// Skip Kit updates when the collection and controlled id are
/// unchanged so an open searchable query survives an unrelated
/// Clojure rerender.
pub fn select_slot_sync(
    prev_fingerprint: u64,
    next_fingerprint: u64,
    prev_selected: Option<&str>,
    next_selected: Option<&str>,
) -> SelectSlotSync {
    let set_items = prev_fingerprint != next_fingerprint;
    SelectSlotSync {
        set_items,
        set_selected: set_items || prev_selected != next_selected,
    }
}

pub fn select_live_sync(
    prev_fingerprint: u64,
    next_fingerprint: u64,
    prev_selected: Option<&str>,
    next_selected: Option<&str>,
) -> SelectLiveSync {
    let sync = select_slot_sync(
        prev_fingerprint,
        next_fingerprint,
        prev_selected,
        next_selected,
    );
    if sync.set_items {
        SelectLiveSync::Rebuild
    } else if sync.set_selected {
        SelectLiveSync::SetValue
    } else {
        SelectLiveSync::Leave
    }
}

/// Full-list Kit `IndexPath` for `SelectState::new` only (no live query).
/// Flat lists stay section 0; grouped lists use section+row from
/// [`select_sections`]. Do not pass this into a filtered delegate —
/// see [`SelectLiveSync::SetValue`].
pub fn select_index(items: &[Item], selected: Option<&str>) -> Option<gpui_component::IndexPath> {
    let id = selected?;
    if select_is_grouped(items) {
        for (section, group) in select_sections(items).iter().enumerate() {
            for (row, leaf) in group.items.iter().enumerate() {
                if leaf.id == id {
                    return Some(
                        gpui_component::IndexPath::default()
                            .section(section)
                            .row(row),
                    );
                }
            }
        }
        None
    } else {
        items
            .iter()
            .position(|item| item.id_or_label() == id)
            .map(|ix| gpui_component::IndexPath::default().row(ix))
    }
}

/// Ids Kit would keep after `SearchableVec<SelectGroup<_>>::perform_search`.
#[cfg(test)]
pub fn select_group_search_ids(sections: &[SelectSection], query: &str) -> Vec<String> {
    let q = query.to_lowercase();
    let mut ids = Vec::new();
    for section in sections {
        let title_hit = section.title.to_lowercase().contains(&q);
        let item_hit = section
            .items
            .iter()
            .any(|item| item.label.to_lowercase().contains(&q));
        if !title_hit && !item_hit {
            continue;
        }
        for item in &section.items {
            if item.label.to_lowercase().contains(&q) {
                ids.push(item.id.clone());
            }
        }
    }
    ids
}

/// Kit `SearchableVec<SelectGroup<_>>::perform_search` section+row for
/// `id` under `query`. Empty sections from a title-only hit still occupy
/// a section index, matching Kit.
#[cfg(test)]
fn select_group_matched_index(
    sections: &[SelectSection],
    id: &str,
    query: &str,
) -> Option<(usize, usize)> {
    let q = query.to_lowercase();
    let mut section_ix = 0usize;
    for section in sections {
        let title_hit = section.title.to_lowercase().contains(&q);
        let item_hit = section
            .items
            .iter()
            .any(|item| item.label.to_lowercase().contains(&q));
        if !title_hit && !item_hit {
            continue;
        }
        if let Some(row) = section
            .items
            .iter()
            .filter(|item| item.label.to_lowercase().contains(&q))
            .position(|item| item.id == id)
        {
            return Some((section_ix, row));
        }
        section_ix += 1;
    }
    None
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
#[derive(Debug, Clone)]
pub struct ChartPoint {
    pub index: usize,
    pub id: String,
    pub label: String,
    pub value: Option<f64>,
    pub values: Vec<f64>,
    pub color: Option<String>,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: Option<f64>,
    #[allow(dead_code)]
    pub source: Option<String>,
    #[allow(dead_code)]
    pub target: Option<String>,
    pub label_lines: Vec<ChartLabelLine>,
    pub content: Option<Box<Node>>,
    pub inner_radius: Option<f32>,
    pub outer_radius: Option<f32>,
}

impl ChartPoint {
    pub fn from_item(item: &Item) -> Self {
        let values = item_number_list(item);
        let value = item
            .number_value()
            .map(|n| n as f64)
            .or_else(|| values.first().copied());
        Self {
            index: 0,
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
            label_lines: item.label_lines.clone(),
            content: item.content.clone(),
            inner_radius: item.inner_radius,
            outer_radius: item.outer_radius,
        }
    }

    pub fn with_index(mut self, index: usize) -> Self {
        self.index = index;
        self
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
        .enumerate()
        .map(|(index, point)| point.with_index(index))
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

/// Kit's `build_band_labels` / `build_point_x_labels` do `(i + 1) % tick_margin`
/// and do not clamp. Zero panics. The bridge always forwards ≥1.
pub fn chart_tick_margin(node: &Node) -> usize {
    node.tick_margin.unwrap_or(1).max(1) as usize
}

/// Kit hover tooltip is off until `.id(...)` is set. Default follows Kit.
pub fn chart_interactive(node: &Node) -> bool {
    node.interactive.unwrap_or(false)
}

/// Candlestick `body_width_ratio`. Kit's builder does not clamp; neither do we.
pub fn chart_body_width_ratio(node: &Node) -> Option<f32> {
    node.body_width_ratio
}

/// Theme tokens Kit cycles for radar / sankey / line-area series (`chart_1`…`chart_5`).
const CHART_SERIES_TOKENS: [&str; 5] = ["chart_1", "chart_2", "chart_3", "chart_4", "chart_5"];

#[cfg(test)]
pub fn chart_series_token(index: usize) -> &'static str {
    CHART_SERIES_TOKENS[index % CHART_SERIES_TOKENS.len()]
}

fn chart_5_palette(cx: &App) -> [Hsla; 5] {
    [
        cx.theme().chart_1,
        cx.theme().chart_2,
        cx.theme().chart_3,
        cx.theme().chart_4,
        cx.theme().chart_5,
    ]
}

/// Partial Sankey `:color` must not collapse the rest of the graph onto `chart_1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SankeyNodeColorKind {
    Custom(String),
    Palette(usize),
}

pub fn sankey_node_color_kind(index: usize, color: Option<&str>) -> SankeyNodeColorKind {
    match color {
        Some(c) if parse_hex_color(c).is_some() => SankeyNodeColorKind::Custom(c.to_string()),
        _ => SankeyNodeColorKind::Palette(index % CHART_SERIES_TOKENS.len()),
    }
}

/// Custom hex vs Kit's own default when a color builder is omitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChartColorKind {
    Custom(String),
    KitDefault,
}

pub fn chart_color_kind(color: Option<&str>) -> ChartColorKind {
    match color {
        Some(c) if parse_hex_color(c).is_some() => ChartColorKind::Custom(c.to_string()),
        _ => ChartColorKind::KitDefault,
    }
}

/// Area series stroke: custom hex, or Kit `chart_2` when the builder is omitted.
pub fn area_series_stroke_kind(color: Option<&str>) -> ChartColorKind {
    chart_color_kind(color)
}

/// Pie slice color: custom hex, or Kit `chart_2` when `.color(...)` is omitted.
pub fn pie_slice_color_kind(color: Option<&str>) -> ChartColorKind {
    chart_color_kind(color)
}

/// Last index that must receive an explicit builder so Kit's parallel `Vec`s
/// do not assign a later custom value to an earlier unspecified series.
pub fn last_custom_color_index(kinds: &[ChartColorKind]) -> Option<usize> {
    kinds
        .iter()
        .rposition(|kind| matches!(kind, ChartColorKind::Custom(_)))
}

pub fn pie_installs_color_fn(points: &[ChartPoint]) -> bool {
    points.iter().any(|p| {
        matches!(
            pie_slice_color_kind(p.color.as_deref()),
            ChartColorKind::Custom(_)
        )
    })
}

pub fn pie_uses_inner_radius_fn(points: &[ChartPoint]) -> bool {
    points.iter().any(|p| p.inner_radius.is_some())
}

pub fn pie_uses_outer_radius_fn(points: &[ChartPoint]) -> bool {
    points.iter().any(|p| p.outer_radius.is_some())
}

/// Kit pie *layout* uses `height × 0.4` when `outer_radius` is 0, but
/// `get_outer_radius` still returns 0 into `arc.paint`. Kit then drops the
/// path (`r1 < EPSILON`), so a donut with only `:inner-radius` shows labels
/// and no ring. Forward that layout default so paint matches labels.
pub fn pie_paint_outer_radius(node: &Node) -> f32 {
    node.outer_radius
        .unwrap_or_else(|| chart_viewport(node).1 * 0.4)
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChartFillGradient {
    PerBar,
    Chart,
    Stops {
        start: String,
        start_at: f32,
        end: String,
        end_at: f32,
    },
}

fn gradient_stop(value: &Value) -> Option<(String, f32)> {
    match value {
        Value::Object(map) => {
            let color = map.get("color")?.as_str()?.to_string();
            if color.is_empty() {
                return None;
            }
            let at = map.get("at").and_then(json_f64).unwrap_or(0.0) as f32;
            Some((color, at))
        }
        _ => None,
    }
}

pub fn chart_fill_gradient(node: &Node) -> Option<ChartFillGradient> {
    let mode = node
        .fill_gradient_mode
        .as_deref()
        .map(crate::catalog::normalize);
    match &node.fill_gradient {
        Some(Value::Array(stops)) if stops.len() >= 2 => {
            let (start, start_at) = gradient_stop(&stops[0])?;
            let (end, end_at) = gradient_stop(&stops[1])?;
            Some(ChartFillGradient::Stops {
                start,
                start_at,
                end,
                end_at,
            })
        }
        Some(Value::String(s)) => match crate::catalog::normalize(s).as_str() {
            "chart" => Some(ChartFillGradient::Chart),
            "bar" | "per bar" | "true" => Some(ChartFillGradient::PerBar),
            _ => None,
        },
        Some(Value::Bool(true)) => {
            if mode.as_deref() == Some("chart") {
                Some(ChartFillGradient::Chart)
            } else {
                Some(ChartFillGradient::PerBar)
            }
        }
        _ => None,
    }
}

fn json_object_f32(map: &serde_json::Map<String, Value>, key: &str) -> f32 {
    map.get(key).and_then(json_f64).unwrap_or(0.0) as f32
}

pub fn chart_corner_radii(node: &Node) -> Option<Corners<gpui::Pixels>> {
    if let Some(Value::Number(n)) = node.corner_radii.as_ref() {
        return Some(Corners::all(px(n.as_f64()? as f32)));
    }
    if let Some(Value::Object(map)) = node.corner_radii.as_ref() {
        return Some(Corners {
            top_left: px(json_object_f32(map, "top-left")),
            top_right: px(json_object_f32(map, "top-right")),
            bottom_right: px(json_object_f32(map, "bottom-right")),
            bottom_left: px(json_object_f32(map, "bottom-left")),
        });
    }
    node.corner_radius.map(|r| Corners::all(px(r)))
}

fn normalized_opt(value: Option<&str>) -> Option<String> {
    value.map(crate::catalog::normalize)
}

/// Kit `StrokeStyle` name after catalog normalize, if the wire value is known.
pub fn chart_stroke_style_name(value: Option<&str>) -> Option<&'static str> {
    match normalized_opt(value).as_deref() {
        Some("linear") => Some("linear"),
        Some("step after") | Some("stepafter") => Some("step-after"),
        Some("natural") => Some("natural"),
        _ => None,
    }
}

fn ensure_series_values(points: &mut [ChartPoint]) {
    for point in points {
        if point.values.is_empty() {
            if let Some(v) = point.value {
                point.values = vec![v];
            }
        }
    }
}

struct ChartSeriesMeta {
    name: String,
    stroke: Option<String>,
    fill: Option<String>,
    stroke_style: Option<String>,
}

fn custom_hex(color: Option<&str>) -> Option<String> {
    match chart_color_kind(color) {
        ChartColorKind::Custom(c) => Some(c),
        ChartColorKind::KitDefault => None,
    }
}

fn chart_series_meta(node: &Node) -> Vec<ChartSeriesMeta> {
    node.series
        .iter()
        .map(|item| ChartSeriesMeta {
            name: item.label_or_id(),
            stroke: custom_hex(item.stroke.as_deref().or(item.color.as_deref())),
            fill: custom_hex(item.fill.as_deref()),
            stroke_style: item.stroke_style.clone(),
        })
        .collect()
}

fn area_stroke_hex<'a>(meta: &'a [ChartSeriesMeta], i: usize, node: &'a Node) -> Option<&'a str> {
    meta.get(i).and_then(|s| s.stroke.as_deref()).or_else(|| {
        if i == 0 && meta.is_empty() {
            node.stroke.as_deref()
        } else {
            None
        }
    })
}

fn area_fill_hex(meta: &[ChartSeriesMeta], i: usize) -> Option<&str> {
    meta.get(i).and_then(|s| s.fill.as_deref())
}

fn area_style_raw<'a>(meta: &'a [ChartSeriesMeta], i: usize, node: &'a Node) -> Option<&'a str> {
    meta.get(i)
        .and_then(|s| s.stroke_style.as_deref())
        .or_else(|| {
            if i == 0 && meta.is_empty() {
                node.stroke_style.as_deref()
            } else {
                None
            }
        })
}

fn radar_dimension_label(point: &ChartPoint) -> RadarLabel {
    if let Some(content) = point.content.as_deref() {
        RadarLabel::Element(crate::overlay::paint_chart_label(
            content,
            &format!("radar-label/{}", point.index),
        ))
    } else {
        RadarLabel::Text(SharedString::from(point.label.clone()))
    }
}

fn sankey_fallback_labels(node: &ChartPoint, value: f64) -> Vec<SankeyLabel> {
    let mut lines = Vec::new();
    lines.push(SankeyLabel::new(format_chart_number(value)));
    if !node.label.is_empty() {
        lines.push(SankeyLabel::new(node.label.clone()));
    }
    lines
}

fn sankey_custom_labels(node: &ChartPoint, value: f64) -> Vec<SankeyLabel> {
    if node.label_lines.is_empty() {
        return sankey_fallback_labels(node, value);
    }
    node.label_lines
        .iter()
        .map(|line| {
            let mut label = SankeyLabel::new(line.text.clone());
            if let Some(color) = line.color.as_deref().and_then(parse_hex_color) {
                label = label.color(color);
            }
            if let Some(size) = line.font_size {
                label = label.font_size(size);
            }
            label
        })
        .collect()
}

pub fn sankey_nodes(node: &Node) -> Vec<ChartPoint> {
    node.collection()
        .iter()
        .map(ChartPoint::from_item)
        .filter(|p| !p.id.is_empty() || !p.label.is_empty())
        .enumerate()
        .map(|(index, point)| point.with_index(index))
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

pub fn paint_chart(node: &Node, key: &str, cx: &App) -> gpui::AnyElement {
    let mut points = chart_points(node);
    let kind = chart_kind(node);
    let stroke = cx.theme().chart_1;
    let chart: gpui::AnyElement = match kind.as_str() {
        "bar" => {
            let mut bar = BarChart::new(points)
                .band(|p| p.label.clone())
                .value(|p| p.series_y().unwrap_or(0.0))
                .alignment(bar_alignment(node));
            if chart_interactive(node) {
                bar = bar.id(SharedString::from(format!("{key}/plot")));
            }
            if let Some(name) = node.name.as_ref() {
                bar = bar.name(name.clone());
            }
            match chart_fill_gradient(node) {
                Some(ChartFillGradient::PerBar) => {
                    bar = bar.fill_gradient(move |p, _, _| {
                        let color = point_fill(p, stroke);
                        [
                            linear_color_stop(color.opacity(0.3), 0.0),
                            linear_color_stop(color, 1.0),
                        ]
                    });
                }
                Some(ChartFillGradient::Chart) => {
                    bar = bar.fill_gradient(move |p, range, chart_to_bar| {
                        let color = point_fill(p, stroke);
                        [
                            linear_color_stop(color.opacity(0.3), chart_to_bar(*range.start())),
                            linear_color_stop(color, chart_to_bar(*range.end())),
                        ]
                    });
                }
                Some(ChartFillGradient::Stops {
                    start,
                    start_at,
                    end,
                    end_at,
                }) => {
                    let start_c = parse_hex_color(&start).unwrap_or(stroke);
                    let end_c = parse_hex_color(&end).unwrap_or(stroke);
                    bar = bar.fill_gradient(move |_, _, _| {
                        [
                            linear_color_stop(start_c, start_at),
                            linear_color_stop(end_c, end_at),
                        ]
                    });
                }
                None => {
                    bar = bar.fill(move |p, _, _, _| point_fill(p, stroke));
                }
            }
            if let Some(corners) = chart_corner_radii(node) {
                bar = bar.corner_radii(corners);
            }
            bar = bar
                .label_axis(node.label_axis.unwrap_or(true))
                .value_axis(node.value_axis.unwrap_or(false))
                .grid(node.grid.unwrap_or(true))
                .tick_margin(chart_tick_margin(node));
            if let Some(ticks) = node.value_tick_count {
                bar = bar.value_tick_count(ticks as usize);
            }
            if node.labels.unwrap_or(false) {
                bar = bar.label(|p| format_chart_number(p.series_y().unwrap_or(0.0)));
            }
            bar.into_any_element()
        }
        "area" => {
            ensure_series_values(&mut points);
            let n = points
                .iter()
                .map(|p| p.values.len())
                .max()
                .unwrap_or(1)
                .max(1);
            let meta = chart_series_meta(node);
            let stroke_kinds: Vec<ChartColorKind> = (0..n)
                .map(|i| area_series_stroke_kind(area_stroke_hex(&meta, i, node)))
                .collect();
            let fill_kinds: Vec<ChartColorKind> = (0..n)
                .map(|i| chart_color_kind(area_fill_hex(&meta, i)))
                .collect();
            let last_stroke = last_custom_color_index(&stroke_kinds);
            let last_fill = last_custom_color_index(&fill_kinds);
            let last_style = (0..n)
                .rev()
                .find(|&i| chart_stroke_style_name(area_style_raw(&meta, i, node)).is_some());
            let any_name = meta.iter().any(|s| !s.name.is_empty());
            let kit_chart_2 = cx.theme().chart_2;
            let mut chart = AreaChart::new(points).x(|p| p.label.clone());
            if chart_interactive(node) {
                chart = chart.id(SharedString::from(format!("{key}/plot")));
            }
            for i in 0..n {
                chart = chart.y(move |p| p.values.get(i).copied().unwrap_or(0.0));
                if any_name {
                    chart = chart.name(meta.get(i).map(|s| s.name.clone()).unwrap_or_default());
                }
                if last_stroke.is_some_and(|last| i <= last) {
                    let series_stroke = area_stroke_hex(&meta, i, node)
                        .and_then(parse_hex_color)
                        .unwrap_or(kit_chart_2);
                    chart = chart.stroke(series_stroke);
                }
                if last_fill.is_some_and(|last| i <= last) {
                    let series_fill = area_fill_hex(&meta, i)
                        .and_then(parse_hex_color)
                        .unwrap_or(kit_chart_2.opacity(0.4));
                    chart = chart.fill(series_fill);
                }
                if last_style.is_some_and(|last| i <= last) {
                    chart = match chart_stroke_style_name(area_style_raw(&meta, i, node)) {
                        Some("linear") => chart.linear(),
                        Some("step-after") => chart.step_after(),
                        _ => chart.natural(),
                    };
                }
            }
            chart = chart.tick_margin(chart_tick_margin(node));
            if let Some(show) = node.x_axis {
                chart = chart.x_axis(show);
            }
            if let Some(grid) = node.grid {
                chart = chart.grid(grid);
            }
            chart.into_any_element()
        }
        "pie" => {
            let pie_data: Vec<ChartPoint> = points
                .into_iter()
                .filter(|p| p.series_y().is_some())
                .collect();
            let install_color = pie_installs_color_fn(&pie_data);
            let any_inner = pie_uses_inner_radius_fn(&pie_data);
            let any_outer = pie_uses_outer_radius_fn(&pie_data);
            let node_inner = node.inner_radius.unwrap_or(0.0);
            let paint_outer = pie_paint_outer_radius(node);
            let kit_chart_2 = cx.theme().chart_2;
            let mut pie = PieChart::new(pie_data).value(|p| p.series_y().unwrap_or(0.0) as f32);
            if install_color {
                pie = pie.color(move |p| {
                    p.color
                        .as_deref()
                        .and_then(parse_hex_color)
                        .unwrap_or(kit_chart_2)
                });
            }
            if any_inner {
                pie = pie.inner_radius_fn(move |arc| arc.data.inner_radius.unwrap_or(node_inner));
            } else if let Some(radius) = node.inner_radius {
                pie = pie.inner_radius(radius);
            }
            // Always install a paint outer radius. Kit's own default of 0 is
            // only a layout sentinel; `arc.paint(Some(0))` draws nothing.
            pie = pie.outer_radius(paint_outer);
            if any_outer {
                pie = pie.outer_radius_fn(move |arc| arc.data.outer_radius.unwrap_or(paint_outer));
            }
            if let Some(angle) = node.pad_angle {
                pie = pie.pad_angle(angle);
            }
            if node.labels.unwrap_or(false) {
                pie = pie.label(|p| SharedString::from(p.label.clone()));
            }
            if let Some(color) = node.label_color.as_deref().and_then(parse_hex_color) {
                pie = pie.label_color(color);
            }
            if let Some(color) = node.label_line_color.as_deref().and_then(parse_hex_color) {
                pie = pie.label_line_color(move |_| color);
            }
            if let Some(gap) = node.label_gap {
                pie = pie.label_gap(gap);
            }
            pie.into_any_element()
        }
        "radar" => {
            ensure_series_values(&mut points);
            let n = points.iter().map(|p| p.values.len()).max().unwrap_or(0);
            let meta = chart_series_meta(node);
            let any_stroke = meta.iter().any(|s| s.stroke.is_some());
            let any_fill = meta.iter().any(|s| s.fill.is_some());
            let any_name = meta.iter().any(|s| !s.name.is_empty());
            let palette = chart_5_palette(cx);
            let mut radar = RadarChart::new(points).label(radar_dimension_label);
            if chart_interactive(node) {
                radar = radar.id(SharedString::from(format!("{key}/plot")));
            }
            if let Some(max) = node.max {
                radar = radar.max_value(max as f64);
            }
            if node.dot {
                radar = radar.dot();
            }
            radar = radar.grid(node.grid.unwrap_or(true));
            if let Some(color) = node.label_color.as_deref().and_then(parse_hex_color) {
                radar = radar.label_color(color);
            }
            if let Some(gap) = node.label_gap {
                radar = radar.label_gap(gap);
            }
            if let Some(radius) = node.outer_radius {
                radar = radar.outer_radius(radius);
            }
            if let Some(levels) = node.grid_levels {
                radar = radar.grid_levels(levels as usize);
            }
            for i in 0..n {
                radar = radar.value(move |p| p.values.get(i).copied().unwrap_or(0.0));
                if any_name {
                    radar = radar.name(meta.get(i).map(|s| s.name.clone()).unwrap_or_default());
                }
                if any_stroke {
                    let series_stroke = meta
                        .get(i)
                        .and_then(|s| s.stroke.as_deref())
                        .and_then(parse_hex_color)
                        .unwrap_or(palette[i % palette.len()]);
                    radar = radar.stroke(series_stroke);
                }
                if any_fill {
                    let series_fill = meta
                        .get(i)
                        .and_then(|s| s.fill.as_deref())
                        .and_then(parse_hex_color)
                        .unwrap_or_else(|| {
                            meta.get(i)
                                .and_then(|s| s.stroke.as_deref())
                                .and_then(parse_hex_color)
                                .unwrap_or(palette[i % palette.len()])
                                .opacity(0.3)
                        });
                    radar = radar.fill(series_fill);
                }
            }
            radar.into_any_element()
        }
        "candlestick" => {
            let candles: Vec<ChartPoint> = node
                .collection()
                .iter()
                .map(ChartPoint::from_item)
                .filter(|p| !p.label.is_empty() && p.has_ohlc())
                .enumerate()
                .map(|(index, point)| point.with_index(index))
                .collect();
            let mut chart = CandlestickChart::new(candles)
                .x(|p| p.label.clone())
                .open(|p| p.open.unwrap_or(0.0))
                .high(|p| p.high.unwrap_or(0.0))
                .low(|p| p.low.unwrap_or(0.0))
                .close(|p| p.close.unwrap_or(0.0))
                .tick_margin(chart_tick_margin(node))
                .grid(node.grid.unwrap_or(true));
            if let Some(ratio) = chart_body_width_ratio(node) {
                chart = chart.body_width_ratio(ratio);
            }
            if let Some(show) = node.x_axis {
                chart = chart.x_axis(show);
            }
            chart.into_any_element()
        }
        "sankey" => {
            let nodes = sankey_nodes(node);
            let links = sankey_links(&nodes, &node.links);
            let palette = chart_5_palette(cx);
            let any_custom = nodes.iter().any(|n| {
                matches!(
                    sankey_node_color_kind(n.index, n.color.as_deref()),
                    SankeyNodeColorKind::Custom(_)
                )
            });
            let custom_labels = nodes.iter().any(|n| !n.label_lines.is_empty());
            let mut chart = SankeyChart::new(nodes, links);
            if custom_labels {
                chart = chart.labels(sankey_custom_labels);
            } else {
                if node.node_label.unwrap_or(true) {
                    chart = chart.node_label(|n| n.label.clone().into());
                }
                if node.value_label.unwrap_or(true) {
                    chart = chart.value_label(|_, v| format_chart_number(v).into());
                }
            }
            if let Some(align) = sankey_align(node) {
                chart = chart.node_align(align);
            }
            if let Some(scale) = sankey_value_scale(node) {
                chart = chart.value_scale(scale);
            }
            if let Some(width) = node.node_width {
                chart = chart.node_width(width);
            }
            if let Some(padding) = node.node_padding {
                chart = chart.node_padding(padding);
            }
            if let Some(iterations) = node.iterations {
                chart = chart.iterations(iterations as usize);
            }
            if let Some(radius) = node.node_corner_radius {
                chart = chart.node_corner_radius(px(radius));
            }
            if let Some(opacity) = node.link_opacity {
                chart = chart.link_opacity(opacity);
            }
            if let Some(width) = node.min_link_width {
                chart = chart.min_link_width(width);
            }
            if let Some(gap) = node.label_gap {
                chart = chart.label_gap(gap);
            }
            if any_custom {
                chart = chart.node_color(move |n| {
                    match sankey_node_color_kind(n.index, n.color.as_deref()) {
                        SankeyNodeColorKind::Custom(color) => {
                            parse_hex_color(&color).unwrap_or(palette[n.index % palette.len()])
                        }
                        SankeyNodeColorKind::Palette(index) => palette[index],
                    }
                });
            }
            chart.into_any_element()
        }
        _ => {
            let mut chart = LineChart::new(points)
                .x(|p| p.label.clone())
                .y(|p| p.series_y().unwrap_or(0.0))
                .tick_margin(chart_tick_margin(node));
            if chart_interactive(node) {
                chart = chart.id(SharedString::from(format!("{key}/plot")));
            }
            if let Some(name) = node.name.as_ref() {
                chart = chart.name(name.clone());
            }
            if let Some(color) = node.stroke.as_deref().and_then(parse_hex_color) {
                chart = chart.stroke(color);
            }
            chart = match chart_stroke_style_name(node.stroke_style.as_deref()) {
                Some("linear") => chart.linear(),
                Some("step-after") => chart.step_after(),
                Some("natural") => chart.natural(),
                _ => chart,
            };
            if node.dot {
                chart = chart.dot();
            }
            if let Some(show) = node.x_axis {
                chart = chart.x_axis(show);
            }
            if let Some(grid) = node.grid {
                chart = chart.grid(grid);
            }
            chart.into_any_element()
        }
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
    fn pagination_pages_follow_kit_defaults() {
        let omitted: Node = serde_json::from_value(json!({"type": "pagination"})).unwrap();
        assert_eq!(pagination_current_page(&omitted), 1);
        assert_eq!(pagination_total_pages(&omitted), 1);
        assert_eq!(pagination_visible_pages(&omitted), None);
        let node: Node = serde_json::from_value(json!({
            "type": "pagination",
            "value": 4,
            "total": 12,
            "visible-pages": 7,
            "compact": true
        }))
        .unwrap();
        assert_eq!(pagination_current_page(&node), 4);
        assert_eq!(pagination_total_pages(&node), 12);
        assert_eq!(pagination_visible_pages(&node), Some(7));
        assert!(node.compact);
        let zero: Node = serde_json::from_value(json!({
            "type": "pagination",
            "value": 0,
            "total": 0
        }))
        .unwrap();
        assert_eq!(pagination_current_page(&zero), 0);
        assert_eq!(pagination_total_pages(&zero), 0);
    }

    #[test]
    fn progress_circle_and_shimmer_options() {
        let circle: Node = serde_json::from_value(json!({
            "type": "progress-circle",
            "value": 72.5,
            "loading": true,
            "color": "#3366ff",
            "accessibility-label": "Upload"
        }))
        .unwrap();
        assert_eq!(progress_circle_value(&circle), 72.5);
        assert!(circle.loading);
        assert_eq!(circle.accessibility_label.as_deref(), Some("Upload"));
        let over: Node = serde_json::from_value(json!({
            "type": "progress-circle",
            "value": 150
        }))
        .unwrap();
        assert_eq!(progress_circle_value(&over), 150.0);
        let shimmer: Node = serde_json::from_value(json!({
            "type": "shimmer",
            "text": "Thinking…",
            "duration": 0,
            "spread": 0.4,
            "spread-px": 48,
            "reverse": true,
            "once": true,
            "highlight-color": "#ffffff"
        }))
        .unwrap();
        assert_eq!(shimmer_duration_secs(&shimmer), Some(0.0));
        match shimmer_spread(&shimmer) {
            Some(ShimmerSpreadSpec::Absolute(px)) => assert_eq!(px, 48.0),
            other => panic!("expected spread-px to win, got {other:?}"),
        }
        assert!(shimmer.reverse);
        assert!(shimmer.once);
        let relative: Node = serde_json::from_value(json!({
            "type": "shimmer",
            "spread": 0.12
        }))
        .unwrap();
        match shimmer_spread(&relative) {
            Some(ShimmerSpreadSpec::Relative(n)) => assert_eq!(n, 0.12),
            other => panic!("expected relative spread, got {other:?}"),
        }
        let bad: Node = serde_json::from_value(json!({
            "type": "shimmer",
            "duration": -1
        }))
        .unwrap();
        assert_eq!(shimmer_duration_secs(&bad), None);
    }

    #[test]
    fn hover_card_delays_and_avatar_group_limit() {
        assert_eq!(hover_card_delay_secs(None), None);
        assert_eq!(hover_card_delay_secs(Some(0.0)), Some(0.0));
        assert_eq!(hover_card_delay_secs(Some(0.2)), Some(0.2));
        assert_eq!(hover_card_delay_secs(Some(-1.0)), None);
        let avatar: Node = serde_json::from_value(json!({
            "type": "avatar",
            "text": "Ada",
            "src": "https://example.com/ada.png",
            "icon": "building-2"
        }))
        .unwrap();
        assert_eq!(avatar_src(&avatar), Some("https://example.com/ada.png"));
        let empty_src: Node = serde_json::from_value(json!({
            "type": "avatar",
            "src": ""
        }))
        .unwrap();
        assert_eq!(avatar_src(&empty_src), None);
        let group: Node = serde_json::from_value(json!({
            "type": "avatar-group",
            "limit": 5,
            "ellipsis": true
        }))
        .unwrap();
        assert_eq!(avatar_group_limit(&group), Some(5));
        assert!(group.ellipsis);
        let omitted: Node = serde_json::from_value(json!({"type": "avatar-group"})).unwrap();
        assert_eq!(avatar_group_limit(&omitted), None);
        assert!(!omitted.ellipsis);
        let zero: Node = serde_json::from_value(json!({
            "type": "avatar-group",
            "limit": 0
        }))
        .unwrap();
        assert_eq!(avatar_group_limit(&zero), Some(0));
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

    fn select_item(id: &str, label: &str) -> Item {
        Item {
            id: Some(id.into()),
            label: Some(label.into()),
            ..Item::default()
        }
    }

    fn select_group(title: &str, items: Vec<Item>) -> Item {
        Item {
            label: Some(title.into()),
            items,
            ..Item::default()
        }
    }

    #[test]
    fn select_index_uses_section_and_row_for_groups() {
        let flat = vec![select_item("clj", "Clojure"), select_item("rs", "Rust")];
        assert!(!select_is_grouped(&flat));
        let a = select_index(&flat, Some("clj")).unwrap();
        let b = select_index(&flat, Some("rs")).unwrap();
        assert_eq!(a.section, 0);
        assert_eq!(a.row, 0);
        assert_eq!(b.row, 1);
        assert!(select_index(&flat, None).is_none());
        assert!(select_index(&flat, Some("go")).is_none());

        let grouped = vec![
            select_group(
                "Lisp",
                vec![
                    select_item("clj", "Clojure"),
                    select_item("cljs", "ClojureScript"),
                ],
            ),
            select_group(
                "Systems",
                vec![select_item("rs", "Rust"), select_item("go", "Go")],
            ),
        ];
        assert!(select_is_grouped(&grouped));
        let clj = select_index(&grouped, Some("clj")).unwrap();
        assert_eq!(clj.section, 0);
        assert_eq!(clj.row, 0);
        let rs = select_index(&grouped, Some("rs")).unwrap();
        assert_eq!(rs.section, 1);
        assert_eq!(rs.row, 0);
        let go = select_index(&grouped, Some("go")).unwrap();
        assert_eq!(go.section, 1);
        assert_eq!(go.row, 1);
        assert!(select_index(&grouped, Some("python")).is_none());
    }

    #[test]
    fn select_mixed_leaves_share_an_untitled_section() {
        let mixed = vec![
            select_item("plain", "Plain"),
            select_group("A", vec![select_item("apple", "Apple")]),
            select_item("tail", "Tail"),
        ];
        let sections = select_sections(&mixed);
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].title, "");
        assert_eq!(sections[0].items[0].id, "plain");
        assert_eq!(sections[1].title, "A");
        assert_eq!(sections[2].title, "");
        assert_eq!(sections[2].items[0].id, "tail");
        let tail = select_index(&mixed, Some("tail")).unwrap();
        assert_eq!(tail.section, 2);
        assert_eq!(tail.row, 0);
    }

    #[test]
    fn select_group_search_filters_titles_not_ids() {
        let sections = select_sections(&[select_group(
            "Lisp",
            vec![select_item("clj", "Clojure"), select_item("rs", "Rust")],
        )]);
        assert_eq!(
            select_group_search_ids(&sections, "clo"),
            vec!["clj".to_string()]
        );
        assert!(
            select_group_search_ids(&sections, "clj").is_empty(),
            "filter is on title, not id"
        );
        assert_eq!(
            select_group_search_ids(&sections, "isp"),
            Vec::<String>::new(),
            "a group-title hit with no matching row keeps an empty section"
        );
        assert_eq!(
            select_group_search_ids(&sections, "ust"),
            vec!["rs".to_string()]
        );
    }

    #[test]
    fn select_unrelated_rerender_skips_set_items() {
        let items = vec![select_group("Lisp", vec![select_item("clj", "Clojure")])];
        let fp = select_fingerprint(&items);
        let sync = select_slot_sync(fp, fp, Some("clj"), Some("clj"));
        assert!(!sync.set_items);
        assert!(!sync.set_selected);
        assert_eq!(
            select_live_sync(fp, fp, Some("clj"), Some("clj")),
            SelectLiveSync::Leave,
            "native Confirm cache matching a Clojure echo must not clear the query"
        );
        let renamed = vec![select_group(
            "Lisp",
            vec![Item {
                id: Some("clj".into()),
                label: Some("Clojure lang".into()),
                ..Item::default()
            }],
        )];
        let items_changed =
            select_slot_sync(fp, select_fingerprint(&renamed), Some("clj"), Some("clj"));
        assert!(items_changed.set_items);
        assert!(items_changed.set_selected);
        assert_eq!(
            select_live_sync(fp, select_fingerprint(&renamed), Some("clj"), Some("clj")),
            SelectLiveSync::Rebuild,
            "a real collection change must rebuild so query text and matched_items agree"
        );
        let sel_only = select_slot_sync(fp, fp, Some("clj"), Some("rs"));
        assert!(!sel_only.set_items);
        assert!(sel_only.set_selected);
        assert_eq!(
            select_live_sync(fp, fp, Some("clj"), Some("rs")),
            SelectLiveSync::SetValue
        );
        assert_eq!(
            select_live_sync(fp, fp, Some("clj"), None),
            SelectLiveSync::SetValue,
            "clearing uses set_selected_index(None), not a full-list path"
        );
        let display = vec![select_group(
            "Lisp",
            vec![Item {
                id: Some("clj".into()),
                label: Some("Clojure".into()),
                display: Some("Clojure (clj)".into()),
                ..Item::default()
            }],
        )];
        assert_ne!(fp, select_fingerprint(&display));
    }

    #[test]
    fn select_full_list_index_differs_from_filtered_matched_items() {
        let flat = vec![
            select_item("clj", "Clojure"),
            select_item("rs", "Rust"),
            select_item("go", "Go"),
        ];
        let full = select_index(&flat, Some("go")).unwrap();
        assert_eq!(full.section, 0);
        assert_eq!(full.row, 2, "full-list Go is row 2");
        let q = "go";
        let filtered_row = flat
            .iter()
            .filter(|item| item.label_or_id().to_lowercase().contains(q))
            .position(|item| item.id_or_label() == "go");
        assert_eq!(filtered_row, Some(0));
        assert_ne!(
            full.row, 0,
            "feeding select_index into a filtered SearchableVec would miss Go"
        );
        let fp = select_fingerprint(&flat);
        assert_eq!(
            select_live_sync(fp, fp, Some("clj"), Some("go")),
            SelectLiveSync::SetValue,
            "controlled sync must look up by value, not this IndexPath"
        );

        let grouped = vec![
            select_group(
                "Lisp",
                vec![
                    select_item("clj", "Clojure"),
                    select_item("cljs", "ClojureScript"),
                ],
            ),
            select_group(
                "Systems",
                vec![select_item("rs", "Rust"), select_item("go", "Go")],
            ),
        ];
        let full_go = select_index(&grouped, Some("go")).unwrap();
        assert_eq!(full_go.section, 1);
        assert_eq!(full_go.row, 1);
        let sections = select_sections(&grouped);
        assert_eq!(
            select_group_matched_index(&sections, "go", "go"),
            Some((0, 0)),
            "filtered Systems/Go is section 0 row 0"
        );
        assert_ne!((full_go.section, full_go.row), (0, 0));
        let gfp = select_fingerprint(&grouped);
        assert_eq!(
            select_live_sync(gfp, gfp, Some("clj"), Some("go")),
            SelectLiveSync::SetValue
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
    fn chart_interactive_is_opt_in() {
        let omitted: Node = serde_json::from_value(json!({"type": "chart"})).unwrap();
        assert!(
            !chart_interactive(&omitted),
            "Kit default is id: None; hover tooltip stays non-interactive"
        );
        let off: Node =
            serde_json::from_value(json!({"type": "chart", "interactive": false})).unwrap();
        assert!(!chart_interactive(&off));
        let on: Node =
            serde_json::from_value(json!({"type": "chart", "interactive": true})).unwrap();
        assert!(chart_interactive(&on));
    }

    #[test]
    fn pie_slice_color_kind_preserves_kit_default() {
        assert_eq!(pie_slice_color_kind(None), ChartColorKind::KitDefault);
        assert_eq!(
            pie_slice_color_kind(Some("not-a-color")),
            ChartColorKind::KitDefault
        );
        assert_eq!(
            pie_slice_color_kind(Some("#ff0000")),
            ChartColorKind::Custom("#ff0000".into())
        );
        let none: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "pie",
            "items": [
                {"label": "A", "value": 2},
                {"label": "B", "value": 5}
            ]
        }))
        .unwrap();
        assert!(!pie_installs_color_fn(&chart_points(&none)));
        let mixed: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "pie",
            "items": [
                {"label": "A", "value": 2, "color": "#3366ff"},
                {"label": "B", "value": 5}
            ]
        }))
        .unwrap();
        let points = chart_points(&mixed);
        assert!(pie_installs_color_fn(&points));
        assert_eq!(
            pie_slice_color_kind(points[1].color.as_deref()),
            ChartColorKind::KitDefault
        );
    }

    #[test]
    fn area_partial_series_stroke_keeps_kit_default() {
        assert_eq!(area_series_stroke_kind(None), ChartColorKind::KitDefault);
        assert_eq!(
            area_series_stroke_kind(Some("#ff0000")),
            ChartColorKind::Custom("#ff0000".into())
        );
        let kinds = [
            area_series_stroke_kind(Some("#ff0000")),
            area_series_stroke_kind(None),
        ];
        assert_eq!(last_custom_color_index(&kinds), Some(0));
        let later = [
            area_series_stroke_kind(None),
            area_series_stroke_kind(Some("#ff0000")),
        ];
        assert_eq!(last_custom_color_index(&later), Some(1));
        let node: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "area",
            "series": [
                {"stroke": "#ff0000"},
                {}
            ]
        }))
        .unwrap();
        let meta = chart_series_meta(&node);
        assert_eq!(meta[0].stroke.as_deref(), Some("#ff0000"));
        assert_eq!(meta[1].stroke, None);
        let kinds: Vec<_> = (0..2)
            .map(|i| area_series_stroke_kind(area_stroke_hex(&meta, i, &node)))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ChartColorKind::Custom("#ff0000".into()),
                ChartColorKind::KitDefault
            ]
        );
        assert_eq!(last_custom_color_index(&kinds), Some(0));
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

    #[test]
    fn chart_tick_margin_clamps_zero() {
        let omitted: Node = serde_json::from_value(json!({"type": "chart"})).unwrap();
        assert_eq!(chart_tick_margin(&omitted), 1);
        let zero: Node =
            serde_json::from_value(json!({"type": "chart", "tick-margin": 0})).unwrap();
        assert_eq!(chart_tick_margin(&zero), 1);
        let two: Node = serde_json::from_value(json!({"type": "chart", "tick-margin": 2})).unwrap();
        assert_eq!(chart_tick_margin(&two), 2);
    }

    #[test]
    fn sankey_partial_node_color_keeps_palette_index() {
        assert_eq!(
            sankey_node_color_kind(2, Some("#3366ff")),
            SankeyNodeColorKind::Custom("#3366ff".into())
        );
        assert_eq!(
            sankey_node_color_kind(0, None),
            SankeyNodeColorKind::Palette(0)
        );
        assert_eq!(
            sankey_node_color_kind(1, Some("not-a-color")),
            SankeyNodeColorKind::Palette(1)
        );
        assert_eq!(
            sankey_node_color_kind(4, None),
            SankeyNodeColorKind::Palette(4)
        );
        assert_eq!(
            sankey_node_color_kind(5, None),
            SankeyNodeColorKind::Palette(0)
        );
        assert_eq!(chart_series_token(0), "chart_1");
        assert_eq!(chart_series_token(4), "chart_5");
        assert_ne!(chart_series_token(0), chart_series_token(1));
        let node: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "sankey",
            "items": [
                {"id": "a", "label": "A"},
                {"id": "b", "label": "B"},
                {"id": "c", "label": "C", "color": "#ff00aa"},
                {"id": "d", "label": "D"},
                {"id": "e", "label": "E"}
            ]
        }))
        .unwrap();
        let kinds: Vec<_> = sankey_nodes(&node)
            .iter()
            .map(|n| sankey_node_color_kind(n.index, n.color.as_deref()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                SankeyNodeColorKind::Palette(0),
                SankeyNodeColorKind::Palette(1),
                SankeyNodeColorKind::Custom("#ff00aa".into()),
                SankeyNodeColorKind::Palette(3),
                SankeyNodeColorKind::Palette(4),
            ]
        );
    }

    #[test]
    fn chart_fill_gradient_and_corner_radii_parse() {
        let per_bar: Node = serde_json::from_value(json!({
            "type": "chart",
            "fill-gradient": true
        }))
        .unwrap();
        assert_eq!(
            chart_fill_gradient(&per_bar),
            Some(ChartFillGradient::PerBar)
        );
        let chart: Node = serde_json::from_value(json!({
            "type": "chart",
            "fill-gradient": true,
            "fill-gradient-mode": "chart"
        }))
        .unwrap();
        assert_eq!(chart_fill_gradient(&chart), Some(ChartFillGradient::Chart));
        let named: Node = serde_json::from_value(json!({
            "type": "chart",
            "fill-gradient": "chart"
        }))
        .unwrap();
        assert_eq!(chart_fill_gradient(&named), Some(ChartFillGradient::Chart));
        let stops: Node = serde_json::from_value(json!({
            "type": "chart",
            "fill-gradient": [
                {"color": "#111111", "at": -1.0},
                {"color": "#ffffff", "at": 1}
            ]
        }))
        .unwrap();
        assert_eq!(
            chart_fill_gradient(&stops),
            Some(ChartFillGradient::Stops {
                start: "#111111".into(),
                start_at: -1.0,
                end: "#ffffff".into(),
                end_at: 1.0,
            })
        );
        let uniform: Node = serde_json::from_value(json!({
            "type": "chart",
            "corner-radii": 4
        }))
        .unwrap();
        let corners = chart_corner_radii(&uniform).unwrap();
        assert_eq!(corners.top_left, px(4.0));
        assert_eq!(corners.bottom_right, px(4.0));
        let mapped: Node = serde_json::from_value(json!({
            "type": "chart",
            "corner-radii": {
                "top-left": 1,
                "top-right": 2,
                "bottom-right": 3,
                "bottom-left": 4
            }
        }))
        .unwrap();
        let corners = chart_corner_radii(&mapped).unwrap();
        assert_eq!(corners.top_left, px(1.0));
        assert_eq!(corners.top_right, px(2.0));
        assert_eq!(corners.bottom_right, px(3.0));
        assert_eq!(corners.bottom_left, px(4.0));
        let via_radius: Node = serde_json::from_value(json!({
            "type": "chart",
            "corner-radius": 6
        }))
        .unwrap();
        assert_eq!(chart_corner_radii(&via_radius).unwrap().top_left, px(6.0));
    }

    #[test]
    fn chart_stroke_style_and_line_opt_in_dot() {
        assert_eq!(chart_stroke_style_name(Some("linear")), Some("linear"));
        assert_eq!(
            chart_stroke_style_name(Some("step-after")),
            Some("step-after")
        );
        assert_eq!(
            chart_stroke_style_name(Some("step_after")),
            Some("step-after")
        );
        assert_eq!(chart_stroke_style_name(Some("natural")), Some("natural"));
        assert_eq!(chart_stroke_style_name(Some("nope")), None);
        let line: Node =
            serde_json::from_value(json!({"type": "chart", "variant": "line"})).unwrap();
        assert!(
            !line.dot,
            "Kit LineChart default is no dots; :dot is opt-in"
        );
    }

    #[test]
    fn area_and_radar_series_values_share_the_item_model() {
        let node: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "area",
            "items": [
                {"label": "Mon", "values": [4, 8]},
                {"label": "Tue", "value": [5, 9]}
            ],
            "series": [
                {"id": "desktop", "label": "Desktop", "color": "#3366ff", "fill": "#3366ff"},
                {"id": "mobile", "label": "Mobile", "stroke-style": "linear"}
            ]
        }))
        .unwrap();
        let points = chart_points(&node);
        assert_eq!(points[0].values, vec![4.0, 8.0]);
        assert_eq!(points[1].values, vec![5.0, 9.0]);
        assert_eq!(node.series[0].fill.as_deref(), Some("#3366ff"));
        assert_eq!(node.series[1].stroke_style.as_deref(), Some("linear"));
    }

    #[test]
    fn pie_donut_and_sankey_layout_fields_round_trip() {
        let pie: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "pie",
            "inner-radius": 40,
            "outer-radius": 70,
            "pad-angle": 0.04,
            "labels": true,
            "label-gap": 12,
            "items": [
                {
                    "label": "A",
                    "value": 2,
                    "inner-radius": 20,
                    "outer-radius": 80
                },
                {"label": "B", "value": 5}
            ]
        }))
        .unwrap();
        assert_eq!(pie.inner_radius, Some(40.0));
        assert_eq!(pie.outer_radius, Some(70.0));
        assert_eq!(pie.pad_angle, Some(0.04));
        assert_eq!(pie.labels, Some(true));
        let points = chart_points(&pie);
        assert_eq!(points[0].inner_radius, Some(20.0));
        assert_eq!(points[0].outer_radius, Some(80.0));
        assert_eq!(points[1].inner_radius, None);
        assert!(pie_uses_inner_radius_fn(&points));
        assert!(pie_uses_outer_radius_fn(&points));
        let donut_only_inner: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "pie",
            "height": 160,
            "inner-radius": 42
        }))
        .unwrap();
        assert_eq!(pie_paint_outer_radius(&donut_only_inner), 64.0);
        assert_eq!(pie_paint_outer_radius(&pie), 70.0);
        let default_h: Node =
            serde_json::from_value(json!({"type": "chart", "variant": "pie"})).unwrap();
        assert_eq!(pie_paint_outer_radius(&default_h), 180.0 * 0.4);
        let sankey: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "sankey",
            "node-width": 14,
            "node-padding": 20,
            "iterations": 8,
            "node-corner-radius": 3,
            "link-opacity": 0.5,
            "min-link-width": 2,
            "node-label": false,
            "items": [{
                "id": "a",
                "label": "A",
                "label-lines": [{"text": "A", "font-size": 11}]
            }]
        }))
        .unwrap();
        assert_eq!(sankey.node_width, Some(14.0));
        assert_eq!(sankey.node_label, Some(false));
        assert_eq!(sankey.items[0].label_lines[0].text, "A");
        assert_eq!(sankey.items[0].label_lines[0].font_size, Some(11.0));
        let radar: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "radar",
            "grid-levels": 6,
            "items": [{"label": "Speed", "value": 8, "content": {"type": "badge", "count": 1, "children": [{"type": "label", "text": "Go"}]}}]
        }))
        .unwrap();
        assert_eq!(radar.grid_levels, Some(6));
        let points = chart_points(&radar);
        assert_eq!(
            points[0].content.as_ref().map(|n| n.kind.as_str()),
            Some("badge")
        );
        let candle: Node = serde_json::from_value(json!({
            "type": "chart",
            "variant": "candlestick",
            "body-width-ratio": 1.5,
            "x-axis": false
        }))
        .unwrap();
        assert_eq!(chart_body_width_ratio(&candle), Some(1.5));
        assert_eq!(candle.body_width_ratio, Some(1.5));
        assert_eq!(candle.x_axis, Some(false));
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
