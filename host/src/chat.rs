//! Kit 0.6 Message / Bubble / Attachment / Marker / MessageScroller.
//!
//! All of these except `MessageScrollerState` are `RenderOnce`. Overlay must
//! not import `extra`; this module only depends on `protocol` and `mapping` so
//! both `renderer` and `overlay` can paint the family.

use crate::catalog;
use crate::mapping;
use crate::protocol::{Cmd, Node};
use gpui::{AnyElement, Hsla, IntoElement, ParentElement, SharedString, Styled, px};
use gpui_component::{
    Colorize as _, Disableable as _, Icon, Selectable as _, Sizable as _,
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

pub fn node_fingerprint(node: &Node) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{node:?}").hash(&mut hasher);
    hasher.finish()
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

fn apply_node_style<E: Styled>(mut el: E, node: &Node) -> E {
    if let Some(gap) = node.gap {
        el = el.gap(px(gap));
    }
    if let Some(padding) = node.padding {
        el = el.p(px(padding));
    }
    if let Some(width) = node.width {
        el = el.w(px(width));
    }
    if let Some(height) = node.height {
        el = el.h(px(height));
    }
    if let Some(color) = node
        .color
        .as_deref()
        .and_then(|s| Hsla::parse_hex(s.trim()).ok())
    {
        el = el.text_color(color);
    }
    if let Some(bg) = node
        .bg
        .as_deref()
        .and_then(|s| Hsla::parse_hex(s.trim()).ok())
    {
        el = el.bg(bg);
    }
    el
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
    if !kids.is_empty() || (!has_content_slot && node.text.is_some()) {
        let mut content = BubbleContent::new();
        if kids.is_empty() {
            if let Some(text) = node.text.clone().filter(|s| !s.is_empty()) {
                content = content.child(text);
            }
        } else {
            for (child, child_path) in kids {
                content = content.child(paint_child(p, child, &child_path));
            }
        }
        bubble = bubble.content(content);
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
    if let Some(src) = node.src.clone().filter(|s| !s.is_empty()) {
        media = media.src(src);
        for (index, child) in node.children.iter().enumerate() {
            media = media.overlay(paint_child(p, child, &child_path(path, index)));
        }
    } else {
        for (index, child) in node.children.iter().enumerate() {
            media = media.child(paint_child(p, child, &child_path(path, index)));
        }
    }
    apply_node_style(media, node)
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
}
