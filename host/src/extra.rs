//! Product widgets that sit on the v6 protocol: dates, colors, charts,
//! markdown, virtual lists, settings fields, and dock panels.
//!
//! Overlay sheet/notification collection lives in `overlay`. Slot maps and
//! `RootView::render_node` arms stay in `renderer`.

use crate::mapping;
use crate::protocol::{self, Cmd, Item, Node};
use chrono::NaiveDate;
use gpui::{
    div, prelude::*, px, size, App, Axis, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Hsla, IntoElement, ParentElement, Render, SharedString, Styled, Window,
};
use gpui_component::{
    calendar::Date,
    chart::{AreaChart, BarChart, LineChart, PieChart},
    dock::{Panel, PanelControl, PanelEvent},
    h_virtual_list,
    input::InputState,
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings},
    text::TextView,
    v_flex, v_virtual_list, ActiveTheme as _, Colorize as _, Placement, Side,
};
use serde_json::{json, Value};
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

pub fn chart_points(node: &Node) -> Vec<(String, f64)> {
    node.collection()
        .iter()
        .filter_map(|item| {
            let y = item.number_value()? as f64;
            let x = item.label_or_id();
            if x.is_empty() {
                None
            } else {
                Some((x, y))
            }
        })
        .collect()
}

pub fn paint_chart(node: &Node, key: &str, cx: &App) -> gpui::AnyElement {
    let points = chart_points(node);
    let width = node.width.or(node.size).unwrap_or(320.0);
    let height = node.height.or(node.size).unwrap_or(180.0);
    let kind = node
        .variant
        .as_deref()
        .map(crate::catalog::normalize)
        .unwrap_or_else(|| "line".into());
    let stroke = cx.theme().chart_1;
    let fill = cx.theme().chart_2;
    let chart: gpui::AnyElement = match kind.as_str() {
        "bar" => BarChart::new(points)
            .x(|p| p.0.clone())
            .y(|p| p.1)
            .fill(move |_| stroke)
            .into_any_element(),
        "area" => AreaChart::new(points)
            .x(|p| p.0.clone())
            .y(|p| p.1)
            .stroke(stroke)
            .fill(fill)
            .into_any_element(),
        "pie" => {
            let palette = [
                cx.theme().chart_1,
                cx.theme().chart_2,
                cx.theme().chart_3,
                cx.theme().chart_4,
                cx.theme().chart_5,
            ];
            let radius = width.min(height) * 0.42;
            PieChart::new(points)
                .value(|p| p.1 as f32)
                .outer_radius(radius)
                .color(move |p| {
                    let ix =
                        p.0.bytes()
                            .fold(0usize, |acc, b| acc.wrapping_add(b as usize));
                    palette[ix % palette.len()]
                })
                .into_any_element()
        }
        _ => LineChart::new(points)
            .x(|p| p.0.clone())
            .y(|p| p.1)
            .stroke(stroke)
            .dot()
            .into_any_element(),
    };
    v_flex()
        .id(SharedString::from(key.to_string()))
        .w(px(width))
        .h(px(height))
        .child(chart)
        .into_any_element()
}

pub fn paint_markdown(
    node: &Node,
    key: &str,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
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
        TextView::html(SharedString::from(key.to_string()), body, window, cx)
    } else {
        TextView::markdown(SharedString::from(key.to_string()), body, window, cx)
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
        }
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
            v_virtual_list(cx.entity(), "vlist", sizes, Self::paint_rows).into_any_element()
        } else {
            h_virtual_list(cx.entity(), "hlist", sizes, Self::paint_rows).into_any_element()
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
    pub cmd_tx: mpsc::Sender<Cmd>,
    focus: FocusHandle,
}

impl CljPanel {
    pub fn new(
        title: impl Into<SharedString>,
        live: Rc<RefCell<Node>>,
        path: String,
        cmd_tx: mpsc::Sender<Cmd>,
        focus: FocusHandle,
    ) -> Self {
        Self {
            title: title.into(),
            live,
            path,
            cmd_tx,
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
        paint_panel_body(&node, &self.path, self.cmd_tx.clone(), window, cx)
    }
}

pub fn paint_panel_body(
    node: &Node,
    path: &str,
    cmd_tx: mpsc::Sender<Cmd>,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
    match node.kind.as_str() {
        "markdown" | "html" => paint_markdown(node, path, window, cx),
        "chart" => paint_chart(node, path, cx),
        _ if !node.children.is_empty() => {
            crate::overlay::paint_static(&node.children, cmd_tx, path)
        }
        _ => crate::overlay::paint_static(std::slice::from_ref(node), cmd_tx, path),
    }
}

impl Panel for CljPanel {
    fn panel_name(&self) -> &'static str {
        "clj-gpui-panel"
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.title.clone()
    }

    fn closable(&self, _: &App) -> bool {
        false
    }

    fn zoomable(&self, _: &App) -> Option<PanelControl> {
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

pub fn settings_pages(node: &Node, cmd_tx: &mpsc::Sender<Cmd>) -> Vec<SettingPage> {
    node.collection()
        .iter()
        .map(|page| {
            let title = page.label_or_id();
            let mut setting_page = SettingPage::new(title).resettable(false);
            let groups = if page.items.iter().any(|g| !g.items.is_empty()) {
                page.items.clone()
            } else {
                vec![Item {
                    label: Some(page.label_or_id()),
                    items: page.items.clone(),
                    ..Item::default()
                }]
            };
            for group in groups {
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
    if n == 0 {
        6
    } else {
        n.clamp(1, 12)
    }
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
    use serde_json::json;

    #[test]
    fn iso_dates_round_trip() {
        let d = parse_iso_date("2026-09-02").unwrap();
        assert_eq!(format_iso_date(d), "2026-09-02");
        assert!(parse_iso_date("2026/09/02").is_some());
        assert!(parse_iso_date("nope").is_none());
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
            chart_points(&node),
            vec![("A".into(), 1.0), ("B".into(), 2.5)]
        );
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
}
