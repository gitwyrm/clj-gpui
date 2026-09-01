//! Mapping from clj-gpui JSON node fields onto gpui-component 0.5.1 types.

use crate::catalog;
use gpui::Axis;
use gpui_component::{
    button::ToggleVariant, group_box::GroupBoxVariant, tab::TabVariant, tag::TagVariant, IconName,
    Size,
};

#[cfg(test)]
use gpui_component::slider::SliderScale;

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

pub fn parse_group_variant(value: Option<&str>) -> GroupBoxVariant {
    GroupBoxVariant::from_str(value.unwrap_or("normal"))
}

pub fn parse_toggle_variant(value: Option<&str>) -> ToggleVariant {
    match value.map(catalog::normalize) {
        Some(name) if name == "outline" => ToggleVariant::Outline,
        _ => ToggleVariant::Ghost,
    }
}

/// Logarithmic scale is category C (deferred). Kept for mapping tests.
#[cfg(test)]
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
        "github" => Some(IconName::GitHub),
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
}
