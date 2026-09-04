//! Kit 0.6 Message / Bubble / Attachment / Marker / MessageScroller.
//!
//! All of these except `MessageScrollerState` are `RenderOnce`. Overlay must
//! not import `extra`; this module only depends on `protocol` and `mapping` so
//! both `renderer` and `overlay` can paint the family.

use crate::catalog;
use crate::mapping;
use crate::protocol::{Cmd, Node};
use gpui::{AnyElement, IntoElement, ParentElement, SharedString, Styled, div};
use gpui_component::{
    Disableable as _, Icon, Selectable as _, Sizable as _,
    attachment::{
        Attachment, AttachmentActions, AttachmentContent, AttachmentDescription, AttachmentGroup,
        AttachmentMedia, AttachmentStatus, AttachmentTitle,
    },
    bubble::{
        Bubble, BubbleContent, BubbleGroup, BubbleReactionSide, BubbleReactions, BubbleVariant,
    },
    button::Button,
    marker::{Marker, MarkerContent, MarkerIcon, MarkerLoadingStyle, MarkerVariant},
    message::{
        Message, MessageAlignment, MessageAvatar, MessageContent, MessageFooter, MessageGroup,
        MessageHeader,
    },
    v_flex,
};
use gpui_kit as gpui;
use gpui_kit::component as gpui_component;
use serde_json::Value;
use std::sync::mpsc;

/// Recursion into the rest of the tree (RootView in-window, overlay painter
/// for scroller rows / hover-card / dock).
pub trait NodePainter {
    fn paint_node(&mut self, node: &Node, path: &str) -> AnyElement;
    fn cmd_tx(&self) -> Option<mpsc::Sender<Cmd>>;
}

pub fn is_chat_kind(kind: &str) -> bool {
    matches!(
        kind,
        "message"
            | "message-group"
            | "message-avatar"
            | "message-header"
            | "message-content"
            | "message-footer"
            | "bubble"
            | "bubble-content"
            | "bubble-group"
            | "bubble-reactions"
            | "attachment"
            | "attachment-media"
            | "attachment-media-overlay"
            | "attachment-content"
            | "attachment-title"
            | "attachment-description"
            | "attachment-actions"
            | "attachment-group"
            | "marker"
            | "marker-icon"
            | "marker-content"
            | "message-scroller"
    )
}

pub fn render_any<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> AnyElement {
    match node.kind.as_str() {
        "message" => render_message(p, node, path).into_any_element(),
        "message-group" => render_message_group(p, node, path).into_any_element(),
        "message-avatar" => render_message_avatar(p, node, path).into_any_element(),
        "message-header" => render_message_header(p, node, path).into_any_element(),
        "message-content" => render_message_content(p, node, path).into_any_element(),
        "message-footer" => render_message_footer(p, node, path).into_any_element(),
        "bubble" => render_bubble(p, node, path).into_any_element(),
        "bubble-content" => render_bubble_content(p, node, path).into_any_element(),
        "bubble-group" => render_bubble_group(p, node, path).into_any_element(),
        "bubble-reactions" => render_bubble_reactions(p, node, path).into_any_element(),
        "attachment" => render_attachment(p, node, path).into_any_element(),
        "attachment-media" => render_attachment_media(p, node, path).into_any_element(),
        "attachment-media-overlay" => render_media_overlay_inner(p, node, path),
        "attachment-content" => render_attachment_content(p, node, path).into_any_element(),
        "attachment-title" => render_attachment_title(node).into_any_element(),
        "attachment-description" => render_attachment_description(node).into_any_element(),
        "attachment-actions" => render_attachment_actions(p, node, path).into_any_element(),
        "attachment-group" => render_attachment_group(p, node, path).into_any_element(),
        "marker" => render_marker(p, node, path).into_any_element(),
        "marker-icon" => render_marker_icon(p, node, path).into_any_element(),
        "marker-content" => render_marker_content(p, node, path).into_any_element(),
        "message-scroller" => render_scroller_static(p, node, path),
        _ => p.paint_node(node, path),
    }
}

/// How to update `MessageScrollerState` after Clojure replaces the row list.
///
/// Index-only ids make prepend look like a full replace; stable `:id` on each
/// row is required for append/prepend without `reset` (which re-enables tail
/// follow).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScrollerEdit {
    Leave,
    Reset { count: usize },
    Append(usize),
    Prepend(usize),
}

pub fn scroller_edit(prev: &[String], next: &[String]) -> ScrollerEdit {
    if prev == next {
        ScrollerEdit::Leave
    } else if prev.is_empty() {
        ScrollerEdit::Reset { count: next.len() }
    } else if next.is_empty() {
        ScrollerEdit::Reset { count: 0 }
    } else if next.len() > prev.len() && next[..prev.len()] == prev[..] {
        ScrollerEdit::Append(next.len() - prev.len())
    } else if next.len() > prev.len() && next[next.len() - prev.len()..] == prev[..] {
        ScrollerEdit::Prepend(next.len() - prev.len())
    } else {
        ScrollerEdit::Reset { count: next.len() }
    }
}

pub fn scroller_item_id(node: &Node, index: usize) -> String {
    node.id
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("idx:{index}"))
}

/// Programmatic MessageScroller navigation. Kit `scroll_to_item` /
/// `scroll_to_end` are one-shots: child-list sync cannot jump to an
/// existing row. Omitted leaves native scroll (user drag, jump button).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScrollerScroll {
    End,
    Item(String),
}

/// Replay identity for a programmatic scroll. Structured so a row id
/// that contains `:` cannot collide with a generation that contains `:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrollerScrollToken {
    pub request: ScrollerScroll,
    pub generation: Option<String>,
}

/// Kit call to make once a request is new and the target exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScrollerScrollApply {
    End,
    Item(usize),
}

/// Wire `scroll-generation`: integer or non-empty string, same shape as
/// nav-stack `replace-generation`. Omitted / null is not a replay token.
pub fn scroller_scroll_generation(value: Option<&Value>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => {
            let s = s.trim();
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        }
        Some(Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                Some(i.to_string())
            } else if let Some(u) = n.as_u64() {
                Some(u.to_string())
            } else {
                let f = n.as_f64()?;
                if f.is_finite() && f == f.trunc() {
                    Some((f as i64).to_string())
                } else {
                    None
                }
            }
        }
        Some(_) => None,
    }
}

fn scroll_item_spec(value: Option<&Value>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => {
            let s = s.trim();
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        }
        Some(Value::Number(n)) => {
            if let Some(u) = n.as_u64() {
                Some(u.to_string())
            } else if let Some(i) = n.as_i64() {
                Some(i.to_string())
            } else {
                None
            }
        }
        Some(_) => None,
    }
}

/// `:scroll-to-end true` is Kit `scroll_to_end` and wins over
/// `:scroll-to-item`. Omitted / false and omitted item leave native scroll.
pub fn scroller_scroll_request(
    scroll_to_end: Option<bool>,
    scroll_to_item: Option<&Value>,
) -> Option<ScrollerScroll> {
    if scroll_to_end == Some(true) {
        return Some(ScrollerScroll::End);
    }
    scroll_item_spec(scroll_to_item).map(ScrollerScroll::Item)
}

/// Resolve a `:scroll-to-item` spec against current row ids. A matching
/// id wins; otherwise a 0-based index that is in range.
pub fn scroller_item_index(spec: &str, ids: &[String]) -> Option<usize> {
    if let Some(index) = ids.iter().position(|id| id == spec) {
        return Some(index);
    }
    spec.parse::<usize>()
        .ok()
        .filter(|index| *index < ids.len())
}

/// Identity of a scroll request for replay. Token includes the Clojure
/// spec (id / index string), not the resolved index, so prepend that
/// shifts `m1` from 0→1 does not re-fire Kit (prepend already preserves
/// the native anchor).
pub fn scroller_scroll_token(
    request: Option<&ScrollerScroll>,
    generation: Option<&str>,
) -> Option<ScrollerScrollToken> {
    Some(ScrollerScrollToken {
        request: request?.clone(),
        generation: generation.map(str::to_string),
    })
}

/// Apply when the token is present and differs from the last
/// *successfully applied* request. Omitted token leaves native scroll.
pub fn should_apply_scroller_scroll(
    last: Option<&ScrollerScrollToken>,
    token: Option<&ScrollerScrollToken>,
) -> bool {
    match token {
        Some(token) => last != Some(token),
        None => false,
    }
}

/// Resolve a present request against current row ids. `None` means do
/// not call Kit: omit, or `:scroll-to-item` is not in this list yet.
pub fn scroller_scroll_apply(
    request: Option<&ScrollerScroll>,
    ids: &[String],
) -> Option<ScrollerScrollApply> {
    match request {
        None => None,
        Some(ScrollerScroll::End) => Some(ScrollerScrollApply::End),
        Some(ScrollerScroll::Item(spec)) => {
            scroller_item_index(spec, ids).map(ScrollerScrollApply::Item)
        }
    }
}

/// Plan a Kit scroll for this tree. Unresolved items return `None` so
/// the host does not record a last-applied token; the same request can
/// succeed after append/load. A successful Kit call then binds `token`.
pub fn scroller_scroll_plan(
    last: Option<&ScrollerScrollToken>,
    request: Option<&ScrollerScroll>,
    generation: Option<&str>,
    ids: &[String],
) -> Option<(ScrollerScrollApply, ScrollerScrollToken)> {
    let token = scroller_scroll_token(request, generation)?;
    if !should_apply_scroller_scroll(last, Some(&token)) {
        return None;
    }
    let apply = scroller_scroll_apply(request, ids)?;
    Some((apply, token))
}

pub fn node_fingerprint(node: &Node) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}", strip_callback_ids(node)).hash(&mut hasher);
    hasher.finish()
}

fn strip_callback_ids(node: &Node) -> Node {
    let mut n = node.clone();
    n.on_click = None;
    n.on_change = None;
    n.on_release = None;
    n.on_submit = None;
    n.on_double_click = None;
    n.on_blur = None;
    n.on_escape = None;
    n.on_close = None;
    n.on_copied = None;
    n.on_ok = None;
    n.on_cancel = None;
    n.on_confirm = None;
    n.on_open_change = None;
    n.on_forward_change = None;
    n.children = n.children.iter().map(strip_callback_ids).collect();
    n.trigger = n.trigger.as_deref().map(strip_callback_ids).map(Box::new);
    n.footer = n.footer.as_deref().map(strip_callback_ids).map(Box::new);
    n.stack_style = n
        .stack_style
        .as_deref()
        .map(strip_callback_ids)
        .map(Box::new);
    n.shimmer_style = n
        .shimmer_style
        .as_deref()
        .map(strip_callback_ids)
        .map(Box::new);
    n.separator_style = n
        .separator_style
        .as_deref()
        .map(strip_callback_ids)
        .map(Box::new);
    n.content_style = n
        .content_style
        .as_deref()
        .map(strip_callback_ids)
        .map(Box::new);
    n.list_style = n
        .list_style
        .as_deref()
        .map(strip_callback_ids)
        .map(Box::new);
    n.row_style = n.row_style.as_deref().map(strip_callback_ids).map(Box::new);
    n.jump_button_style = n
        .jump_button_style
        .as_deref()
        .map(strip_callback_ids)
        .map(Box::new);
    n.jump_button_renderer = n
        .jump_button_renderer
        .as_deref()
        .map(strip_callback_ids)
        .map(Box::new);
    n
}

/// True when rows that already existed in `prev` changed their render fingerprint.
///
/// Child-list splice (append/prepend) does not remeasure by itself. Streaming
/// text, upload status, or reactions on a surviving row still need `remeasure`.
pub fn scroller_survivors_changed(edit: &ScrollerEdit, prev_fps: &[u64], next_fps: &[u64]) -> bool {
    match edit {
        ScrollerEdit::Leave => prev_fps != next_fps,
        ScrollerEdit::Reset { .. } => false,
        ScrollerEdit::Append(added) => {
            let keep = next_fps.len().saturating_sub(*added);
            prev_fps != next_fps.get(..keep).unwrap_or(&[])
        }
        ScrollerEdit::Prepend(added) => prev_fps != next_fps.get(*added..).unwrap_or(&[]),
    }
}

pub fn parse_message_alignment(value: Option<&str>) -> Option<MessageAlignment> {
    match value.map(catalog::normalize) {
        Some(name) if name == "end" => Some(MessageAlignment::End),
        Some(name) if name == "start" => Some(MessageAlignment::Start),
        _ => None,
    }
}

pub fn parse_bubble_variant(value: Option<&str>) -> BubbleVariant {
    match value.map(catalog::normalize) {
        Some(name) if name == "secondary" => BubbleVariant::Secondary,
        Some(name) if name == "muted" => BubbleVariant::Muted,
        Some(name) if name == "tinted" => BubbleVariant::Tinted,
        Some(name) if name == "outline" => BubbleVariant::Outline,
        Some(name) if name == "ghost" => BubbleVariant::Ghost,
        Some(name) if name == "destructive" || name == "danger" => BubbleVariant::Destructive,
        _ => BubbleVariant::Filled,
    }
}

pub fn parse_attachment_status(value: Option<&str>) -> AttachmentStatus {
    match value.map(catalog::normalize) {
        Some(name) if name == "pending" => AttachmentStatus::Pending,
        Some(name) if name == "uploading" => AttachmentStatus::Uploading,
        Some(name) if name == "processing" => AttachmentStatus::Processing,
        Some(name) if name == "failed" || name == "error" => AttachmentStatus::Failed,
        _ => AttachmentStatus::Complete,
    }
}

pub fn parse_marker_variant(value: Option<&str>) -> MarkerVariant {
    match value.map(catalog::normalize) {
        Some(name) if name == "separator" => MarkerVariant::Separator,
        Some(name) if name == "border" => MarkerVariant::Border,
        _ => MarkerVariant::Plain,
    }
}

pub fn parse_marker_loading_style(value: Option<&str>) -> MarkerLoadingStyle {
    match value.map(catalog::normalize) {
        Some(name) if name == "shimmer" => MarkerLoadingStyle::Shimmer,
        _ => MarkerLoadingStyle::Spinner,
    }
}

pub fn parse_reaction_side(value: Option<&str>) -> Option<BubbleReactionSide> {
    match value.map(catalog::normalize) {
        Some(name) if name == "top" => Some(BubbleReactionSide::Top),
        Some(name) if name == "bottom" => Some(BubbleReactionSide::Bottom),
        _ => None,
    }
}

fn parse_marker_role(value: Option<&str>) -> Option<gpui::Role> {
    match value.map(catalog::normalize) {
        Some(name) if name == "status" => Some(gpui::Role::Status),
        Some(name) if name == "alert" => Some(gpui::Role::Alert),
        Some(name) if name == "log" => Some(gpui::Role::Log),
        _ => None,
    }
}

fn apply_node_style<E: Styled>(el: E, node: &Node) -> E {
    mapping::apply_styled(el, node)
}

fn child_path(path: &str, index: usize) -> String {
    format!("{path}.{index}")
}

fn paint_child<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> AnyElement {
    if is_chat_kind(&node.kind) {
        render_any(p, node, path)
    } else {
        p.paint_node(node, path)
    }
}

fn kit_button<P: NodePainter>(p: &P, node: &Node, path: &str) -> Button {
    let label = node.text.clone().unwrap_or_default();
    let mut button = Button::new(SharedString::from(path.to_string())).label(label);
    let chrome = mapping::button_chrome(
        node.variant.as_deref(),
        node.primary,
        node.outline,
        node.selected,
        node.control_size.as_deref(),
    );
    button = mapping::apply_named_button_variant(button, chrome.variant);
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
    if let Some(callback) = node.on_click.clone() {
        if let Some(tx) = p.cmd_tx() {
            button = button.on_click(move |_, _, _| {
                let _ = tx.send(Cmd::Callback {
                    id: callback.clone(),
                    value: None,
                    seq: None,
                });
            });
        }
    }
    button
}

fn render_message<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> Message {
    let mut msg = Message::new();
    if let Some(alignment) = parse_message_alignment(node.alignment.as_deref()) {
        msg = msg.alignment(alignment);
    }
    if let Some(style) = mapping::style_refinement(node.stack_style.as_deref()) {
        msg = msg.with_stack_style(style);
    }
    let mut avatar_slot = None;
    let mut avatar_el = None;
    let mut header = None;
    let mut footer = None;
    let mut content_nodes = Vec::new();
    let mut leftover = Vec::new();
    for (index, child) in node.children.iter().enumerate() {
        let child_path = child_path(path, index);
        match child.kind.as_str() {
            "message-avatar" => avatar_slot = Some((child, child_path)),
            "avatar" if avatar_slot.is_none() && avatar_el.is_none() => {
                avatar_el = Some((child, child_path));
            }
            "message-header" => header = Some((child, child_path)),
            "message-footer" => footer = Some((child, child_path)),
            "message-content" => content_nodes.push((child, child_path)),
            _ => leftover.push((child, child_path)),
        }
    }
    if let Some((child, child_path)) = avatar_slot {
        msg = msg.avatar_slot(render_message_avatar(p, child, &child_path));
    } else if let Some((child, child_path)) = avatar_el {
        msg = msg.avatar(paint_child(p, child, &child_path));
    }
    if let Some((child, child_path)) = header {
        msg = msg.header(render_message_header(p, child, &child_path));
    }
    let mut content = MessageContent::new();
    let mut has_content = false;
    for (child, child_path) in content_nodes {
        has_content = true;
        content = extend_message_content(p, content, child, &child_path);
    }
    for (child, child_path) in leftover {
        has_content = true;
        if child.kind == "bubble" {
            content = content.bubble(render_bubble(p, child, &child_path));
        } else {
            content = content.child(paint_child(p, child, &child_path));
        }
    }
    if has_content {
        msg = msg.content(content);
    }
    if let Some((child, child_path)) = footer {
        msg = msg.footer(render_message_footer(p, child, &child_path));
    }
    apply_node_style(msg, node)
}

fn extend_message_content<P: NodePainter>(
    p: &mut P,
    mut content: MessageContent,
    node: &Node,
    path: &str,
) -> MessageContent {
    if node.children.is_empty() {
        if let Some(text) = node.text.clone().filter(|s| !s.is_empty()) {
            return content.child(text);
        }
        return content;
    }
    for (index, child) in node.children.iter().enumerate() {
        let child_path = child_path(path, index);
        if child.kind == "bubble" {
            content = content.bubble(render_bubble(p, child, &child_path));
        } else {
            content = content.child(paint_child(p, child, &child_path));
        }
    }
    apply_node_style(content, node)
}

fn render_message_group<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> MessageGroup {
    let mut group = MessageGroup::new();
    for (index, child) in node.children.iter().enumerate() {
        group = group.child(paint_child(p, child, &child_path(path, index)));
    }
    apply_node_style(group, node)
}

fn render_message_avatar<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> MessageAvatar {
    let mut avatar = MessageAvatar::new();
    if node.children.is_empty() {
        if let Some(text) = node.text.clone().filter(|s| !s.is_empty()) {
            avatar = avatar.child(text);
        }
    } else {
        for (index, child) in node.children.iter().enumerate() {
            avatar = avatar.child(paint_child(p, child, &child_path(path, index)));
        }
    }
    apply_node_style(avatar, node)
}

fn render_message_header<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> MessageHeader {
    let mut header = MessageHeader::new();
    if let Some(inset) = node.content_inset {
        header = header.content_inset(inset);
    }
    if node.children.is_empty() {
        if let Some(text) = node.text.clone().filter(|s| !s.is_empty()) {
            header = header.child(text);
        }
    } else {
        for (index, child) in node.children.iter().enumerate() {
            header = header.child(paint_child(p, child, &child_path(path, index)));
        }
    }
    apply_node_style(header, node)
}

fn render_message_footer<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> MessageFooter {
    let mut footer = MessageFooter::new();
    if let Some(inset) = node.content_inset {
        footer = footer.content_inset(inset);
    }
    if node.children.is_empty() {
        if let Some(text) = node.text.clone().filter(|s| !s.is_empty()) {
            footer = footer.child(text);
        }
    } else {
        for (index, child) in node.children.iter().enumerate() {
            footer = footer.child(paint_child(p, child, &child_path(path, index)));
        }
    }
    apply_node_style(footer, node)
}

fn render_message_content<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> MessageContent {
    extend_message_content(p, MessageContent::new(), node, path)
}

fn render_bubble<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> Bubble {
    let mut bubble = Bubble::new().with_variant(parse_bubble_variant(node.variant.as_deref()));
    if let Some(alignment) = parse_message_alignment(node.alignment.as_deref()) {
        bubble = bubble.alignment(alignment);
    }
    let mut reactions = None;
    let mut content_slot = None;
    let mut kids = Vec::new();
    for (index, child) in node.children.iter().enumerate() {
        let child_path = child_path(path, index);
        match child.kind.as_str() {
            "bubble-reactions" => reactions = Some((child, child_path)),
            "bubble-content" => content_slot = Some((child, child_path)),
            _ => kids.push((child, child_path)),
        }
    }
    let has_content_slot = content_slot.is_some();
    if let Some((child, child_path)) = content_slot {
        bubble = bubble.content(render_bubble_content(p, child, &child_path));
    }
    if kids.is_empty() {
        if !has_content_slot {
            if let Some(text) = node.text.clone().filter(|s| !s.is_empty()) {
                bubble = bubble.child(text);
            }
        }
    } else {
        for (child, child_path) in kids {
            bubble = bubble.child(paint_child(p, child, &child_path));
        }
    }
    if let Some((child, child_path)) = reactions {
        bubble = bubble.reactions(render_bubble_reactions(p, child, &child_path));
    }
    apply_node_style(bubble, node)
}

fn render_bubble_content<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> BubbleContent {
    let mut content = BubbleContent::new();
    if node.children.is_empty() {
        if let Some(text) = node.text.clone().filter(|s| !s.is_empty()) {
            content = content.child(text);
        }
    } else {
        for (index, child) in node.children.iter().enumerate() {
            content = content.child(paint_child(p, child, &child_path(path, index)));
        }
    }
    apply_node_style(content, node)
}

fn render_bubble_group<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> BubbleGroup {
    let mut group = BubbleGroup::new();
    for (index, child) in node.children.iter().enumerate() {
        group = group.child(paint_child(p, child, &child_path(path, index)));
    }
    apply_node_style(group, node)
}

fn render_bubble_reactions<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> BubbleReactions {
    let mut reactions = BubbleReactions::new();
    if let Some(side) = parse_reaction_side(node.side.as_deref()) {
        reactions = reactions.side(side);
    }
    if let Some(alignment) = parse_message_alignment(node.alignment.as_deref()) {
        reactions = reactions.alignment(alignment);
    }
    if node.children.is_empty() {
        if let Some(text) = node.text.clone().filter(|s| !s.is_empty()) {
            reactions = reactions.child(text);
        }
    } else {
        for (index, child) in node.children.iter().enumerate() {
            let child_path = child_path(path, index);
            if child.kind == "button" {
                reactions = reactions.action(kit_button(p, child, &child_path));
            } else {
                reactions = reactions.child(paint_child(p, child, &child_path));
            }
        }
    }
    apply_node_style(reactions, node)
}

fn render_attachment<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> Attachment {
    let mut attachment = Attachment::new()
        .status(parse_attachment_status(node.status.as_deref()))
        .axis(mapping::parse_axis(node.orientation.as_deref()))
        .with_size(mapping::parse_scale(node.control_size.as_deref()));
    if let Some(id) = node.id.clone().filter(|s| !s.is_empty()) {
        attachment = attachment.id(id);
    }
    if let Some(callback) = node.on_click.clone() {
        if let Some(tx) = p.cmd_tx() {
            attachment = attachment.on_click(move |_, _, _| {
                let _ = tx.send(Cmd::Callback {
                    id: callback.clone(),
                    value: None,
                    seq: None,
                });
            });
        }
    }
    let mut media = None;
    let mut content = None;
    let mut actions = None;
    let mut leftover = Vec::new();
    for (index, child) in node.children.iter().enumerate() {
        let child_path = child_path(path, index);
        match child.kind.as_str() {
            "attachment-media" => media = Some((child, child_path)),
            "attachment-content" => content = Some((child, child_path)),
            "attachment-actions" => actions = Some((child, child_path)),
            _ => leftover.push((child, child_path)),
        }
    }
    if let Some((child, child_path)) = media {
        attachment = attachment.media(render_attachment_media(p, child, &child_path));
    }
    if let Some((child, child_path)) = content {
        attachment = attachment.content(render_attachment_content(p, child, &child_path));
    } else if !leftover.is_empty() {
        let mut body = AttachmentContent::new();
        for (child, child_path) in leftover {
            body = extend_attachment_content(p, body, child, &child_path);
        }
        attachment = attachment.content(body);
    }
    if let Some((child, child_path)) = actions {
        attachment = attachment.actions(render_attachment_actions(p, child, &child_path));
    }
    apply_node_style(attachment, node)
}

fn render_attachment_media<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> AttachmentMedia {
    let mut media = AttachmentMedia::new();
    if let Some(size) = mapping::parse_named_size(node.control_size.as_deref()) {
        media = media.with_size(size);
    }
    if let Some(src) = node.src.clone().filter(|s| !s.is_empty()) {
        media = media.src(src);
    }
    for (index, child) in node.children.iter().enumerate() {
        let child_path = child_path(path, index);
        if child.kind == "attachment-media-overlay" {
            media = media.overlay(render_media_overlay_inner(p, child, &child_path));
        } else {
            media = media.child(paint_child(p, child, &child_path));
        }
    }
    apply_node_style(media, node)
}

fn render_media_overlay_inner<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> AnyElement {
    if node.children.is_empty() {
        if let Some(text) = node.text.clone().filter(|s| !s.is_empty()) {
            return text.into_any_element();
        }
        return apply_node_style(div(), node).into_any_element();
    }
    if node.children.len() == 1 && node.text.is_none() {
        let child = &node.children[0];
        return paint_child(p, child, &child_path(path, 0));
    }
    let mut wrap = apply_node_style(div(), node);
    if let Some(text) = node.text.clone().filter(|s| !s.is_empty()) {
        wrap = wrap.child(text);
    }
    for (index, child) in node.children.iter().enumerate() {
        wrap = wrap.child(paint_child(p, child, &child_path(path, index)));
    }
    wrap.into_any_element()
}

fn extend_attachment_content<P: NodePainter>(
    p: &mut P,
    content: AttachmentContent,
    node: &Node,
    path: &str,
) -> AttachmentContent {
    match node.kind.as_str() {
        "attachment-title" => content.title(render_attachment_title(node)),
        "attachment-description" => content.description(render_attachment_description(node)),
        _ => content.child(paint_child(p, node, path)),
    }
}

fn render_attachment_content<P: NodePainter>(
    p: &mut P,
    node: &Node,
    path: &str,
) -> AttachmentContent {
    let mut content = AttachmentContent::new();
    if node.children.is_empty() {
        if let Some(text) = node.text.clone().filter(|s| !s.is_empty()) {
            content = content.title(AttachmentTitle::new(text));
        }
        if let Some(message) = node.message.clone().filter(|s| !s.is_empty()) {
            content = content.description(AttachmentDescription::new(message));
        }
    } else {
        for (index, child) in node.children.iter().enumerate() {
            content = extend_attachment_content(p, content, child, &child_path(path, index));
        }
    }
    apply_node_style(content, node)
}

fn render_attachment_title(node: &Node) -> AttachmentTitle {
    let mut title = AttachmentTitle::new(node.text.clone().unwrap_or_default());
    if node.status.is_some() {
        title = title.status(parse_attachment_status(node.status.as_deref()));
    }
    if let Some(style) = mapping::shimmer_style(node.shimmer_style.as_deref()) {
        title = title.with_shimmer_style(style);
    }
    apply_node_style(title, node)
}

fn render_attachment_description(node: &Node) -> AttachmentDescription {
    let mut description = AttachmentDescription::new(node.text.clone().unwrap_or_default());
    if node.status.is_some() {
        description = description.status(parse_attachment_status(node.status.as_deref()));
    }
    apply_node_style(description, node)
}

fn render_attachment_actions<P: NodePainter>(
    p: &mut P,
    node: &Node,
    path: &str,
) -> AttachmentActions {
    let mut actions = AttachmentActions::new();
    for (index, child) in node.children.iter().enumerate() {
        actions = actions.child(paint_child(p, child, &child_path(path, index)));
    }
    apply_node_style(actions, node)
}

fn render_attachment_group<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> AttachmentGroup {
    let id = node
        .id
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| path.to_string());
    let mut group = AttachmentGroup::new(id);
    for (index, child) in node.children.iter().enumerate() {
        group = group.child(paint_child(p, child, &child_path(path, index)));
    }
    apply_node_style(group, node)
}

fn render_marker<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> Marker {
    let mut marker = Marker::new()
        .with_variant(parse_marker_variant(node.variant.as_deref()))
        .loading(node.loading)
        .with_loading_style(parse_marker_loading_style(node.loading_style.as_deref()));
    if let Some(id) = node.id.clone().filter(|s| !s.is_empty()) {
        marker = marker.id(id);
    }
    if let Some(role) = parse_marker_role(node.role.as_deref()) {
        marker = marker.role(role);
    }
    if let Some(style) = mapping::shimmer_style(node.shimmer_style.as_deref()) {
        marker = marker.with_shimmer_style(style);
    }
    if let Some(style) = mapping::style_refinement(node.separator_style.as_deref()) {
        marker = marker.separator_style(style);
    }
    if let Some(name) = node.icon.as_deref().filter(|s| !s.is_empty()) {
        if let Some(icon) = mapping::parse_icon(name) {
            marker = marker.icon(MarkerIcon::new().child(Icon::new(icon)));
        }
    }
    let mut has_content = false;
    for (index, child) in node.children.iter().enumerate() {
        let child_path = child_path(path, index);
        match child.kind.as_str() {
            "marker-icon" => marker = marker.icon(render_marker_icon(p, child, &child_path)),
            "marker-content" => {
                has_content = true;
                marker = marker.content(render_marker_content(p, child, &child_path));
            }
            _ => marker = marker.child(paint_child(p, child, &child_path)),
        }
    }
    if !has_content {
        if let Some(text) = node.text.clone().filter(|s| !s.is_empty()) {
            marker = marker.content(MarkerContent::new().text(text));
        }
    }
    apply_node_style(marker, node)
}

fn render_marker_icon<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> MarkerIcon {
    let mut icon = MarkerIcon::new();
    if let Some(name) = node.icon.as_deref().or(node.text.as_deref()) {
        if let Some(parsed) = mapping::parse_icon(name) {
            icon = icon.child(Icon::new(parsed));
        }
    }
    for (index, child) in node.children.iter().enumerate() {
        icon = icon.child(paint_child(p, child, &child_path(path, index)));
    }
    apply_node_style(icon, node)
}

fn render_marker_content<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> MarkerContent {
    let mut content = MarkerContent::new();
    if let Some(text) = node.text.clone().filter(|s| !s.is_empty()) {
        content = content.text(text);
    }
    for (index, child) in node.children.iter().enumerate() {
        content = content.child(paint_child(p, child, &child_path(path, index)));
    }
    apply_node_style(content, node)
}

fn render_scroller_static<P: NodePainter>(p: &mut P, node: &Node, path: &str) -> AnyElement {
    let mut col = apply_node_style(v_flex().gap_2().w_full().min_w_0(), node);
    for (index, child) in node.children.iter().enumerate() {
        col = col.child(paint_child(p, child, &child_path(path, index)));
    }
    col.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chat_parsers_use_kit_names() {
        assert_eq!(
            parse_message_alignment(Some("end")),
            Some(MessageAlignment::End)
        );
        assert_eq!(parse_message_alignment(Some("left")), None);
        assert_eq!(parse_bubble_variant(Some("ghost")), BubbleVariant::Ghost);
        assert_eq!(
            parse_attachment_status(Some("uploading")),
            AttachmentStatus::Uploading
        );
        assert_eq!(
            parse_marker_variant(Some("separator")),
            MarkerVariant::Separator
        );
        assert_eq!(
            parse_marker_loading_style(Some("shimmer")),
            MarkerLoadingStyle::Shimmer
        );
        assert_eq!(
            parse_reaction_side(Some("top")),
            Some(BubbleReactionSide::Top)
        );
    }

    #[test]
    fn scroller_edit_detects_append_prepend_and_reset() {
        let a = ["m1".into(), "m2".into()];
        let append = ["m1".into(), "m2".into(), "m3".into()];
        let prepend = ["m0".into(), "m1".into(), "m2".into()];
        let replace = ["x".into(), "y".into()];
        assert_eq!(scroller_edit(&a, &a), ScrollerEdit::Leave);
        assert_eq!(scroller_edit(&a, &append), ScrollerEdit::Append(1));
        assert_eq!(scroller_edit(&a, &prepend), ScrollerEdit::Prepend(1));
        assert_eq!(
            scroller_edit(&a, &replace),
            ScrollerEdit::Reset { count: 2 }
        );
        assert_eq!(scroller_edit(&[], &a), ScrollerEdit::Reset { count: 2 });
        let index_ids_prev = ["idx:0".into(), "idx:1".into()];
        let index_ids_prepend = ["idx:0".into(), "idx:1".into(), "idx:2".into()];
        assert_eq!(
            scroller_edit(&index_ids_prev, &index_ids_prepend),
            ScrollerEdit::Append(1),
            "index keys cannot express prepend; callers must set stable :id"
        );
    }

    #[test]
    fn scroller_item_id_prefers_node_id() {
        let named: Node = serde_json::from_value(json!({"type": "message", "id": "m1"})).unwrap();
        assert_eq!(scroller_item_id(&named, 3), "m1");
        let anon: Node = serde_json::from_value(json!({"type": "message"})).unwrap();
        assert_eq!(scroller_item_id(&anon, 3), "idx:3");
    }

    #[test]
    fn scroller_scroll_request_end_wins_and_item_resolves_id_before_index() {
        let ids = ["m1".into(), "m2".into(), "m3".into()];
        assert!(scroller_scroll_request(None, None).is_none());
        assert!(scroller_scroll_request(Some(false), Some(&json!("m1"))).is_some());
        assert_eq!(
            scroller_scroll_request(Some(true), Some(&json!("m1"))),
            Some(ScrollerScroll::End),
            "scroll_to_end wins over scroll_to_item"
        );
        assert_eq!(
            scroller_scroll_request(None, Some(&json!("m2"))),
            Some(ScrollerScroll::Item("m2".into()))
        );
        assert_eq!(
            scroller_scroll_request(None, Some(&json!(0))),
            Some(ScrollerScroll::Item("0".into()))
        );
        assert_eq!(scroller_item_index("m2", &ids), Some(1));
        assert_eq!(scroller_item_index("0", &ids), Some(0), "numeric index");
        let id_zero = ["0".into(), "m1".into()];
        assert_eq!(
            scroller_item_index("0", &id_zero),
            Some(0),
            "matching row id wins over index"
        );
        assert_eq!(scroller_item_index("9", &ids), None);
        assert!(scroller_scroll_request(None, Some(&json!(""))).is_none());
        assert!(scroller_scroll_request(None, Some(&Value::Null)).is_none());
    }

    #[test]
    fn scroller_scroll_applies_on_new_token_not_omit_or_echo() {
        let end = ScrollerScroll::End;
        let item = ScrollerScroll::Item("m1".into());
        let first = scroller_scroll_token(Some(&item), Some("1"));
        assert!(should_apply_scroller_scroll(None, first.as_ref()));
        assert!(!should_apply_scroller_scroll(
            first.as_ref(),
            first.as_ref()
        ));
        let again = scroller_scroll_token(Some(&item), Some("2"));
        assert!(
            should_apply_scroller_scroll(first.as_ref(), again.as_ref()),
            "generation bump re-applies the same row"
        );
        assert!(!should_apply_scroller_scroll(first.as_ref(), None));
        let shifted = scroller_scroll_token(Some(&item), Some("1"));
        assert_eq!(
            shifted, first,
            "token is the id spec, not the resolved index"
        );
        let to_end = scroller_scroll_token(Some(&end), Some("1"));
        assert!(should_apply_scroller_scroll(
            first.as_ref(),
            to_end.as_ref()
        ));
        assert_eq!(
            scroller_scroll_generation(Some(&json!(3))).as_deref(),
            Some("3")
        );
        assert!(scroller_scroll_generation(Some(&json!(""))).is_none());
        let colon_id = scroller_scroll_token(Some(&ScrollerScroll::Item("a:b".into())), Some("c"));
        let colon_gen = scroller_scroll_token(Some(&ScrollerScroll::Item("a".into())), Some("b:c"));
        assert_ne!(
            colon_id, colon_gen,
            "row id and generation are not flattened through `:`"
        );
    }

    #[test]
    fn unresolved_scroll_to_item_is_not_applied_until_the_row_exists() {
        let request = scroller_scroll_request(None, Some(&json!("m9")));
        let mut last = None;
        let absent = ["m1".into(), "m2".into()];
        assert!(
            scroller_scroll_plan(last.as_ref(), request.as_ref(), None, &absent).is_none(),
            "absent row must not consume the request"
        );
        assert!(
            last.is_none(),
            "failed resolve does not record a last-applied token"
        );

        let loaded = ["m1".into(), "m2".into(), "m9".into()];
        let (apply, token) = scroller_scroll_plan(last.as_ref(), request.as_ref(), None, &loaded)
            .expect("same request is attempted once the row exists");
        assert_eq!(apply, ScrollerScrollApply::Item(2));
        assert!(
            scroller_scroll_plan(last.as_ref(), request.as_ref(), None, &loaded).is_some(),
            "Kit reject (token not stored) retries the same request"
        );
        last = Some(token);
        assert!(
            scroller_scroll_plan(last.as_ref(), request.as_ref(), None, &loaded).is_none(),
            "successful apply then echoes"
        );
    }

    #[test]
    fn fingerprint_ignores_generated_callback_ids() {
        let a: Node = serde_json::from_value(json!({
            "type": "message",
            "text": "Hi",
            "children": [{
                "type": "button",
                "text": "Copy",
                "on-click": "cb-1"
            }]
        }))
        .unwrap();
        let b: Node = serde_json::from_value(json!({
            "type": "message",
            "text": "Hi",
            "children": [{
                "type": "button",
                "text": "Copy",
                "on-click": "cb-99"
            }]
        }))
        .unwrap();
        assert_eq!(node_fingerprint(&a), node_fingerprint(&b));
        let changed: Node = serde_json::from_value(json!({
            "type": "message",
            "text": "Hello",
            "children": [{
                "type": "button",
                "text": "Copy",
                "on-click": "cb-99"
            }]
        }))
        .unwrap();
        assert_ne!(node_fingerprint(&a), node_fingerprint(&changed));
    }

    #[test]
    fn append_and_prepend_detect_survivor_fingerprint_changes() {
        let prev = [1u64, 2];
        let append_same = [1u64, 2, 3];
        let append_changed = [1u64, 9, 3];
        let prepend_same = [0u64, 1, 2];
        let prepend_changed = [0u64, 8, 2];
        assert!(!scroller_survivors_changed(
            &ScrollerEdit::Append(1),
            &prev,
            &append_same
        ));
        assert!(scroller_survivors_changed(
            &ScrollerEdit::Append(1),
            &prev,
            &append_changed
        ));
        assert!(!scroller_survivors_changed(
            &ScrollerEdit::Prepend(1),
            &prev,
            &prepend_same
        ));
        assert!(scroller_survivors_changed(
            &ScrollerEdit::Prepend(1),
            &prev,
            &prepend_changed
        ));
        assert!(!scroller_survivors_changed(
            &ScrollerEdit::Reset { count: 3 },
            &prev,
            &append_changed
        ));
    }

    #[test]
    fn attachment_media_omitted_size_does_not_use_medium_default() {
        assert!(mapping::parse_named_size(None).is_none());
        assert_eq!(mapping::parse_scale(None), gpui_component::Size::Medium);
        assert_eq!(
            mapping::parse_named_size(Some("lg")),
            Some(gpui_component::Size::Large)
        );
    }
}
