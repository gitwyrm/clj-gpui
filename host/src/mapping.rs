//! Mapping from clj-gpui JSON node fields onto GPUI Kit 0.6 types.

use crate::catalog;
use crate::protocol::{Node, StyledKeys};
use gpui::{Axis, FontWeight, Hsla, StyleRefinement, Styled, div, px};
use gpui_component::{
    Colorize as _, Disableable as _, IconName, Placement, Selectable as _, Side, Sizable as _,
    Size,
    button::{Button, ButtonVariants, ToggleVariant},
    group_box::GroupBoxVariant,
    shimmer::ShimmerStyle,
    slider::SliderScale,
    tab::TabVariant,
    tag::TagVariant,
};
use gpui_kit as gpui;
use gpui_kit::component as gpui_component;
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

/// Kit `Styled` visual keys: gap, padding, type, colors, alignment.
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
    el
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
        el = el.flex_1().min_w_0().min_h_0();
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
        || style.strikethrough
        || style.shadow
        || style.align.is_some()
        || style.justify.is_some()
        || style.width.is_some()
        || style.height.is_some()
        || style.size.is_some()
        || style.flex.is_some()
}

/// Overlay `over` onto `base` for the Styled vocabulary only.
/// Set keys on `over` win; omitted keys keep `base`. Bools are or-merged
/// because the wire cannot distinguish omitted `false` from explicit.
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
        strikethrough: over.strikethrough || base.strikethrough,
        shadow: over.shadow || base.shadow,
        align: over.align.clone().or_else(|| base.align.clone()),
        justify: over.justify.clone().or_else(|| base.justify.clone()),
        width: over.width.or(base.width),
        height: over.height.or(base.height),
        size: over.size.or(base.size),
        flex: over.flex.or(base.flex),
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

/// Kit `MessageScroller::with_jump_button_renderer` chrome (variant, size, icon, label, tooltip).
pub fn apply_jump_button_renderer(mut button: Button, node: &Node) -> Button {
    let chrome = button_chrome(
        node.variant.as_deref(),
        node.primary,
        node.outline,
        node.selected,
        node.control_size.as_deref(),
    );
    button = apply_named_button_variant(button, chrome.variant);
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
    if let Some(label) = jump_button_visible_label(node) {
        button = button.label(label.to_string());
    }
    if let Some(tooltip) = node.tooltip.clone().filter(|s| !s.is_empty()) {
        button = button.tooltip(tooltip);
    }
    button
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

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
