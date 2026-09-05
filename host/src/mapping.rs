//! Mapping from clj-gpui JSON node fields onto GPUI Kit 0.6 types.

use crate::catalog;
use crate::protocol::{ButtonCustomVariantSpec, Node, StyledKeys};
use chrono::Weekday;
use gpui::{
    AnyElement, App, Axis, FontWeight, Hsla, IntoElement, Keystroke, Role, StyleRefinement, Styled,
    div, px,
};
use gpui_component::{
    Colorize as _, Disableable as _, FocusableExt as _, Icon, IconName, Placement, RoleOverride,
    Selectable as _, Side, Sizable, Size,
    alert::Alert,
    badge::Badge,
    button::{
        Button, ButtonCustomVariant, ButtonRounded, ButtonVariants, Toggle, ToggleVariant,
        ToggleVariants as _,
    },
    checkbox::Checkbox,
    dialog::DialogButtonProps,
    group_box::GroupBoxVariant,
    input::{Editor, Input, InputContentType, NumberInput, OtpInput, Textarea},
    kbd::Kbd,
    label::{HighlightsMatch, Label},
    list::{List, ListDelegate},
    progress::Progress,
    shimmer::ShimmerStyle,
    skeleton::Skeleton,
    slider::SliderScale,
    spinner::Spinner,
    switch::Switch,
    tab::TabVariant,
    table::{DataTable, TableDelegate},
    tag::TagVariant,
};
use gpui_kit as gpui;
use gpui_kit::component as gpui_component;
use serde_json::Value;
use std::ops::Range;
use std::time::Duration;

pub fn parse_scale(value: Option<&str>) -> Size {
    match value.map(catalog::normalize) {
        Some(name) => match name.as_str() {
            "xs" | "xsmall" | "x small" => Size::XSmall,
            "sm" | "small" => Size::Small,
            "lg" | "large" => Size::Large,
            _ => Size::Medium,
        },
        None => Size::Medium,
    }
}

/// Named control size without the Medium default `parse_scale(None)` uses.
///
/// `AttachmentMedia` must keep `size: None` so `layout()` can inherit the
/// parent `Attachment` size. Call this instead of `parse_scale` when the
/// wire field is optional.
pub fn parse_named_size(value: Option<&str>) -> Option<Size> {
    value
        .filter(|name| !name.is_empty())
        .map(|name| parse_scale(Some(name)))
}

/// DataTable row height. Pixel `:row-height` is Kit `Size::Size`
/// (`table_row_height`). Named `:control-size` is Kit `Sizable`. Omitted
/// is Kit Medium (32px), the same default `DataTable` already used.
pub fn table_row_size(node: &Node) -> Size {
    if let Some(height) = node.row_height.filter(|h| h.is_finite() && *h > 0.0) {
        Size::Size(px(height))
    } else {
        parse_scale(node.control_size.as_deref())
    }
}

/// Kit `TableState` flags. Omitted wires restore Kit defaults on every
/// tree (`true`, except `cell_selectable` which is false).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableStateFlags {
    pub cell_selectable: bool,
    pub row_header: bool,
    pub sortable: bool,
    pub col_movable: bool,
    pub col_resizable: bool,
    pub col_fixed: bool,
    pub loop_selection: bool,
    pub row_selectable: bool,
    pub col_selectable: bool,
}

/// Declarative TableState flags for one Clojure tree. Omitted is not
/// "leave the retained value"; it is the Kit default.
pub fn table_state_flags(node: &Node) -> TableStateFlags {
    TableStateFlags {
        cell_selectable: node.cell_selectable.unwrap_or(false),
        row_header: node.row_header.unwrap_or(true),
        sortable: node.sortable.unwrap_or(true),
        col_movable: node.col_movable.unwrap_or(true),
        col_resizable: node.col_resizable.unwrap_or(true),
        col_fixed: node.col_fixed.unwrap_or(true),
        loop_selection: node.loop_selection.unwrap_or(true),
        row_selectable: node.row_selectable.unwrap_or(true),
        col_selectable: node.col_selectable.unwrap_or(true),
    }
}

/// Kit `DataTable` stripe / bordered / scrollbar. Omitted keeps Kit
/// (stripe false, bordered true, both scrollbars true).
pub fn apply_data_table_chrome<D: TableDelegate>(
    mut table: DataTable<D>,
    node: &Node,
) -> DataTable<D> {
    if let Some(stripe) = node.stripe {
        table = table.stripe(stripe);
    }
    if let Some(bordered) = node.bordered {
        table = table.bordered(bordered);
    }
    if let Some(visible) = node.scrollbar {
        table = table.scrollbar_visible(visible, visible);
    }
    table
}

/// Kit `List` scrollbar, search placeholder, and `Sizable`. Selectable
/// is on `ListState`.
pub fn apply_list_chrome<D: ListDelegate + 'static>(mut list: List<D>, node: &Node) -> List<D> {
    if let Some(visible) = node.scrollbar {
        list = list.scrollbar_visible(visible);
    }
    if let Some(placeholder) = node.search_placeholder.as_deref().filter(|s| !s.is_empty()) {
        list = list.search_placeholder(placeholder.to_string());
    }
    if node.control_size.is_some() {
        list = list.with_size(parse_scale(node.control_size.as_deref()));
    }
    list
}

pub fn parse_hsla(value: &str) -> Option<Hsla> {
    let value = value.trim();
    Hsla::parse_hex(value).ok().or_else(|| {
        if value.starts_with('#') {
            None
        } else {
            Hsla::parse_hex(&format!("#{value}")).ok()
        }
    })
}

/// Kit `Styled` visual keys: gap, padding, type, colors, alignment,
/// wrap/clip (`truncate`, `whitespace`, `text-overflow`, `overflow`).
/// Not box geometry (`:width` / `:height` / `:size` / `:flex`).
pub fn apply_visual_style<E: Styled>(mut el: E, node: &Node) -> E {
    if let Some(gap) = node.gap {
        el = el.gap(px(gap));
    }
    if let Some(padding) = node.padding {
        el = el.p(px(padding));
    }
    if let Some(font_size) = node.font_size {
        el = el.text_size(px(font_size));
    }
    if let Some(family) = &node.font_family {
        el = el.font_family(family.clone());
    }
    if let Some(weight) = &node.font_weight {
        el = match weight.as_str() {
            "thin" => el.font_weight(FontWeight::THIN),
            "extralight" | "extra-light" | "ultralight" => el.font_weight(FontWeight::EXTRA_LIGHT),
            "bold" => el.font_weight(FontWeight::BOLD),
            "semibold" | "semi-bold" => el.font_weight(FontWeight::SEMIBOLD),
            "medium" => el.font_weight(FontWeight::MEDIUM),
            "light" => el.font_weight(FontWeight::LIGHT),
            _ => el.font_weight(FontWeight::NORMAL),
        };
    }
    if let Some(color) = node.color.as_deref().and_then(parse_hsla) {
        el = el.text_color(color);
    }
    if let Some(bg) = node.bg.as_deref().and_then(parse_hsla) {
        el = el.bg(bg);
    }
    if let Some(border) = node.border.as_deref().and_then(parse_hsla) {
        el = el.border_1().border_color(border);
    }
    if let Some(border) = node.border_bottom.as_deref().and_then(parse_hsla) {
        el = el.border_b_1().border_color(border);
    }
    if node.strikethrough {
        el = el.line_through();
    }
    if node.shadow {
        el = el.shadow_lg();
    }
    match node.align.as_deref() {
        Some("stretch") => el = el.items_stretch(),
        Some("center") => el = el.items_center(),
        Some("end") => el = el.items_end(),
        Some("start") => el = el.items_start(),
        _ => {}
    }
    match node.justify.as_deref() {
        Some("center") => el = el.justify_center(),
        Some("end") | Some("right") => el = el.justify_end(),
        Some("between") => el = el.justify_between(),
        _ => {}
    }
    apply_text_overflow(el, node)
}

/// True when `overflow: hidden` or `overflow-hidden: true`.
/// NavStack uses this for a slide; any Styled node may clip with the
/// same keys. Not AvatarGroup `:ellipsis`.
pub fn overflow_clips(overflow: Option<&str>, overflow_hidden: bool) -> bool {
    overflow_hidden || matches!(overflow.map(catalog::normalize).as_deref(), Some("hidden"))
}

/// GPUI wrap-off + layout ellipsis (`whitespace_nowrap`, `text_ellipsis*`,
/// `truncate`, `line_clamp`, `overflow_hidden`). A character-count `…`
/// suffix is not this.
pub fn apply_text_overflow<E: Styled>(mut el: E, node: &Node) -> E {
    let truncate = node.truncate;
    if overflow_clips(node.overflow.as_deref(), node.overflow_hidden) || truncate {
        el = el.overflow_hidden();
    }
    match node
        .whitespace
        .as_deref()
        .map(catalog::normalize)
        .as_deref()
    {
        Some("nowrap") => el = el.whitespace_nowrap(),
        Some("normal") => el = el.whitespace_normal(),
        _ if truncate => el = el.whitespace_nowrap(),
        _ => {}
    }
    match node
        .text_overflow
        .as_deref()
        .map(catalog::normalize)
        .as_deref()
    {
        Some("ellipsis") | Some("end") => el = el.text_ellipsis(),
        Some("ellipsis start") | Some("start") => el = el.text_ellipsis_start(),
        Some("ellipsis middle") | Some("middle") => el = el.text_ellipsis_middle(),
        _ if truncate => el = el.text_ellipsis(),
        _ => {}
    }
    if let Some(lines) = node.line_clamp {
        if lines.is_finite() && lines >= 1.0 {
            el = el.line_clamp(lines as usize);
        }
    }
    if text_needs_min_w_0(node) {
        el = el.min_w_0();
    }
    el
}

/// Flex children default to a content min-size. Nowrap / ellipsis need
/// `min_w_0` so a StatusBar region can shrink the text instead of wrapping.
pub fn text_needs_min_w_0(node: &Node) -> bool {
    node.truncate
        || matches!(
            node.whitespace
                .as_deref()
                .map(catalog::normalize)
                .as_deref(),
            Some("nowrap")
        )
        || node
            .text_overflow
            .as_deref()
            .is_some_and(|value| !catalog::normalize(value).is_empty())
}

/// Truncate / nowrap / ellipsis / line-clamp keep intrinsic line height.
/// `flex 1` still applies `min_w_0`, but not `min_h_0`: that plus
/// `overflow_hidden` from truncate collapses an auto-height parent to 0
/// (empty clip box). Overflow-hidden without text clip still shrinks.
pub fn text_keeps_line_height(node: &Node) -> bool {
    text_needs_min_w_0(node)
        || node
            .line_clamp
            .is_some_and(|lines| lines.is_finite() && lines >= 1.0)
}

/// Kit 0.6 `Label::render` replaces the painted string with this glyph.
const MASKED_GLYPH: &str = "•";

struct KitLabelSpec {
    text: String,
    secondary: Option<String>,
    masked: bool,
    highlights: Option<HighlightsMatch>,
}

/// Constructor args for Kit `Label`.
///
/// Kit 0.6 `Label::render` masks *after* measuring highlight ranges on the
/// original UTF-8, then feeds those byte offsets to `StyledText` on a string
/// of U+2022 glyphs. That trips char-boundary assertions for ASCII, `café`,
/// emoji, and mixed CJK. When `:masked`, fold secondary into the main text
/// (same bullet count as Kit `full_text`) and skip highlights.
fn kit_label_spec(node: &Node) -> KitLabelSpec {
    let text = node.text.clone().unwrap_or_default();
    let secondary = node
        .secondary
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if node.masked {
        let text = match secondary {
            Some(s) => format!("{text} {s}"),
            None => text,
        };
        return KitLabelSpec {
            text,
            secondary: None,
            masked: true,
            highlights: None,
        };
    }
    let highlights = node
        .highlights
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|text| {
            match node
                .highlights_match
                .as_deref()
                .map(catalog::normalize)
                .as_deref()
            {
                Some("prefix") => HighlightsMatch::Prefix(text.to_string().into()),
                _ => HighlightsMatch::Full(text.to_string().into()),
            }
        });
    KitLabelSpec {
        text,
        secondary,
        masked: false,
        highlights,
    }
}

/// Kit `Label`: main text plus optional secondary, mask, and highlights.
pub fn kit_label(node: &Node) -> Label {
    let spec = kit_label_spec(node);
    let mut label = Label::new(spec.text);
    if let Some(secondary) = spec.secondary {
        label = label.secondary(secondary);
    }
    if spec.masked {
        label = label.masked(true);
    }
    if let Some(matched) = spec.highlights {
        label = label.highlights(matched);
    }
    label
}

/// Clojure box geometry (`:width` / `:height` / `:size` / `:flex`).
pub fn apply_box_style<E: Styled>(mut el: E, node: &Node) -> E {
    if let Some(width) = node.width {
        el = el.w(px(width));
    }
    if let Some(height) = node.height {
        el = el.h(px(height));
    }
    if let Some(size) = node.size {
        el = el.size(px(size));
    }
    if node.flex.unwrap_or(0.0) >= 1.0 {
        el = el.flex_1().min_w_0();
        if !text_keeps_line_height(node) {
            el = el.min_h_0();
        }
    }
    el
}

/// Visual + box surface used by first-class Kit nodes (including chat).
/// Theme fallback that reads `cx.theme()` stays in `renderer`.
pub fn apply_styled<E: Styled>(el: E, node: &Node) -> E {
    apply_box_style(apply_visual_style(el, node), node)
}

/// True when `style` carries any key `apply_styled` reads.
pub fn has_styled_keys(style: &StyledKeys) -> bool {
    style.gap.is_some()
        || style.padding.is_some()
        || style.font_size.is_some()
        || style.font_family.is_some()
        || style.font_weight.is_some()
        || style.color.is_some()
        || style.bg.is_some()
        || style.border.is_some()
        || style.border_bottom.is_some()
        || style.strikethrough.is_some()
        || style.shadow.is_some()
        || style.align.is_some()
        || style.justify.is_some()
        || style.width.is_some()
        || style.height.is_some()
        || style.size.is_some()
        || style.flex.is_some()
        || style.truncate.is_some()
        || style.whitespace.is_some()
        || style.text_overflow.is_some()
        || style.line_clamp.is_some()
        || style.overflow.is_some()
        || style.overflow_hidden.is_some()
}

/// Overlay `over` onto `base` for the Styled vocabulary only.
/// Set keys on `over` win; omitted keys keep `base`. Recipe booleans are
/// `Option<bool>` so an explicit `false` can disable a base `true`.
pub fn overlay_styled(base: &StyledKeys, over: &StyledKeys) -> StyledKeys {
    StyledKeys {
        gap: over.gap.or(base.gap),
        padding: over.padding.or(base.padding),
        font_size: over.font_size.or(base.font_size),
        font_family: over
            .font_family
            .clone()
            .or_else(|| base.font_family.clone()),
        font_weight: over
            .font_weight
            .clone()
            .or_else(|| base.font_weight.clone()),
        color: over.color.clone().or_else(|| base.color.clone()),
        bg: over.bg.clone().or_else(|| base.bg.clone()),
        border: over.border.clone().or_else(|| base.border.clone()),
        border_bottom: over
            .border_bottom
            .clone()
            .or_else(|| base.border_bottom.clone()),
        strikethrough: over.strikethrough.or(base.strikethrough),
        shadow: over.shadow.or(base.shadow),
        align: over.align.clone().or_else(|| base.align.clone()),
        justify: over.justify.clone().or_else(|| base.justify.clone()),
        width: over.width.or(base.width),
        height: over.height.or(base.height),
        size: over.size.or(base.size),
        flex: over.flex.or(base.flex),
        truncate: over.truncate.or(base.truncate),
        whitespace: over.whitespace.clone().or_else(|| base.whitespace.clone()),
        text_overflow: over
            .text_overflow
            .clone()
            .or_else(|| base.text_overflow.clone()),
        line_clamp: over.line_clamp.or(base.line_clamp),
        overflow: over.overflow.clone().or_else(|| base.overflow.clone()),
        overflow_hidden: over.overflow_hidden.or(base.overflow_hidden),
    }
}

pub fn apply_styled_keys<E: Styled>(el: E, style: &StyledKeys) -> E {
    apply_styled(el, &style.to_node())
}

/// Build a `StyleRefinement` from a nested Clojure style map.
pub fn style_refinement(node: Option<&Node>) -> Option<StyleRefinement> {
    let node = node?;
    let mut dummy = apply_styled(div(), node);
    Some(std::mem::take(dummy.style()))
}

/// Kit `ShimmerStyle` from the existing shimmer option vocabulary.
pub fn shimmer_style(node: Option<&Node>) -> Option<ShimmerStyle> {
    let node = node?;
    let mut style = ShimmerStyle::new();
    if let Some(secs) = node.duration.filter(|n| n.is_finite() && *n >= 0.0) {
        style = style.duration(Duration::from_secs_f32(secs));
    }
    if let Some(color) = node.highlight_color.as_deref().and_then(parse_hsla) {
        style = style.highlight_color(color);
    }
    if let Some(pixels) = node.spread_px.filter(|n| n.is_finite()) {
        style = style.spread(px(pixels));
    } else if let Some(fraction) = node.spread.filter(|n| n.is_finite()) {
        style = style.spread(fraction);
    }
    if node.reverse {
        style = style.reverse(true);
    }
    if node.once {
        style = style.once(true);
    }
    Some(style)
}

/// Visible / accessible name for Kit `Button::label`.
///
/// Clojure `:label` on `:jump-button-renderer` is rewritten to wire `text`.
/// `jump-button-label` on the scroller is Kit's tooltip only.
pub fn jump_button_visible_label(node: &Node) -> Option<&str> {
    node.text.as_deref().filter(|s| !s.is_empty())
}

/// Kit `ButtonRounded` from a named keyword or a pixel number.
pub fn parse_button_rounded(value: Option<&Value>) -> Option<ButtonRounded> {
    let value = value?;
    if let Some(n) = value.as_f64() {
        if n.is_finite() && n >= 0.0 {
            return Some(ButtonRounded::Size(px(n as f32)));
        }
        return None;
    }
    match value.as_str().map(catalog::normalize)?.as_str() {
        "none" => Some(ButtonRounded::None),
        "sm" | "small" => Some(ButtonRounded::Small),
        "md" | "medium" => Some(ButtonRounded::Medium),
        "lg" | "large" => Some(ButtonRounded::Large),
        _ => None,
    }
}

/// Kit `RoleOverride` for `Button::role`. Omitted / unknown is `None`
/// (leave Kit implicit).
pub fn parse_button_role(value: Option<&str>) -> Option<RoleOverride> {
    match value.map(catalog::normalize) {
        Some(name) if matches!(name.as_str(), "none" | "presentation" | "presentational") => {
            Some(RoleOverride::Presentational)
        }
        Some(name) if name == "button" => Some(RoleOverride::Role(Role::Button)),
        Some(name) if name == "link" => Some(RoleOverride::Role(Role::Link)),
        Some(name) if matches!(name.as_str(), "menuitem" | "menu item") => {
            Some(RoleOverride::Role(Role::MenuItem))
        }
        Some(name) if matches!(name.as_str(), "checkbox" | "check box") => {
            Some(RoleOverride::Role(Role::CheckBox))
        }
        Some(name) if matches!(name.as_str(), "radio" | "radio button") => {
            Some(RoleOverride::Role(Role::RadioButton))
        }
        Some(name) if name == "switch" => Some(RoleOverride::Role(Role::Switch)),
        Some(name) if name == "tab" => Some(RoleOverride::Role(Role::Tab)),
        Some(name) if name == "status" => Some(RoleOverride::Role(Role::Status)),
        Some(name) if name == "alert" => Some(RoleOverride::Role(Role::Alert)),
        Some(name) if name == "log" => Some(RoleOverride::Role(Role::Log)),
        _ => None,
    }
}

fn parse_hex(text: Option<&str>) -> Option<Hsla> {
    text.and_then(|s| Hsla::parse_hex(s.trim()).ok())
}

/// Kit `ButtonCustomVariant` from a nested `:custom-variant` map.
/// Omitted keys keep `ButtonCustomVariant::new` theme defaults. Nested
/// so hex `color` cannot become host `text_color`.
pub fn button_custom_variant(spec: &ButtonCustomVariantSpec, cx: &App) -> ButtonCustomVariant {
    let mut style = ButtonCustomVariant::new(cx);
    if let Some(color) = parse_hex(spec.color.as_deref()) {
        style = style.color(color);
    }
    if let Some(color) = parse_hex(spec.foreground.as_deref()) {
        style = style.foreground(color);
    }
    if let Some(color) = parse_hex(spec.hover.as_deref()) {
        style = style.hover(color);
    }
    if let Some(color) = parse_hex(spec.active.as_deref()) {
        style = style.active(color);
    }
    if let Some(shadow) = spec.shadow {
        style = style.shadow(shadow);
    }
    style
}

pub fn apply_custom_button_variant<B: ButtonVariants>(el: B, node: &Node, cx: Option<&App>) -> B {
    match (node.custom_variant.as_ref(), cx) {
        (Some(spec), Some(cx)) => el.custom(button_custom_variant(spec, cx)),
        _ => el,
    }
}

/// ColorPicker `featured_colors`.
///
/// Kit distinguishes omitted (`None` → default swatches) from an
/// explicit empty vector (`Some(vec![])` → no featured swatches).
/// Invalid hex entries are skipped; presence still wins.
pub fn featured_colors(node: &Node) -> Option<Vec<Hsla>> {
    node.featured_colors.as_ref().map(|colors| {
        colors
            .iter()
            .filter_map(|s| parse_hex(Some(s.as_str())))
            .collect()
    })
}

/// Kit `DialogButtonProps` (ok/cancel text and named variants).
pub fn dialog_button_props(node: &Node, show_cancel: bool) -> DialogButtonProps {
    let mut props = DialogButtonProps::default().show_cancel(show_cancel);
    if let Some(text) = node.ok_text.as_deref().filter(|s| !s.is_empty()) {
        props = props.ok_text(text.to_string());
    }
    if let Some(text) = node.cancel_text.as_deref().filter(|s| !s.is_empty()) {
        props = props.cancel_text(text.to_string());
    }
    if let Some(variant) = parse_dialog_button_variant(node.ok_variant.as_deref()) {
        props = props.ok_variant(variant);
    }
    if let Some(variant) = parse_dialog_button_variant(node.cancel_variant.as_deref()) {
        props = props.cancel_variant(variant);
    }
    props
}

fn parse_dialog_button_variant(
    value: Option<&str>,
) -> Option<gpui_component::button::ButtonVariant> {
    use gpui_component::button::ButtonVariant;
    match parse_named_button_variant(value) {
        Some(NamedButtonVariant::Primary) => Some(ButtonVariant::Primary),
        Some(NamedButtonVariant::Secondary) => Some(ButtonVariant::Secondary),
        Some(NamedButtonVariant::Danger) => Some(ButtonVariant::Danger),
        Some(NamedButtonVariant::Warning) => Some(ButtonVariant::Warning),
        Some(NamedButtonVariant::Success) => Some(ButtonVariant::Success),
        Some(NamedButtonVariant::Info) => Some(ButtonVariant::Info),
        Some(NamedButtonVariant::Ghost) => Some(ButtonVariant::Ghost),
        Some(NamedButtonVariant::Link) => Some(ButtonVariant::Link),
        Some(NamedButtonVariant::Text) => Some(ButtonVariant::Text),
        None => None,
    }
}

/// DatePicker `first_day_of_week`. `sun`…`sat` or 0–6 from Sunday.
/// Omitted / unknown is Kit Sunday.
pub fn parse_first_day_of_week(value: Option<&serde_json::Value>) -> Weekday {
    match value {
        Some(serde_json::Value::Number(n)) => match n.as_i64().unwrap_or(0).rem_euclid(7) {
            0 => Weekday::Sun,
            1 => Weekday::Mon,
            2 => Weekday::Tue,
            3 => Weekday::Wed,
            4 => Weekday::Thu,
            5 => Weekday::Fri,
            _ => Weekday::Sat,
        },
        Some(serde_json::Value::String(s)) => match catalog::normalize(s).as_str() {
            "mon" | "monday" | "1" => Weekday::Mon,
            "tue" | "tues" | "tuesday" | "2" => Weekday::Tue,
            "wed" | "wednesday" | "3" => Weekday::Wed,
            "thu" | "thur" | "thurs" | "thursday" | "4" => Weekday::Thu,
            "fri" | "friday" | "5" => Weekday::Fri,
            "sat" | "saturday" | "6" => Weekday::Sat,
            _ => Weekday::Sun,
        },
        _ => Weekday::Sun,
    }
}

/// DatePicker `number_of_months`. Omitted is Kit 1. `0` becomes 1.
pub fn date_number_of_months(node: &Node) -> usize {
    node.number_of_months
        .filter(|n| n.is_finite() && *n > 0.0)
        .map(|n| n as usize)
        .unwrap_or(1)
        .max(1)
}

/// Sidebar `collapsible`. Omitted is Kit `Icon`.
pub fn parse_sidebar_collapsible(
    value: Option<&serde_json::Value>,
) -> gpui_component::sidebar::SidebarCollapsible {
    use gpui_component::sidebar::SidebarCollapsible;
    match value {
        Some(serde_json::Value::Bool(false)) => SidebarCollapsible::None,
        Some(serde_json::Value::Bool(true)) => SidebarCollapsible::Icon,
        Some(serde_json::Value::String(s)) => match catalog::normalize(s).as_str() {
            "none" | "false" => SidebarCollapsible::None,
            "offcanvas" | "off-canvas" => SidebarCollapsible::Offcanvas,
            _ => SidebarCollapsible::Icon,
        },
        _ => SidebarCollapsible::Icon,
    }
}

/// Host header chrome is hidden only for Kit icon-collapse
/// (`collapsed && collapsible == Icon`). Offcanvas keeps the title
/// while the panel animates out. `None` ignores `:collapsed`.
pub fn sidebar_icon_collapsed(node: &Node) -> bool {
    use gpui_component::sidebar::SidebarCollapsible;
    node.collapsed
        && matches!(
            parse_sidebar_collapsible(node.collapsible.as_ref()),
            SidebarCollapsible::Icon
        )
}

/// Settings `sidebar_size_range`. `[min, max]` or `{min, max}` pixels.
/// Omitted, reversed, negative, or non-finite leaves Kit `160px..360px`.
pub fn sidebar_size_range(node: &Node) -> Option<Range<gpui::Pixels>> {
    parse_pixel_range(node.sidebar_size_range.as_ref())
}

fn parse_pixel_range(value: Option<&Value>) -> Option<Range<gpui::Pixels>> {
    let (min, max) = match value {
        Some(Value::Array(xs)) if xs.len() >= 2 => {
            (json_number_f32(&xs[0])?, json_number_f32(&xs[1])?)
        }
        Some(Value::Object(map)) => (
            map.get("min").and_then(json_number_f32)?,
            map.get("max").and_then(json_number_f32)?,
        ),
        _ => return None,
    };
    if min.is_finite() && max.is_finite() && min >= 0.0 && max >= min {
        Some(px(min)..px(max))
    } else {
        None
    }
}

fn json_number_f32(value: &Value) -> Option<f32> {
    value.as_f64().map(|n| n as f32)
}

/// Kit `Button` chrome (variant, size, icon, loading, tooltip, a11y, …).
///
/// Does not set `label` — empty text is icon-button mode. Callers that need
/// a default label (`Open`) apply it themselves. `cx` is required for
/// `:custom-variant` theme defaults.
pub fn apply_button_chrome(mut button: Button, node: &Node, cx: Option<&App>) -> Button {
    let chrome = button_chrome(
        node.variant.as_deref(),
        node.primary,
        node.outline,
        node.selected,
        node.control_size.as_deref(),
    );
    button = apply_named_button_variant(button, chrome.variant);
    button = apply_custom_button_variant(button, node, cx);
    if chrome.outline {
        button = button.outline();
    }
    if chrome.selected {
        button = button.selected(true);
    }
    if let Some(size) = chrome.size {
        button = button.with_size(size);
    }
    if node.compact {
        button = button.compact();
    }
    if node.disabled {
        button = button.disabled(true);
    }
    if let Some(icon) = node.icon.as_deref().and_then(parse_icon) {
        button = button.icon(icon);
    }
    if node.loading {
        button = button.loading(true);
    }
    if let Some(icon) = node.loading_icon.as_deref().and_then(parse_icon) {
        button = button.loading_icon(icon);
    }
    if let Some(tooltip) = node.tooltip.as_deref().filter(|s| !s.is_empty()) {
        button = button.tooltip(tooltip.to_string());
    }
    if let Some(rounded) = parse_button_rounded(node.rounded.as_ref()) {
        button = button.rounded(rounded);
    }
    if node.dropdown_caret {
        button = button.dropdown_caret(true);
    }
    if let Some(toggled) = node.toggled {
        button = button.toggled(toggled);
    }
    if let Some(label) = node
        .accessibility_label
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        button = button.accessibility_label(label.to_string());
    }
    if let Some(id) = node.id.as_deref().filter(|s| !s.is_empty()) {
        button = button.accessibility_id(id.to_string());
    }
    if let Some(role) = parse_button_role(node.role.as_deref()) {
        button = button.role(role);
    }
    if let Some(index) = node.tab_index.filter(|n| n.is_finite()) {
        button = button.tab_index(index as isize);
    }
    if let Some(stop) = node.tab_stop {
        button = button.tab_stop(stop);
    }
    button
}

/// Kit `MessageScroller::with_jump_button_renderer` chrome (variant, size, icon, label, tooltip).
pub fn apply_jump_button_renderer(button: Button, node: &Node, cx: Option<&App>) -> Button {
    let button = apply_button_chrome(button, node, cx);
    match jump_button_visible_label(node) {
        Some(label) => button.label(label.to_string()),
        None => button,
    }
}

fn kit_tooltip(node: &Node) -> Option<String> {
    node.tooltip
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn kit_accessibility_label(node: &Node) -> Option<String> {
    node.accessibility_label
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Kit `Switch` chrome (size, disabled, native tooltip, a11y). Not `.color()`
/// (checked-track fill would collide with host text `:color`).
pub fn apply_switch_chrome(mut el: Switch, node: &Node) -> Switch {
    if node.disabled {
        el = el.disabled(true);
    }
    el = el.with_size(parse_scale(node.control_size.as_deref()));
    if let Some(tooltip) = kit_tooltip(node) {
        el = el.tooltip(tooltip);
    }
    if let Some(label) = kit_accessibility_label(node) {
        el = el.accessibility_label(label);
    }
    el
}

/// Kit `Checkbox` chrome (size, disabled, native tooltip, a11y, role, tab).
/// Not the clj-gpui `:shape :circle` extra.
pub fn apply_checkbox_kit_chrome(mut checkbox: Checkbox, node: &Node) -> Checkbox {
    if node.disabled {
        checkbox = checkbox.disabled(true);
    }
    checkbox = checkbox.with_size(parse_scale(node.control_size.as_deref()));
    if let Some(tooltip) = kit_tooltip(node) {
        checkbox = checkbox.tooltip(tooltip);
    }
    if let Some(label) = kit_accessibility_label(node) {
        checkbox = checkbox.accessibility_label(label);
    }
    if let Some(role) = parse_button_role(node.role.as_deref()) {
        checkbox = checkbox.role(role);
    }
    if let Some(index) = node.tab_index.filter(|n| n.is_finite()) {
        checkbox = checkbox.tab_index(index as isize);
    }
    if let Some(stop) = node.tab_stop {
        checkbox = checkbox.tab_stop(stop);
    }
    checkbox
}

/// Kit `Toggle` chrome (size, disabled, variant, native tooltip).
pub fn apply_toggle_chrome(mut el: Toggle, node: &Node) -> Toggle {
    el = el.with_variant(parse_toggle_variant(node.variant.as_deref()));
    if node.disabled {
        el = el.disabled(true);
    }
    el = el.with_size(parse_scale(node.control_size.as_deref()));
    if let Some(tooltip) = kit_tooltip(node) {
        el = el.tooltip(tooltip);
    }
    el
}

/// Kit `Alert` chrome (title, size, icon, banner, visible). `on_close` stays
/// at the call site (needs a host callback).
pub fn apply_alert_chrome(mut alert: Alert, node: &Node) -> Alert {
    if let Some(title) = node.title.clone() {
        alert = alert.title(title);
    }
    alert = alert.with_size(parse_scale(node.control_size.as_deref()));
    if let Some(icon) = node.icon.as_deref().and_then(parse_icon) {
        alert = alert.icon(icon);
    }
    if node.banner {
        alert = alert.banner();
    }
    if let Some(visible) = node.visible {
        alert = alert.visible(visible);
    }
    alert
}

/// Kit `Progress` bar chrome (value, loading, size, color, a11y).
pub fn apply_progress_chrome(mut progress: Progress, node: &Node) -> Progress {
    let value = node.number_value().unwrap_or(0.0).clamp(0.0, 100.0);
    progress = progress
        .value(value)
        .loading(node.loading)
        .with_size(parse_scale(node.control_size.as_deref()));
    if let Some(color) = parse_hex(node.color.as_deref()) {
        progress = progress.color(color);
    }
    if let Some(label) = node
        .accessibility_label
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        progress = progress.accessibility_label(label.to_string());
    }
    progress
}

/// Kit `Badge` chrome (icon / dot / count, max, color, size).
pub fn apply_badge_chrome(mut badge: Badge, node: &Node) -> Badge {
    if let Some(icon) = node.icon.as_deref().and_then(parse_icon) {
        badge = badge.icon(icon);
    } else if node.dot {
        badge = badge.dot();
    } else if let Some(count) = node.count {
        badge = badge.count(count as usize);
    } else if let Some(n) = node.number_value() {
        badge = badge.count(n.max(0.0) as usize);
    }
    if let Some(max) = node.max.filter(|n| n.is_finite() && *n >= 0.0) {
        badge = badge.max(max as usize);
    }
    if let Some(color) = parse_hex(node.color.as_deref()) {
        badge = badge.color(color);
    }
    badge.with_size(parse_scale(node.control_size.as_deref()))
}

/// Badge `:color` is Kit overlay background (`Badge::color`), not GPUI
/// `text_color` on the host wrapper. Clear it so wrapped children do
/// not inherit the badge color. Other layout/visual keys stay.
pub fn badge_host_node(node: &Node) -> Node {
    let mut host = node.clone();
    host.color = None;
    host
}

/// Kit `Skeleton` chrome (`secondary()`).
///
/// Label already owns Node `secondary` as muted trailing text, so
/// Skeleton chrome is `variant: secondary`. Clojure `:secondary true`
/// rewrites to that variant.
pub fn apply_skeleton_chrome(mut skeleton: Skeleton, node: &Node) -> Skeleton {
    if skeleton_secondary(node) {
        skeleton = skeleton.secondary();
    }
    skeleton
}

fn skeleton_secondary(node: &Node) -> bool {
    matches!(
        node.variant.as_deref().map(catalog::normalize).as_deref(),
        Some("secondary")
    )
}

/// Kit `Spinner` chrome (size, icon, color).
pub fn apply_spinner_chrome(mut spinner: Spinner, node: &Node) -> Spinner {
    spinner = spinner.with_size(parse_scale(node.control_size.as_deref()));
    if let Some(icon) = node.icon.as_deref().and_then(parse_icon) {
        spinner = spinner.icon(icon);
    }
    if let Some(color) = parse_hex(node.color.as_deref()) {
        spinner = spinner.color(color);
    }
    spinner
}

/// Kit `Kbd` chrome (`appearance`, `outline`).
pub fn apply_kbd_chrome(mut kbd: Kbd, node: &Node) -> Kbd {
    if let Some(appearance) = node.appearance {
        kbd = kbd.appearance(appearance);
    }
    if node.outline {
        kbd = kbd.outline();
    }
    kbd
}

/// Parse a GPUI keystroke; `None` when the string is not a keystroke.
pub fn parse_keystroke(text: &str) -> Option<Keystroke> {
    Keystroke::parse(text).ok()
}

pub fn parse_axis(value: Option<&str>) -> Axis {
    match value.map(catalog::normalize) {
        Some(name) if name == "vertical" => Axis::Vertical,
        _ => Axis::Horizontal,
    }
}

/// Description lists default to a vertical stack of rows.
///
/// gpui-component's `horizontal()` constructor also uses `columns: 3`, which
/// jams two label/value pairs into a clipped three-column pill. Omitted
/// orientation is vertical; pass `"horizontal"` plus `:columns` for a grid.
pub fn parse_description_axis(value: Option<&str>) -> Axis {
    match value.map(catalog::normalize) {
        Some(name) if name == "horizontal" => Axis::Horizontal,
        _ => Axis::Vertical,
    }
}

/// Virtual lists default to a vertical column of rows.
///
/// `parse_axis` defaults to horizontal (sliders, separators, resizable). A
/// virtual-list without `:orientation` should still look like `ui/list`.
pub fn parse_virtual_list_axis(value: Option<&str>) -> Axis {
    match value.map(catalog::normalize) {
        Some(name) if name == "horizontal" => Axis::Horizontal,
        _ => Axis::Vertical,
    }
}

/// Column count for `description-list` (1–10). Default 1, not the crate's 3.
pub fn parse_columns(value: Option<u32>) -> usize {
    value.map(|n| (n as usize).clamp(1, 10)).unwrap_or(1)
}

/// Per-item column span. `0` / omitted is 1.
pub fn parse_span(value: u32) -> usize {
    (value as usize).max(1)
}

pub fn parse_tab_variant(value: Option<&str>) -> TabVariant {
    match value.map(catalog::normalize) {
        Some(name) => match name.as_str() {
            "outline" => TabVariant::Outline,
            "pill" => TabVariant::Pill,
            "segmented" => TabVariant::Segmented,
            "underline" => TabVariant::Underline,
            _ => TabVariant::Tab,
        },
        None => TabVariant::Tab,
    }
}

pub fn parse_tag_variant(value: Option<&str>) -> TagVariant {
    match value.map(catalog::normalize) {
        Some(name) => match name.as_str() {
            "primary" => TagVariant::Primary,
            "danger" | "error" => TagVariant::Danger,
            "success" => TagVariant::Success,
            "warning" => TagVariant::Warning,
            "info" => TagVariant::Info,
            _ => TagVariant::Secondary,
        },
        None => TagVariant::Secondary,
    }
}

/// HoverCard `Anchor`. Omitted / unknown leaves Kit's `TopCenter`.
pub fn parse_anchor(value: Option<&str>) -> Option<gpui::Anchor> {
    match value.map(catalog::normalize) {
        Some(name) => match name.as_str() {
            "top left" | "topleft" => Some(gpui::Anchor::TopLeft),
            "top right" | "topright" => Some(gpui::Anchor::TopRight),
            "top center" | "topcenter" | "top" => Some(gpui::Anchor::TopCenter),
            "bottom left" | "bottomleft" => Some(gpui::Anchor::BottomLeft),
            "bottom right" | "bottomright" => Some(gpui::Anchor::BottomRight),
            "bottom center" | "bottomcenter" | "bottom" => Some(gpui::Anchor::BottomCenter),
            "left center" | "leftcenter" | "left" => Some(gpui::Anchor::LeftCenter),
            "right center" | "rightcenter" | "right" => Some(gpui::Anchor::RightCenter),
            _ => None,
        },
        None => None,
    }
}

pub fn parse_placement(value: Option<&str>, default: Placement) -> Placement {
    match value.map(catalog::normalize) {
        Some(name) if name == "left" => Placement::Left,
        Some(name) if name == "top" => Placement::Top,
        Some(name) if name == "bottom" => Placement::Bottom,
        Some(name) if name == "right" => Placement::Right,
        _ => default,
    }
}

pub fn parse_side(value: Option<&str>, default: Side) -> Side {
    match value.map(catalog::normalize) {
        Some(name) if name == "right" => Side::Right,
        Some(name) if name == "left" => Side::Left,
        _ => default,
    }
}

pub fn parse_group_variant(value: Option<&str>) -> GroupBoxVariant {
    GroupBoxVariant::from_str(value.unwrap_or("normal"))
}

pub fn parse_toggle_variant(value: Option<&str>) -> ToggleVariant {
    match value.map(catalog::normalize) {
        Some(name) if name == "outline" => ToggleVariant::Outline,
        _ => ToggleVariant::Ghost,
    }
}

/// Slider scale. Omitted / unknown is linear. `log` is an alias of
/// `logarithmic`. The host still refuses log when `min <= 0` so Kit
/// does not assert.
pub fn parse_slider_scale(value: Option<&str>) -> SliderScale {
    match value.map(catalog::normalize) {
        Some(name) if name == "logarithmic" || name == "log" => SliderScale::Logarithmic,
        _ => SliderScale::Linear,
    }
}

/// Kit `ButtonVariants` names. Outline is a separate look, not a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedButtonVariant {
    Primary,
    Secondary,
    Danger,
    Warning,
    Success,
    Info,
    Ghost,
    Link,
    Text,
}

pub fn parse_named_button_variant(value: Option<&str>) -> Option<NamedButtonVariant> {
    match value.map(catalog::normalize) {
        Some(name) => match name.as_str() {
            "primary" => Some(NamedButtonVariant::Primary),
            "secondary" => Some(NamedButtonVariant::Secondary),
            "danger" => Some(NamedButtonVariant::Danger),
            "warning" => Some(NamedButtonVariant::Warning),
            "success" => Some(NamedButtonVariant::Success),
            "info" => Some(NamedButtonVariant::Info),
            "ghost" => Some(NamedButtonVariant::Ghost),
            "link" => Some(NamedButtonVariant::Link),
            "text" => Some(NamedButtonVariant::Text),
            _ => None,
        },
        None => None,
    }
}

pub fn is_outline_look(value: Option<&str>) -> bool {
    matches!(value.map(catalog::normalize).as_deref(), Some("outline"))
}

/// Chrome plan for `Button` / `DropdownButton`. `size` is `None` when
/// `control-size` is omitted so Kit can inherit from an inner Button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonChrome {
    pub variant: Option<NamedButtonVariant>,
    pub outline: bool,
    pub selected: bool,
    pub size: Option<Size>,
}

pub fn button_chrome(
    variant: Option<&str>,
    primary: bool,
    outline: bool,
    selected: bool,
    control_size: Option<&str>,
) -> ButtonChrome {
    let named = parse_named_button_variant(variant);
    let outline_from_variant = is_outline_look(variant);
    let variant = named.or(if !outline_from_variant && primary {
        Some(NamedButtonVariant::Primary)
    } else {
        None
    });
    ButtonChrome {
        variant,
        outline: outline || outline_from_variant,
        selected,
        size: control_size.map(|s| parse_scale(Some(s))),
    }
}

pub fn apply_named_button_variant<B: ButtonVariants>(
    el: B,
    variant: Option<NamedButtonVariant>,
) -> B {
    match variant {
        Some(NamedButtonVariant::Primary) => el.primary(),
        Some(NamedButtonVariant::Secondary) => el.secondary(),
        Some(NamedButtonVariant::Danger) => el.danger(),
        Some(NamedButtonVariant::Warning) => el.warning(),
        Some(NamedButtonVariant::Success) => el.success(),
        Some(NamedButtonVariant::Info) => el.info(),
        Some(NamedButtonVariant::Ghost) => el.ghost(),
        Some(NamedButtonVariant::Link) => el.link(),
        Some(NamedButtonVariant::Text) => el.text(),
        None => el,
    }
}

pub fn parse_icon(name: &str) -> Option<IconName> {
    match catalog::normalize(name).as_str() {
        "a large small" => Some(IconName::ALargeSmall),
        "arrow down" => Some(IconName::ArrowDown),
        "arrow left" => Some(IconName::ArrowLeft),
        "arrow right" => Some(IconName::ArrowRight),
        "arrow up" => Some(IconName::ArrowUp),
        "asterisk" => Some(IconName::Asterisk),
        "bell" => Some(IconName::Bell),
        "book open" => Some(IconName::BookOpen),
        "bot" => Some(IconName::Bot),
        "building 2" | "building2" => Some(IconName::Building2),
        "calendar" => Some(IconName::Calendar),
        "case sensitive" => Some(IconName::CaseSensitive),
        "chart pie" => Some(IconName::ChartPie),
        "check" => Some(IconName::Check),
        "chevron down" => Some(IconName::ChevronDown),
        "chevron left" => Some(IconName::ChevronLeft),
        "chevron right" => Some(IconName::ChevronRight),
        "chevrons up down" => Some(IconName::ChevronsUpDown),
        "chevron up" => Some(IconName::ChevronUp),
        "circle check" => Some(IconName::CircleCheck),
        "circle user" => Some(IconName::CircleUser),
        "circle x" => Some(IconName::CircleX),
        "close" => Some(IconName::Close),
        "copy" => Some(IconName::Copy),
        "dash" => Some(IconName::Dash),
        "delete" => Some(IconName::Delete),
        "ellipsis" => Some(IconName::Ellipsis),
        "ellipsis vertical" => Some(IconName::EllipsisVertical),
        "external link" => Some(IconName::ExternalLink),
        "eye" => Some(IconName::Eye),
        "eye off" => Some(IconName::EyeOff),
        "file" => Some(IconName::File),
        "folder" => Some(IconName::Folder),
        "folder closed" => Some(IconName::FolderClosed),
        "folder open" => Some(IconName::FolderOpen),
        "frame" => Some(IconName::Frame),
        "gallery vertical end" => Some(IconName::GalleryVerticalEnd),
        "github" => Some(IconName::Github),
        "globe" => Some(IconName::Globe),
        "heart" => Some(IconName::Heart),
        "heart off" => Some(IconName::HeartOff),
        "inbox" => Some(IconName::Inbox),
        "info" => Some(IconName::Info),
        "inspector" => Some(IconName::Inspector),
        "layout dashboard" => Some(IconName::LayoutDashboard),
        "loader" => Some(IconName::Loader),
        "loader circle" => Some(IconName::LoaderCircle),
        "map" => Some(IconName::Map),
        "maximize" => Some(IconName::Maximize),
        "menu" => Some(IconName::Menu),
        "minimize" => Some(IconName::Minimize),
        "minus" => Some(IconName::Minus),
        "moon" => Some(IconName::Moon),
        "palette" => Some(IconName::Palette),
        "panel bottom" => Some(IconName::PanelBottom),
        "panel bottom open" => Some(IconName::PanelBottomOpen),
        "panel left" => Some(IconName::PanelLeft),
        "panel left close" => Some(IconName::PanelLeftClose),
        "panel left open" => Some(IconName::PanelLeftOpen),
        "panel right" => Some(IconName::PanelRight),
        "panel right close" => Some(IconName::PanelRightClose),
        "panel right open" => Some(IconName::PanelRightOpen),
        "plus" => Some(IconName::Plus),
        "redo" => Some(IconName::Redo),
        "redo 2" | "redo2" => Some(IconName::Redo2),
        "replace" => Some(IconName::Replace),
        "resize corner" => Some(IconName::ResizeCorner),
        "search" => Some(IconName::Search),
        "settings" => Some(IconName::Settings),
        "settings 2" | "settings2" => Some(IconName::Settings2),
        "sort ascending" => Some(IconName::SortAscending),
        "sort descending" => Some(IconName::SortDescending),
        "square terminal" => Some(IconName::SquareTerminal),
        "star" => Some(IconName::Star),
        "star off" => Some(IconName::StarOff),
        "sun" => Some(IconName::Sun),
        "thumbs down" => Some(IconName::ThumbsDown),
        "thumbs up" => Some(IconName::ThumbsUp),
        "triangle alert" => Some(IconName::TriangleAlert),
        "undo" => Some(IconName::Undo),
        "undo 2" | "undo2" => Some(IconName::Undo2),
        "user" => Some(IconName::User),
        "window close" => Some(IconName::WindowClose),
        "window maximize" => Some(IconName::WindowMaximize),
        "window minimize" => Some(IconName::WindowMinimize),
        "window restore" => Some(IconName::WindowRestore),
        _ => None,
    }
}

/// Kit `InputContentType` from a kebab / space name. `email` is
/// `EmailAddress`; unknown names are omitted (Kit has no content type).
pub fn parse_content_type(value: Option<&str>) -> Option<InputContentType> {
    match value.map(catalog::normalize).as_deref() {
        Some("name") => Some(InputContentType::Name),
        Some("name prefix") | Some("honorific prefix") => Some(InputContentType::NamePrefix),
        Some("given name") | Some("first name") => Some(InputContentType::GivenName),
        Some("middle name") => Some(InputContentType::MiddleName),
        Some("family name") | Some("last name") => Some(InputContentType::FamilyName),
        Some("name suffix") | Some("honorific suffix") => Some(InputContentType::NameSuffix),
        Some("nickname") => Some(InputContentType::Nickname),
        Some("job title") => Some(InputContentType::JobTitle),
        Some("organization name") | Some("organization") => {
            Some(InputContentType::OrganizationName)
        }
        Some("location") => Some(InputContentType::Location),
        Some("full street address") | Some("street address") => {
            Some(InputContentType::FullStreetAddress)
        }
        Some("street address line 1") | Some("address line 1") => {
            Some(InputContentType::StreetAddressLine1)
        }
        Some("street address line 2") | Some("address line 2") => {
            Some(InputContentType::StreetAddressLine2)
        }
        Some("address city") | Some("city") => Some(InputContentType::AddressCity),
        Some("address state") | Some("state") => Some(InputContentType::AddressState),
        Some("address city and state") => Some(InputContentType::AddressCityAndState),
        Some("sublocality") => Some(InputContentType::Sublocality),
        Some("country name") | Some("country") => Some(InputContentType::CountryName),
        Some("postal code") | Some("zip") => Some(InputContentType::PostalCode),
        Some("telephone number") | Some("tel") | Some("phone") => {
            Some(InputContentType::TelephoneNumber)
        }
        Some("email address") | Some("email") => Some(InputContentType::EmailAddress),
        Some("url") => Some(InputContentType::Url),
        Some("credit card number") | Some("cc number") => Some(InputContentType::CreditCardNumber),
        Some("credit card name") | Some("cc name") => Some(InputContentType::CreditCardName),
        Some("credit card given name") => Some(InputContentType::CreditCardGivenName),
        Some("credit card middle name") => Some(InputContentType::CreditCardMiddleName),
        Some("credit card family name") => Some(InputContentType::CreditCardFamilyName),
        Some("credit card security code") | Some("cc csc") => {
            Some(InputContentType::CreditCardSecurityCode)
        }
        Some("credit card expiration") | Some("cc exp") => {
            Some(InputContentType::CreditCardExpiration)
        }
        Some("credit card expiration month") => Some(InputContentType::CreditCardExpirationMonth),
        Some("credit card expiration year") => Some(InputContentType::CreditCardExpirationYear),
        Some("credit card type") => Some(InputContentType::CreditCardType),
        Some("username") => Some(InputContentType::Username),
        Some("password") => Some(InputContentType::Password),
        Some("new password") => Some(InputContentType::NewPassword),
        Some("one time code") | Some("otp") => Some(InputContentType::OneTimeCode),
        Some("shipment tracking number") => Some(InputContentType::ShipmentTrackingNumber),
        Some("flight number") => Some(InputContentType::FlightNumber),
        Some("date time") | Some("datetime") => Some(InputContentType::DateTime),
        Some("birthdate") | Some("bday") => Some(InputContentType::Birthdate),
        Some("birthdate day") => Some(InputContentType::BirthdateDay),
        Some("birthdate month") => Some(InputContentType::BirthdateMonth),
        Some("birthdate year") => Some(InputContentType::BirthdateYear),
        Some("cellular eid") => Some(InputContentType::CellularEid),
        Some("cellular imei") => Some(InputContentType::CellularImei),
        _ => None,
    }
}

/// Prefix / suffix as a string `Label`, or an icon when text is omitted.
/// Nested widgets are not wrapped (Kit `IntoElement`).
pub fn affix_element(text: Option<&str>, icon: Option<&str>) -> Option<AnyElement> {
    if let Some(text) = text.map(str::trim).filter(|s| !s.is_empty()) {
        Some(Label::new(text.to_string()).into_any_element())
    } else {
        parse_icon(icon.unwrap_or_default()).map(|name| Icon::new(name).into_any_element())
    }
}

/// Kit `Input::with_size` argument. Omitted `:control-size` is Medium.
pub fn input_control_size(node: &Node) -> Size {
    parse_scale(node.control_size.as_deref())
}

/// Forward named `:size` / `:control-size` onto a Kit `Sizable` field.
pub fn apply_input_size<T: Sizable>(input: T, node: &Node) -> T {
    input.with_size(input_control_size(node))
}

/// Whether a retained `InputState` should `set_masked` from Clojure.
///
/// While `:mask-toggle` is on, the native eye button may diverge from
/// Clojure `:masked`. Resync when Clojure `:masked` changes, or when the
/// toggle is removed (`true`→`false`) so a stuck native mask cannot
/// outlive the button.
pub fn input_masked_needs_resync(
    last_masked: bool,
    last_mask_toggle: bool,
    masked: bool,
    mask_toggle: bool,
) -> bool {
    last_masked != masked || (last_mask_toggle && !mask_toggle)
}

/// Kit `OtpInput::groups` when Clojure sent `groups`. Omitted leaves Kit 2.
/// `0` is forwarded so Kit `resolved_groups` can clamp to 1.
pub fn otp_groups(node: &Node) -> Option<usize> {
    node.groups.map(|n| n as usize)
}

/// Kit `Input` chrome that is not `Styled` / `InputState`.
pub fn apply_input_chrome(input: Input, node: &Node) -> Input {
    let mut input = apply_input_size(input, node);
    if node.cleanable {
        input = input.cleanable(true);
    }
    if let Some(appearance) = node.appearance {
        input = input.appearance(appearance);
    }
    if let Some(bordered) = node.bordered {
        input = input.bordered(bordered);
    }
    if let Some(focus) = node.focus_ring {
        input = input.focus_bordered(focus);
    }
    if node.readonly {
        input = input.readonly(true);
    }
    if node.disabled {
        input = input.disabled(true);
    }
    if node.mask_toggle {
        input = input.mask_toggle();
    }
    if let Some(content_type) = parse_content_type(node.content_type.as_deref()) {
        input = input.content_type(content_type);
    }
    if let Some(label) = node
        .accessibility_label
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        input = input.aria_label(label.to_string());
    }
    if let Some(id) = node.id.as_deref().filter(|s| !s.is_empty()) {
        input = input.accessibility_id(id.to_string());
    }
    if let Some(prefix) = affix_element(node.prefix.as_deref(), node.icon.as_deref()) {
        input = input.prefix(prefix);
    }
    if let Some(suffix) = affix_element(node.suffix.as_deref(), None) {
        input = input.suffix(suffix);
    }
    input
}

/// Kit `Textarea` chrome. Not `context_menu` (arbitrary native-menu builder).
pub fn apply_textarea_chrome(mut input: Textarea, node: &Node) -> Textarea {
    if let Some(appearance) = node.appearance {
        input = input.appearance(appearance);
    }
    if let Some(bordered) = node.bordered {
        input = input.bordered(bordered);
    }
    if node.readonly {
        input = input.readonly(true);
    }
    if node.disabled {
        input = input.disabled(true);
    }
    if let Some(label) = node
        .accessibility_label
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        input = input.aria_label(label.to_string());
    }
    input
}

/// Kit `Editor` chrome. Not LSP, not `context_menu`.
pub fn apply_editor_chrome(mut editor: Editor, node: &Node) -> Editor {
    if let Some(appearance) = node.appearance {
        editor = editor.appearance(appearance);
    }
    if let Some(bordered) = node.bordered {
        editor = editor.bordered(bordered);
    }
    if node.readonly {
        editor = editor.readonly(true);
    }
    if node.disabled {
        editor = editor.disabled(true);
    }
    if let Some(label) = node
        .accessibility_label
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        editor = editor.aria_label(label.to_string());
    }
    editor
}

/// Kit `NumberInput` chrome. Placeholder stays on the builder (already set).
pub fn apply_number_input_chrome(mut input: NumberInput, node: &Node) -> NumberInput {
    if let Some(appearance) = node.appearance {
        input = input.appearance(appearance);
    }
    if node.disabled {
        input = input.disabled(true);
    }
    if let Some(focus) = node.focus_ring {
        input = input.focus_ring(focus);
    }
    if let Some(prefix) = affix_element(node.prefix.as_deref(), node.icon.as_deref()) {
        input = input.prefix(prefix);
    }
    if let Some(suffix) = affix_element(node.suffix.as_deref(), None) {
        input = input.suffix(suffix);
    }
    input
}

/// Kit `OtpInput` groups + focus ring. Size / disabled stay in the renderer.
pub fn apply_otp_chrome(mut input: OtpInput, node: &Node) -> OtpInput {
    if let Some(groups) = otp_groups(node) {
        input = input.groups(groups);
    }
    if let Some(focus) = node.focus_ring {
        input = input.focus_ring(focus);
    }
    input
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_component::Sizable;
    use serde_json::json;

    #[test]
    fn scale_keywords() {
        assert_eq!(parse_scale(Some("small")), Size::Small);
        assert_eq!(parse_scale(Some("xs")), Size::XSmall);
        assert_eq!(parse_scale(Some("LARGE")), Size::Large);
        assert_eq!(parse_scale(Some("medium")), Size::Medium);
        assert_eq!(parse_scale(None), Size::Medium);
    }

    #[test]
    fn named_size_stays_none_when_omitted() {
        assert_eq!(parse_named_size(None), None);
        assert_eq!(parse_named_size(Some("")), None);
        assert_eq!(parse_named_size(Some("lg")), Some(Size::Large));
        assert_eq!(parse_named_size(Some("small")), Some(Size::Small));
        assert_ne!(parse_named_size(None), Some(parse_scale(None)));
    }

    #[test]
    fn table_row_size_prefers_pixel_height() {
        let pixel = Node {
            row_height: Some(40.0),
            control_size: Some("small".into()),
            ..Node::default()
        };
        assert_eq!(table_row_size(&pixel), Size::Size(px(40.0)));
        let named = Node {
            control_size: Some("small".into()),
            ..Node::default()
        };
        assert_eq!(table_row_size(&named), Size::Small);
        assert_eq!(table_row_size(&Node::default()), Size::Medium);
        let invalid = Node {
            row_height: Some(0.0),
            control_size: Some("large".into()),
            ..Node::default()
        };
        assert_eq!(table_row_size(&invalid), Size::Large);
    }

    #[test]
    fn omitted_table_state_flags_restore_kit_defaults() {
        let disabled = Node {
            sortable: Some(false),
            col_movable: Some(false),
            col_resizable: Some(false),
            col_fixed: Some(false),
            loop_selection: Some(false),
            row_selectable: Some(false),
            col_selectable: Some(false),
            cell_selectable: Some(true),
            row_header: Some(false),
            ..Node::default()
        };
        let flags = table_state_flags(&disabled);
        assert!(!flags.sortable);
        assert!(!flags.col_movable);
        assert!(!flags.col_resizable);
        assert!(!flags.col_fixed);
        assert!(!flags.loop_selection);
        assert!(!flags.row_selectable);
        assert!(!flags.col_selectable);
        assert!(flags.cell_selectable);
        assert!(!flags.row_header);

        let restored = table_state_flags(&Node::default());
        assert!(restored.sortable);
        assert!(restored.col_movable);
        assert!(restored.col_resizable);
        assert!(restored.col_fixed);
        assert!(restored.loop_selection);
        assert!(restored.row_selectable);
        assert!(restored.col_selectable);
        assert!(!restored.cell_selectable);
        assert!(restored.row_header);
    }

    #[test]
    fn axis_keywords() {
        assert_eq!(parse_axis(Some("vertical")), Axis::Vertical);
        assert_eq!(parse_axis(Some("horizontal")), Axis::Horizontal);
        assert_eq!(parse_axis(None), Axis::Horizontal);
    }

    #[test]
    fn description_list_defaults_to_vertical_one_column() {
        assert_eq!(parse_description_axis(None), Axis::Vertical);
        assert_eq!(parse_description_axis(Some("vertical")), Axis::Vertical);
        assert_eq!(parse_description_axis(Some("horizontal")), Axis::Horizontal);
        assert_eq!(parse_columns(None), 1);
        assert_eq!(parse_columns(Some(3)), 3);
        assert_eq!(parse_columns(Some(0)), 1);
        assert_eq!(parse_columns(Some(99)), 10);
        assert_eq!(parse_span(0), 1);
        assert_eq!(parse_span(2), 2);
    }

    #[test]
    fn tab_and_tag_variants() {
        assert_eq!(parse_tab_variant(Some("underline")), TabVariant::Underline);
        assert_eq!(parse_tab_variant(Some("pill")), TabVariant::Pill);
        assert_eq!(parse_tag_variant(Some("danger")), TagVariant::Danger);
        assert_eq!(parse_tag_variant(Some("error")), TagVariant::Danger);
        assert_eq!(parse_tag_variant(Some("success")), TagVariant::Success);
        assert_eq!(
            parse_toggle_variant(Some("outline")),
            ToggleVariant::Outline
        );
        assert_eq!(
            parse_slider_scale(Some("logarithmic")),
            SliderScale::Logarithmic
        );
        assert_eq!(parse_slider_scale(Some("log")), SliderScale::Logarithmic);
        assert_eq!(parse_slider_scale(Some("linear")), SliderScale::Linear);
        assert_eq!(parse_slider_scale(None), SliderScale::Linear);
    }

    #[test]
    fn named_button_variants_cover_kit() {
        assert_eq!(
            parse_named_button_variant(Some("primary")),
            Some(NamedButtonVariant::Primary)
        );
        assert_eq!(
            parse_named_button_variant(Some("secondary")),
            Some(NamedButtonVariant::Secondary)
        );
        assert_eq!(
            parse_named_button_variant(Some("danger")),
            Some(NamedButtonVariant::Danger)
        );
        assert_eq!(
            parse_named_button_variant(Some("warning")),
            Some(NamedButtonVariant::Warning)
        );
        assert_eq!(
            parse_named_button_variant(Some("success")),
            Some(NamedButtonVariant::Success)
        );
        assert_eq!(
            parse_named_button_variant(Some("info")),
            Some(NamedButtonVariant::Info)
        );
        assert_eq!(
            parse_named_button_variant(Some("ghost")),
            Some(NamedButtonVariant::Ghost)
        );
        assert_eq!(
            parse_named_button_variant(Some("link")),
            Some(NamedButtonVariant::Link)
        );
        assert_eq!(
            parse_named_button_variant(Some("text")),
            Some(NamedButtonVariant::Text)
        );
        assert_eq!(parse_named_button_variant(Some("outline")), None);
        assert_eq!(parse_named_button_variant(None), None);
        assert!(is_outline_look(Some("outline")));
        assert!(!is_outline_look(Some("primary")));
    }

    #[test]
    fn button_chrome_omits_size_when_control_size_is_unset() {
        let outer = button_chrome(Some("warning"), false, false, true, None);
        assert_eq!(outer.variant, Some(NamedButtonVariant::Warning));
        assert!(!outer.outline);
        assert!(outer.selected);
        assert_eq!(outer.size, None);

        let inner = button_chrome(None, false, true, true, Some("small"));
        assert_eq!(inner.variant, None);
        assert!(inner.outline);
        assert!(inner.selected);
        assert_eq!(inner.size, Some(Size::Small));

        let primary_flag = button_chrome(None, true, false, false, None);
        assert_eq!(primary_flag.variant, Some(NamedButtonVariant::Primary));

        let outline_variant = button_chrome(Some("outline"), true, false, false, None);
        assert_eq!(outline_variant.variant, None);
        assert!(outline_variant.outline);

        let secondary = button_chrome(Some("secondary"), false, false, false, Some("large"));
        assert_eq!(secondary.variant, Some(NamedButtonVariant::Secondary));
        assert_eq!(secondary.size, Some(Size::Large));
    }

    #[test]
    fn icon_names_from_kebab() {
        assert!(matches!(parse_icon("check"), Some(IconName::Check)));
        assert!(matches!(
            parse_icon("circle-check"),
            Some(IconName::CircleCheck)
        ));
        assert!(matches!(parse_icon("loader"), Some(IconName::Loader)));
        assert!(matches!(parse_icon("star_off"), Some(IconName::StarOff)));
        assert!(parse_icon("not-an-icon").is_none());
    }

    #[test]
    fn input_content_type_from_kebab() {
        assert!(matches!(
            parse_content_type(Some("password")),
            Some(InputContentType::Password)
        ));
        assert!(matches!(
            parse_content_type(Some("email")),
            Some(InputContentType::EmailAddress)
        ));
        assert!(matches!(
            parse_content_type(Some("one-time-code")),
            Some(InputContentType::OneTimeCode)
        ));
        assert!(matches!(
            parse_content_type(Some("tel")),
            Some(InputContentType::TelephoneNumber)
        ));
        assert!(parse_content_type(Some("not-a-type")).is_none());
        assert!(parse_content_type(None).is_none());
        assert!(affix_element(Some("$"), None).is_some());
        assert!(affix_element(None, Some("search")).is_some());
        assert!(affix_element(None, Some("not-an-icon")).is_none());
        assert!(affix_element(Some("  "), None).is_none());
    }

    #[test]
    fn input_chrome_forwards_named_size_to_with_size() {
        struct SizeSink {
            size: Option<Size>,
        }
        impl Sizable for SizeSink {
            fn with_size(mut self, size: impl Into<Size>) -> Self {
                self.size = Some(size.into());
                self
            }
        }
        let small = Node {
            kind: "input".into(),
            control_size: Some("small".into()),
            ..Node::default()
        };
        let large = Node {
            kind: "input".into(),
            control_size: Some("large".into()),
            ..Node::default()
        };
        let omitted = Node {
            kind: "input".into(),
            ..Node::default()
        };
        assert_eq!(
            apply_input_size(SizeSink { size: None }, &small).size,
            Some(Size::Small)
        );
        assert_eq!(
            apply_input_size(SizeSink { size: None }, &large).size,
            Some(Size::Large)
        );
        assert_eq!(
            apply_input_size(SizeSink { size: None }, &omitted).size,
            Some(Size::Medium)
        );
    }

    #[test]
    fn input_masked_resyncs_when_mask_toggle_is_removed() {
        // Native eye button flipped masked, Clojure `:masked` unchanged.
        assert!(!input_masked_needs_resync(false, true, false, true));
        assert!(!input_masked_needs_resync(true, true, true, true));
        // Removing the toggle restores Clojure `:masked` even when equal.
        assert!(input_masked_needs_resync(false, true, false, false));
        assert!(input_masked_needs_resync(true, true, true, false));
        // Clojure `:masked` still wins when it changes while the toggle exists.
        assert!(input_masked_needs_resync(false, true, true, true));
        assert!(input_masked_needs_resync(true, true, false, true));
        // No toggle: only a Clojure `:masked` change resyncs.
        assert!(!input_masked_needs_resync(false, false, false, false));
        assert!(input_masked_needs_resync(false, false, true, false));
        // Adding the toggle does not overwrite native state by itself.
        assert!(!input_masked_needs_resync(false, false, false, true));
    }

    #[test]
    fn otp_groups_zero_is_forwarded_for_kit_clamp() {
        // Kit 0.6 `resolved_groups(length, 0)` is `requested.max(1).min(length.max(1))`.
        assert_eq!(
            otp_groups(&Node {
                groups: Some(0),
                ..Node::default()
            }),
            Some(0)
        );
        assert_eq!(
            otp_groups(&Node {
                groups: Some(3),
                ..Node::default()
            }),
            Some(3)
        );
        assert!(otp_groups(&Node::default()).is_none());
    }

    #[test]
    fn placement_and_side_from_kebab() {
        assert_eq!(
            parse_placement(Some("left"), Placement::Right),
            Placement::Left
        );
        assert_eq!(
            parse_anchor(Some("top-center")),
            Some(gpui::Anchor::TopCenter)
        );
        assert_eq!(
            parse_anchor(Some("bottom_left")),
            Some(gpui::Anchor::BottomLeft)
        );
        assert_eq!(parse_anchor(Some("left")), Some(gpui::Anchor::LeftCenter));
        assert_eq!(parse_anchor(None), None);
        assert_eq!(parse_anchor(Some("not-an-anchor")), None);
        assert_eq!(parse_placement(None, Placement::Right), Placement::Right);
        assert_eq!(parse_side(Some("right"), Side::Left), Side::Right);
        assert_eq!(parse_side(None, Side::Left), Side::Left);
        assert_eq!(parse_virtual_list_axis(None), Axis::Vertical);
        assert_eq!(
            parse_virtual_list_axis(Some("horizontal")),
            Axis::Horizontal
        );
        assert_eq!(parse_virtual_list_axis(Some("vertical")), Axis::Vertical);
    }

    #[test]
    fn jump_button_renderer_label_uses_text_not_scroller_tooltip() {
        let tooltip = Node {
            kind: "message-scroller".into(),
            jump_button_label: Some("Jump tooltip".into()),
            jump_button_renderer: Some(Box::new(Node {
                text: Some("Latest".into()),
                ..Node::default()
            })),
            ..Node::default()
        };
        assert_eq!(tooltip.jump_button_label.as_deref(), Some("Jump tooltip"));
        assert_eq!(
            jump_button_visible_label(tooltip.jump_button_renderer.as_ref().unwrap()),
            Some("Latest")
        );

        let empty = Node {
            text: Some("".into()),
            ..Node::default()
        };
        assert_eq!(jump_button_visible_label(&empty), None);
        assert_eq!(jump_button_visible_label(&Node::default()), None);

        let button = apply_jump_button_renderer(
            Button::new("jump"),
            tooltip.jump_button_renderer.as_ref().unwrap(),
            None,
        );
        let _ = button;
    }

    #[test]
    fn overlay_styled_merges_visual_and_box_keys() {
        let base = StyledKeys {
            padding: Some(8.0),
            color: Some("#eeeeee".into()),
            ..StyledKeys::default()
        };
        let over = StyledKeys {
            padding: Some(4.0),
            bg: Some("#111111".into()),
            ..StyledKeys::default()
        };
        assert!(has_styled_keys(&base));
        assert!(has_styled_keys(&over));
        assert!(!has_styled_keys(&StyledKeys::default()));
        let merged = overlay_styled(&base, &over);
        assert_eq!(merged.padding, Some(4.0));
        assert_eq!(merged.color.as_deref(), Some("#eeeeee"));
        assert_eq!(merged.bg.as_deref(), Some("#111111"));
        assert_eq!(merged.gap, None);

        let base_bools = StyledKeys {
            shadow: Some(true),
            strikethrough: Some(true),
            ..StyledKeys::default()
        };
        let off = StyledKeys {
            shadow: Some(false),
            strikethrough: Some(false),
            ..StyledKeys::default()
        };
        assert!(has_styled_keys(&off));
        let disabled = overlay_styled(&base_bools, &off);
        assert_eq!(disabled.shadow, Some(false));
        assert_eq!(disabled.strikethrough, Some(false));
        assert!(!disabled.to_node().shadow);
        assert!(!disabled.to_node().strikethrough);
        let kept = overlay_styled(&base_bools, &StyledKeys::default());
        assert_eq!(kept.shadow, Some(true));
        assert_eq!(kept.strikethrough, Some(true));
        assert!(kept.to_node().shadow);
        assert!(kept.to_node().strikethrough);
    }

    #[test]
    fn truncate_is_gpui_overflow_nowrap_and_ellipsis() {
        use gpui::{Overflow, TextOverflow, WhiteSpace};

        let truncated = Node {
            truncate: true,
            ..Node::default()
        };
        let mut dummy = apply_styled(div(), &truncated);
        let style = dummy.style();
        assert_eq!(style.overflow.x, Some(Overflow::Hidden));
        assert_eq!(style.overflow.y, Some(Overflow::Hidden));
        assert_eq!(style.text.white_space, Some(WhiteSpace::Nowrap));
        assert!(matches!(
            style.text.text_overflow,
            Some(TextOverflow::Truncate(_))
        ));
        assert!(text_needs_min_w_0(&truncated));
        assert!(text_keeps_line_height(&truncated));

        let flex_fill = Node {
            flex: Some(1.0),
            ..Node::default()
        };
        let mut dummy = apply_styled(div(), &flex_fill);
        assert!(dummy.style().min_size.width.is_some());
        assert!(dummy.style().min_size.height.is_some());
        assert!(!text_keeps_line_height(&flex_fill));

        let flex_truncated = Node {
            flex: Some(1.0),
            truncate: true,
            ..Node::default()
        };
        let mut dummy = apply_styled(div(), &flex_truncated);
        assert!(dummy.style().min_size.width.is_some());
        assert!(
            dummy.style().min_size.height.is_none(),
            "flex 1 + truncate must not min_h_0 or overflow_hidden collapses line height"
        );
        assert!(text_keeps_line_height(&flex_truncated));

        let flex_clamped = Node {
            flex: Some(1.0),
            line_clamp: Some(2.0),
            ..Node::default()
        };
        assert!(text_keeps_line_height(&flex_clamped));
        let mut dummy = apply_styled(div(), &flex_clamped);
        assert!(dummy.style().min_size.height.is_none());

        let middle = Node {
            text_overflow: Some("ellipsis-middle".into()),
            ellipsis: true,
            ..Node::default()
        };
        let mut dummy = apply_styled(div(), &middle);
        assert!(matches!(
            dummy.style().text.text_overflow,
            Some(TextOverflow::TruncateMiddle(_))
        ));
        assert!(!overflow_clips(
            middle.overflow.as_deref(),
            middle.overflow_hidden
        ));

        let hidden = Node {
            overflow: Some("hidden".into()),
            ..Node::default()
        };
        assert!(overflow_clips(
            hidden.overflow.as_deref(),
            hidden.overflow_hidden
        ));
        let mut dummy = apply_styled(div(), &hidden);
        assert_eq!(dummy.style().overflow.x, Some(Overflow::Hidden));
        assert!(dummy.style().text.white_space.is_none());
        assert!(dummy.style().text.text_overflow.is_none());

        let start = Node {
            text_overflow: Some("ellipsis-start".into()),
            whitespace: Some("nowrap".into()),
            ..Node::default()
        };
        let mut dummy = apply_styled(div(), &start);
        assert_eq!(dummy.style().text.white_space, Some(WhiteSpace::Nowrap));
        assert!(matches!(
            dummy.style().text.text_overflow,
            Some(TextOverflow::TruncateStart(_))
        ));

        let clamped = Node {
            line_clamp: Some(2.0),
            ..Node::default()
        };
        let mut dummy = apply_styled(div(), &clamped);
        assert_eq!(dummy.style().text.line_clamp, Some(2));
        assert_eq!(dummy.style().overflow.x, Some(Overflow::Hidden));
    }

    #[test]
    fn avatar_ellipsis_is_not_text_overflow() {
        let avatar = Node {
            ellipsis: true,
            ..Node::default()
        };
        assert!(!overflow_clips(
            avatar.overflow.as_deref(),
            avatar.overflow_hidden
        ));
        let mut dummy = apply_styled(div(), &avatar);
        assert!(dummy.style().overflow.x.is_none());
        assert!(dummy.style().text.text_overflow.is_none());
        assert!(!has_styled_keys(&StyledKeys::default()));
        assert!(has_styled_keys(&StyledKeys {
            truncate: Some(true),
            ..StyledKeys::default()
        }));
        let merged = overlay_styled(
            &StyledKeys {
                truncate: Some(true),
                ..StyledKeys::default()
            },
            &StyledKeys {
                truncate: Some(false),
                text_overflow: Some("ellipsis-middle".into()),
                ..StyledKeys::default()
            },
        );
        assert_eq!(merged.truncate, Some(false));
        assert_eq!(merged.text_overflow.as_deref(), Some("ellipsis-middle"));
        assert!(!merged.to_node().truncate);
        assert_eq!(
            merged.to_node().text_overflow.as_deref(),
            Some("ellipsis-middle")
        );
    }

    #[test]
    fn kit_label_accepts_secondary_mask_and_prefix_highlights() {
        let unmasked = Node {
            text: Some("Hello World".into()),
            secondary: Some("Ada".into()),
            highlights: Some("Hel".into()),
            highlights_match: Some("prefix".into()),
            ..Node::default()
        };
        let spec = kit_label_spec(&unmasked);
        assert_eq!(spec.text, "Hello World");
        assert_eq!(spec.secondary.as_deref(), Some("Ada"));
        assert!(!spec.masked);
        assert!(spec.highlights.is_some());
        let _ = kit_label(&unmasked);
        let _ = kit_label(&Node {
            text: Some("Hi".into()),
            ..Node::default()
        });
    }

    /// Kit 0.6 `Label::highlight_ranges`, copied so the canary stays honest
    /// if we bump the crate. Search runs on the unmasked `full_text`;
    /// `total_length` is whatever `Label::render` then passes in (masked
    /// UTF-8 length when `masked` is set).
    fn kit_0_6_highlight_ranges(
        label: &str,
        secondary: Option<&str>,
        highlights: Option<&str>,
        prefix: bool,
        total_length: usize,
    ) -> Vec<std::ops::Range<usize>> {
        let full = match secondary {
            Some(s) => format!("{label} {s}"),
            None => label.to_string(),
        };
        let mut ranges = Vec::new();
        if secondary.is_some() {
            ranges.push(0..label.len());
            ranges.push(label.len()..total_length);
        }
        if let Some(matched_str) = highlights.filter(|s| !s.is_empty()) {
            let search_lower = matched_str.to_lowercase();
            let full_text_lower = full.to_lowercase();
            if prefix {
                if full_text_lower.starts_with(&search_lower) {
                    ranges.push(0..matched_str.len());
                }
            } else {
                let mut search_start = 0;
                while let Some(pos) = full_text_lower[search_start..].find(&search_lower) {
                    let match_start = search_start + pos;
                    let match_end = match_start + matched_str.len();
                    if match_end <= full.len() {
                        ranges.push(match_start..match_end);
                    }
                    search_start = match_start + 1;
                    while !full.is_char_boundary(search_start) && search_start < full.len() {
                        search_start += 1;
                    }
                    if search_start >= full.len() {
                        break;
                    }
                }
            }
        }
        ranges
    }

    fn kit_0_6_masked_highlight_ranges(
        label: &str,
        secondary: Option<&str>,
        highlights: Option<&str>,
        prefix: bool,
    ) -> (String, Vec<std::ops::Range<usize>>) {
        let full = match secondary {
            Some(s) => format!("{label} {s}"),
            None => label.to_string(),
        };
        let masked = MASKED_GLYPH.repeat(full.chars().count());
        let ranges = kit_0_6_highlight_ranges(label, secondary, highlights, prefix, masked.len());
        (masked, ranges)
    }

    fn paint_like_kit_label_render(spec: &KitLabelSpec) {
        use gpui::{HighlightStyle, StyledText};
        let mut text = match &spec.secondary {
            Some(s) => format!("{} {}", spec.text, s),
            None => spec.text.clone(),
        };
        if spec.masked {
            text = MASKED_GLYPH.repeat(text.chars().count());
        }
        let ranges: Vec<std::ops::Range<usize>> = match &spec.highlights {
            Some(matched) if !spec.masked => kit_0_6_highlight_ranges(
                &spec.text,
                spec.secondary.as_deref(),
                Some(matched.as_str()),
                matched.is_prefix(),
                text.len(),
            ),
            _ => Vec::new(),
        };
        for range in &ranges {
            assert!(
                text.is_char_boundary(range.start) && text.is_char_boundary(range.end),
                "StyledText highlight {range:?} is not a char boundary on {text:?}"
            );
        }
        let _ = StyledText::new(&text).with_highlights(
            ranges
                .into_iter()
                .map(|range| (range, HighlightStyle::default())),
        );
    }

    #[test]
    fn kit_0_6_masked_unicode_secondary_highlights_are_not_char_boundaries() {
        let (masked, ranges) =
            kit_0_6_masked_highlight_ranges("café", Some("世界"), Some("fé"), false);
        assert!(
            ranges.iter().any(|range| {
                !masked.is_char_boundary(range.start) || !masked.is_char_boundary(range.end)
            }),
            "kit 0.6 still measures original-string ranges after swapping in U+2022 glyphs"
        );
    }

    #[test]
    fn kit_label_masked_unicode_secondary_highlights_is_safe_for_styled_text() {
        let node = Node {
            text: Some("café".into()),
            secondary: Some("世界".into()),
            masked: true,
            highlights: Some("fé".into()),
            highlights_match: Some("prefix".into()),
            ..Node::default()
        };
        let spec = kit_label_spec(&node);
        assert!(spec.masked);
        assert!(spec.secondary.is_none());
        assert!(spec.highlights.is_none());
        assert_eq!(spec.text, "café 世界");
        paint_like_kit_label_render(&spec);
        let _ = kit_label(&node);

        let unmasked = Node {
            text: Some("café".into()),
            secondary: Some("世界".into()),
            highlights: Some("fé".into()),
            ..Node::default()
        };
        paint_like_kit_label_render(&kit_label_spec(&unmasked));
        let _ = kit_label(&unmasked);
    }

    #[test]
    fn nested_flex_scroll_still_gets_min_h_0_when_a_truncated_child_keeps_line_height() {
        use gpui::{Overflow, Styled};

        let column = Node {
            kind: "vstack".into(),
            flex: Some(1.0),
            ..Node::default()
        };
        let mut dummy = apply_styled(div(), &column);
        assert!(dummy.style().min_size.height.is_some());
        assert!(!text_keeps_line_height(&column));

        let scroll = Node {
            kind: "scroll".into(),
            flex: Some(1.0),
            ..Node::default()
        };
        let mut dummy = apply_styled(div(), &scroll);
        assert!(dummy.style().min_size.height.is_some());
        assert!(!text_keeps_line_height(&scroll));
        assert_eq!(dummy.style().overflow.x, None);

        let truncated = Node {
            kind: "label".into(),
            flex: Some(1.0),
            truncate: true,
            ..Node::default()
        };
        let mut dummy = apply_styled(div(), &truncated);
        assert!(dummy.style().min_size.width.is_some());
        assert!(dummy.style().min_size.height.is_none());
        assert!(text_keeps_line_height(&truncated));
        assert_eq!(dummy.style().overflow.x, Some(Overflow::Hidden));

        let nested = Node {
            kind: "vstack".into(),
            flex: Some(1.0),
            children: vec![scroll.clone(), truncated],
            ..Node::default()
        };
        assert!(
            !text_keeps_line_height(&nested),
            "a truncated child must not suppress min_h_0 on the flex/scroll parent"
        );
        let mut dummy = apply_styled(div(), &nested);
        assert!(dummy.style().min_size.height.is_some());
    }

    #[test]
    fn button_rounded_from_name_or_pixels() {
        assert!(matches!(
            parse_button_rounded(Some(&json!("none"))),
            Some(ButtonRounded::None)
        ));
        assert!(matches!(
            parse_button_rounded(Some(&json!("small"))),
            Some(ButtonRounded::Small)
        ));
        assert!(matches!(
            parse_button_rounded(Some(&json!(8))),
            Some(ButtonRounded::Size(_))
        ));
        assert!(parse_button_rounded(Some(&json!("nope"))).is_none());
        assert!(parse_button_rounded(None).is_none());
    }

    #[test]
    fn button_role_from_kebab() {
        assert_eq!(
            parse_button_role(Some("presentation")),
            Some(RoleOverride::Presentational)
        );
        assert_eq!(
            parse_button_role(Some("button")),
            Some(RoleOverride::Role(Role::Button))
        );
        assert_eq!(
            parse_button_role(Some("menu-item")),
            Some(RoleOverride::Role(Role::MenuItem))
        );
        assert_eq!(parse_button_role(Some("not-a-role")), None);
        assert_eq!(parse_button_role(None), None);
    }

    #[test]
    fn skeleton_secondary_is_variant_not_label_text() {
        assert!(skeleton_secondary(&Node {
            variant: Some("secondary".into()),
            ..Node::default()
        }));
        assert!(!skeleton_secondary(&Node {
            secondary: Some("Lovelace".into()),
            ..Node::default()
        }));
        assert!(!skeleton_secondary(&Node::default()));
    }

    #[test]
    fn badge_host_node_drops_color_keeps_layout() {
        let node = Node {
            color: Some("#22c55e".into()),
            width: Some(40.0),
            flex: Some(1.0),
            bg: Some("#111111".into()),
            ..Node::default()
        };
        let host = badge_host_node(&node);
        assert!(host.color.is_none());
        assert_eq!(host.width, Some(40.0));
        assert_eq!(host.flex, Some(1.0));
        assert_eq!(host.bg.as_deref(), Some("#111111"));
        assert_eq!(node.color.as_deref(), Some("#22c55e"));
    }

    #[test]
    fn badge_host_style_does_not_inherit_overlay_color() {
        let node = Node {
            color: Some("#22c55e".into()),
            ..Node::default()
        };
        let mut with_color = apply_visual_style(div(), &node);
        assert!(
            with_color.style().text.color.is_some(),
            "Badge :color would otherwise become host text_color"
        );

        let host = badge_host_node(&node);
        let mut without = apply_visual_style(div(), &host);
        assert!(
            without.style().text.color.is_none(),
            "host wrapper must not pass Badge overlay color to wrapped children"
        );
    }

    #[test]
    fn first_day_of_week_parses_names_and_sunday_origin_numbers() {
        assert_eq!(parse_first_day_of_week(Some(&json!("mon"))), Weekday::Mon);
        assert_eq!(
            parse_first_day_of_week(Some(&json!("monday"))),
            Weekday::Mon
        );
        assert_eq!(parse_first_day_of_week(Some(&json!(1))), Weekday::Mon);
        assert_eq!(parse_first_day_of_week(Some(&json!(0))), Weekday::Sun);
        assert_eq!(parse_first_day_of_week(None), Weekday::Sun);
        assert_eq!(parse_first_day_of_week(Some(&json!("nope"))), Weekday::Sun);
    }

    #[test]
    fn sidebar_collapsible_parses_bool_and_named() {
        use gpui_component::sidebar::SidebarCollapsible;
        assert!(matches!(
            parse_sidebar_collapsible(Some(&json!(true))),
            SidebarCollapsible::Icon
        ));
        assert!(matches!(
            parse_sidebar_collapsible(Some(&json!(false))),
            SidebarCollapsible::None
        ));
        assert!(matches!(
            parse_sidebar_collapsible(Some(&json!("offcanvas"))),
            SidebarCollapsible::Offcanvas
        ));
        assert!(matches!(
            parse_sidebar_collapsible(Some(&json!("none"))),
            SidebarCollapsible::None
        ));
        assert!(matches!(
            parse_sidebar_collapsible(None),
            SidebarCollapsible::Icon
        ));
    }

    #[test]
    fn featured_colors_distinguishes_omitted_and_explicit_empty() {
        assert_eq!(featured_colors(&Node::default()), None);

        let empty = Node {
            featured_colors: Some(vec![]),
            ..Node::default()
        };
        assert_eq!(featured_colors(&empty), Some(vec![]));

        let colors = Node {
            featured_colors: Some(vec!["#3366ff".into(), "nope".into()]),
            ..Node::default()
        };
        let parsed = featured_colors(&colors).expect("explicit list");
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn sidebar_icon_collapsed_ignores_collapsed_when_collapsible_is_none() {
        let expanded = Node {
            title: Some("Project".into()),
            ..Node::default()
        };
        assert!(!sidebar_icon_collapsed(&expanded));

        let icon = Node {
            collapsed: true,
            collapsible: Some(json!("icon")),
            title: Some("Project".into()),
            ..Node::default()
        };
        assert!(sidebar_icon_collapsed(&icon));

        let none = Node {
            collapsed: true,
            collapsible: Some(json!("none")),
            title: Some("Project".into()),
            ..Node::default()
        };
        assert!(!sidebar_icon_collapsed(&none));

        let offcanvas = Node {
            collapsed: true,
            collapsible: Some(json!("offcanvas")),
            title: Some("Project".into()),
            ..Node::default()
        };
        assert!(!sidebar_icon_collapsed(&offcanvas));
    }

    #[test]
    fn sidebar_size_range_parses_pair_and_object() {
        let arr = Node {
            sidebar_size_range: Some(json!([160, 360])),
            ..Node::default()
        };
        let range = sidebar_size_range(&arr).expect("array range");
        assert_eq!(range.start, px(160.0));
        assert_eq!(range.end, px(360.0));

        let obj = Node {
            sidebar_size_range: Some(json!({"min": 140, "max": 280})),
            ..Node::default()
        };
        let range = sidebar_size_range(&obj).expect("object range");
        assert_eq!(range.start, px(140.0));
        assert_eq!(range.end, px(280.0));

        assert_eq!(sidebar_size_range(&Node::default()), None);

        let equal = Node {
            sidebar_size_range: Some(json!([200, 200])),
            ..Node::default()
        };
        let range = sidebar_size_range(&equal).expect("equal bounds");
        assert_eq!(range.start, px(200.0));
        assert_eq!(range.end, px(200.0));

        let reversed_arr = Node {
            sidebar_size_range: Some(json!([360, 160])),
            ..Node::default()
        };
        assert_eq!(sidebar_size_range(&reversed_arr), None);

        let reversed_obj = Node {
            sidebar_size_range: Some(json!({"min": 360, "max": 160})),
            ..Node::default()
        };
        assert_eq!(sidebar_size_range(&reversed_obj), None);

        let negative = Node {
            sidebar_size_range: Some(json!([-10, 100])),
            ..Node::default()
        };
        assert_eq!(sidebar_size_range(&negative), None);

        let non_finite = Node {
            sidebar_size_range: Some(json!(["nan", 100])),
            ..Node::default()
        };
        assert_eq!(sidebar_size_range(&non_finite), None);
    }

    #[test]
    fn dialog_button_props_forward_named_variants_only() {
        let node = Node {
            ok_text: Some("Delete".into()),
            cancel_text: Some("Keep".into()),
            ok_variant: Some("danger".into()),
            cancel_variant: Some("ghost".into()),
            ..Node::default()
        };
        let _ = dialog_button_props(&node, true);

        let custom = Node {
            ok_variant: Some("custom".into()),
            ..Node::default()
        };
        let _ = dialog_button_props(&custom, false);
    }

    #[test]
    fn custom_variant_color_does_not_become_host_text_color() {
        let node = Node {
            custom_variant: Some(crate::protocol::ButtonCustomVariantSpec {
                color: Some("#b91c1c".into()),
                foreground: Some("#f8fafc".into()),
                hover: None,
                active: None,
                shadow: Some(true),
            }),
            ..Node::default()
        };
        assert!(node.color.is_none());
        let mut styled = apply_visual_style(div(), &node);
        assert!(
            styled.style().text.color.is_none(),
            "nested ButtonCustomVariant :color must not become host text_color"
        );
    }

    #[test]
    fn tabs_max_width_is_not_host_layout_width() {
        let node = Node {
            kind: "tabs".into(),
            max_width: Some(120.0),
            ..Node::default()
        };
        assert!(node.width.is_none());
        let mut styled = apply_styled(div(), &node);
        assert!(
            styled.style().size.width.is_none(),
            "TabBar :max-width is per-tab label truncation, not apply_styled width"
        );
    }

    #[test]
    fn settings_sidebar_width_is_not_host_layout_width() {
        let node = Node {
            kind: "settings".into(),
            sidebar_width: Some(200.0),
            ..Node::default()
        };
        assert!(node.width.is_none());
        let mut styled = apply_styled(div(), &node);
        assert!(
            styled.style().size.width.is_none(),
            "Settings :sidebar-width is not the host wrapper :width"
        );
    }

    #[test]
    fn description_list_label_width_is_not_host_layout_width() {
        let node = Node {
            kind: "description-list".into(),
            label_width: Some(160.0),
            ..Node::default()
        };
        assert!(node.width.is_none());
        let mut styled = apply_styled(div(), &node);
        assert!(
            styled.style().size.width.is_none(),
            "DescriptionList :label-width is not the host wrapper :width"
        );
    }

    #[test]
    fn date_number_of_months_omitted_is_one() {
        assert_eq!(date_number_of_months(&Node::default()), 1);
        assert_eq!(
            date_number_of_months(&Node {
                number_of_months: Some(2.0),
                ..Node::default()
            }),
            2
        );
        assert_eq!(
            date_number_of_months(&Node {
                number_of_months: Some(0.0),
                ..Node::default()
            }),
            1
        );
    }
}
