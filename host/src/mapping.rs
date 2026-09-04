//! Mapping from clj-gpui JSON node fields onto GPUI Kit 0.6 types.

use crate::catalog;
use gpui::Axis;
use gpui_component::{
    IconName, Placement, Side, Size, button::ToggleVariant, group_box::GroupBoxVariant,
    slider::SliderScale, tab::TabVariant, tag::TagVariant,
};
use gpui_kit as gpui;
use gpui_kit::component as gpui_component;

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
}
