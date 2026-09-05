//! Overlay family: dialogs, sheets, notifications, popovers, and popup menus.
//!
//! Dialogs and sheets are not ordinary tree children. gpui-component paints
//! them from `Root` via `WindowExt`. The host collects open overlays from the
//! Clojure tree and syncs on the next frame so `RootView::render` never
//! re-enters `Root`. Notifications are a stack on `Root.notification`.

use crate::chat;
use crate::mapping;
use crate::protocol::{self, Cmd, Item, Node};
use gpui::{
    App, Axis, Div, Hsla, InteractiveElement, IntoElement, ParentElement, SharedString, Styled,
    Window, div, px,
};
use gpui_component::{
    Colorize as _, Disableable as _, Icon, IconName, Selectable as _, Sizable as _, Size,
    alert::Alert,
    avatar::{Avatar, AvatarGroup},
    badge::Badge,
    breadcrumb::{Breadcrumb, BreadcrumbItem},
    button::{Button, DropdownButton},
    clipboard::Clipboard,
    dialog::AlertDialog,
    group_box::{GroupBox, GroupBoxVariants as _},
    h_flex,
    hover_card::HoverCard,
    kbd::Kbd,
    link::Link,
    menu::{PopupMenu, PopupMenuItem},
    pagination::Pagination,
    progress::{Progress, ProgressCircle},
    separator::Separator,
    shimmer::ShimmerText,
    skeleton::Skeleton,
    spinner::Spinner,
    tag::Tag,
    v_flex,
};
use gpui_kit as gpui;
use gpui_kit::component as gpui_component;
use serde_json::json;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct DialogSpec {
    pub key: String,
    pub node: Node,
}

fn is_dialog_kind(kind: &str) -> bool {
    matches!(kind, "dialog" | "alert-dialog")
}

pub fn node_key(node: &Node, path: &str) -> String {
    node.id
        .as_deref()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.to_string())
}

/// Kit `Avatar` from a node. Image `src` is an http URL or file path.
pub fn kit_avatar(node: &Node) -> Avatar {
    let mut avatar = Avatar::new().with_size(mapping::parse_scale(node.control_size.as_deref()));
    if let Some(name) = node
        .text
        .clone()
        .or(node.title.clone())
        .filter(|s| !s.is_empty())
    {
        avatar = avatar.name(name);
    }
    if let Some(src) = node.src.as_deref().filter(|s| !s.is_empty()) {
        avatar = avatar.src(src.to_string());
    }
    if let Some(icon) = node.icon.as_deref().and_then(mapping::parse_icon) {
        avatar = avatar.placeholder(icon);
    }
    avatar
}

/// Kit `AvatarGroup` from a node. A host wrap owns the overlap width;
/// the group fills that box (`w_full`). Kit's negative child margins make
/// flex min-content about one face, so an unsized row stacks every avatar
/// on the same spot.
pub(crate) fn kit_avatar_group(node: &Node) -> AvatarGroup {
    let mut group =
        AvatarGroup::new().with_size(mapping::parse_scale(node.control_size.as_deref()));
    if let Some(limit) = node
        .limit
        .filter(|n| n.is_finite())
        .map(|n| n.round().max(0.0) as usize)
    {
        group = group.limit(limit);
    }
    if node.ellipsis {
        group = group.ellipsis();
    }
    group
        .children(
            node.children
                .iter()
                .filter(|child| child.kind == "avatar")
                .map(kit_avatar),
        )
        .flex_shrink_0()
        .w_full()
}

/// Kit AvatarGroup *visual* width after 30% overlap: `face * (1 + 0.7 *
/// (visible - 1))`, plus the ellipsis avatar and `ml_1` (0.25rem ≈ 4px).
pub(crate) fn avatar_group_content_width(node: &Node) -> f32 {
    let face = avatar_face_px(mapping::parse_scale(node.control_size.as_deref()));
    let (visible, ellipsis) = avatar_group_visible(node);
    if visible == 0 {
        return if ellipsis { face + 4.0 } else { 0.0 };
    }
    let mut width = face * visible as f32 - face * 0.3 * (visible - 1) as f32;
    if ellipsis {
        width += face + 4.0;
    }
    width
}

/// Width the inner flex row needs so `flex_shrink_0` avatars fit before
/// Kit's negative margins overlap them. Taffy does not shrink this by
/// those margins, so a wrap sized to the visual overlap overflows left.
pub(crate) fn avatar_group_flex_width(node: &Node) -> f32 {
    let face = avatar_face_px(mapping::parse_scale(node.control_size.as_deref()));
    let (visible, ellipsis) = avatar_group_visible(node);
    let n = visible + usize::from(ellipsis);
    if n == 0 {
        return 0.0;
    }
    face * n as f32 + if ellipsis { 4.0 } else { 0.0 }
}

fn avatar_group_visible(node: &Node) -> (usize, bool) {
    let count = node
        .children
        .iter()
        .filter(|child| child.kind == "avatar")
        .count();
    let limit = node
        .limit
        .filter(|n| n.is_finite())
        .map(|n| n.round().max(0.0) as usize)
        .unwrap_or(3);
    let visible = count.min(limit);
    let ellipsis = node.ellipsis && count > limit;
    (visible, ellipsis)
}

pub(crate) fn avatar_face_px(size: Size) -> f32 {
    match size {
        Size::Large => 80.0,
        Size::Medium => 48.0,
        Size::Small => 24.0,
        Size::XSmall => 16.0,
        Size::Size(px) => px.as_f32(),
    }
}

/// Host box for AvatarGroup: inner row is wide enough for `flex_shrink_0`
/// faces, outer clips to Kit's overlapped width and right-aligns so the
/// leftover (taffy ignores negative margins) is clipped on the left.
///
/// This wrap owns only that workaround geometry (and Clojure box keys the
/// caller applies on top). Kit `AvatarGroup: Styled` keys such as `:gap`
/// belong on `group`, not here — a one-child outer row would ignore them.
pub(crate) fn avatar_group_element(group: AvatarGroup, node: &Node) -> Div {
    let overlapped = avatar_group_content_width(node);
    let flex_w = avatar_group_flex_width(node);
    let face = avatar_face_px(mapping::parse_scale(node.control_size.as_deref()));
    h_flex()
        .flex_none()
        .w(px(overlapped))
        .h(px(face))
        .overflow_hidden()
        .justify_end()
        .child(
            h_flex()
                .w(px(flex_w))
                .h(px(face))
                .flex_shrink_0()
                .child(group),
        )
}

/// Clojure keys that refine Kit `AvatarGroup` vs the clip wrap.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AvatarGroupStyleSplit {
    pub kit_gap: Option<f32>,
    pub kit_padding: Option<f32>,
    pub wrap_width: Option<f32>,
    pub wrap_height: Option<f32>,
    pub wrap_size: Option<f32>,
    pub wrap_flex_fill: bool,
    pub wrap_workaround_w: f32,
    pub wrap_workaround_h: f32,
}

#[cfg(test)]
pub(crate) fn avatar_group_style_split(node: &Node) -> AvatarGroupStyleSplit {
    AvatarGroupStyleSplit {
        kit_gap: node.gap,
        kit_padding: node.padding,
        wrap_width: node.width,
        wrap_height: node.height,
        wrap_size: node.size,
        wrap_flex_fill: node.flex.unwrap_or(0.0) >= 1.0,
        wrap_workaround_w: avatar_group_content_width(node),
        wrap_workaround_h: avatar_face_px(mapping::parse_scale(node.control_size.as_deref())),
    }
}

fn kit_hover_card(node: &Node, path: &str, cx: Option<&App>) -> HoverCard {
    let mut card = HoverCard::new(SharedString::from(node_key(node, path)));
    if let Some(anchor) = mapping::parse_anchor(node.placement.as_deref()) {
        card = card.anchor(anchor);
    }
    if let Some(secs) = node.open_delay.filter(|n| n.is_finite() && *n >= 0.0) {
        card = card.open_delay(Duration::from_secs_f32(secs));
    }
    if let Some(secs) = node.close_delay.filter(|n| n.is_finite() && *n >= 0.0) {
        card = card.close_delay(Duration::from_secs_f32(secs));
    }
    if let Some(appearance) = node.appearance {
        card = card.appearance(appearance);
    }
    if let Some(trigger) = node.trigger.as_deref() {
        card = card.trigger(paint_chart_element(trigger, &format!("{path}-trigger"), cx));
    }
    card.children(node.children.iter().enumerate().map(|(child_ix, child)| {
        paint_chart_element(child, &static_child_path(path, child_ix), cx)
    }))
}

pub fn collect_open_dialogs(root: &Node) -> Vec<DialogSpec> {
    let mut out = Vec::new();
    walk_nodes(root, "root", &mut |node, path| {
        if is_dialog_kind(&node.kind) && node.open.unwrap_or(false) {
            out.push(DialogSpec {
                key: node_key(node, path),
                node: node.clone(),
            });
        }
    });
    out
}

/// Every `native-menu` node, open or closed. Show is a rising-edge snapshot.
pub fn collect_native_menus(root: &Node) -> Vec<(String, Node)> {
    let mut out = Vec::new();
    walk_nodes(root, "root", &mut |node, path| {
        if node.kind == "native-menu" {
            out.push((node_key(node, path), node.clone()));
        }
    });
    out
}

/// Exact-path walk used by PopupMenu `item_path` and the CljAction bridge.
/// Each segment is `Item::id_or_label()`. Disabled / separator hops fail.
/// The path must end on a leaf (empty nested `:items`).
pub fn item_at_path<'a>(items: &'a [Item], path: &[String]) -> Option<&'a Item> {
    let mut items = items;
    let mut selected = None;
    for identity in path {
        let item = items.iter().find(|item| item.id_or_label() == *identity)?;
        if item.disabled || item.is_separator() {
            return None;
        }
        selected = Some(item);
        items = &item.items;
    }
    selected.filter(|item| item.items.is_empty())
}

fn walk_nodes(node: &Node, path: &str, visit: &mut impl FnMut(&Node, &str)) {
    visit(node, path);
    if let Some(trigger) = node.trigger.as_ref() {
        walk_nodes(trigger, &format!("{path}-trigger"), visit);
    }
    if let Some(footer) = node.footer.as_ref() {
        walk_nodes(footer, &format!("{path}-footer"), visit);
    }
    for (index, child) in node.children.iter().enumerate() {
        walk_nodes(child, &format!("{path}-{index}"), visit);
    }
    for (index, child) in node.left.iter().enumerate() {
        walk_nodes(child, &format!("{path}-left-{index}"), visit);
    }
    for (index, child) in node.right.iter().enumerate() {
        walk_nodes(child, &format!("{path}-right-{index}"), visit);
    }
    for (index, item) in node.items.iter().enumerate() {
        if let Some(content) = item.content.as_ref() {
            // Match RootView::render_accordion for children without explicit ids.
            let content_path = if node.kind == "accordion" {
                format!("{path}-acc-{index}")
            } else {
                format!("{path}-item-{index}")
            };
            walk_nodes(content, &content_path, visit);
        }
        for (child_ix, child) in item.children.iter().enumerate() {
            walk_nodes(child, &format!("{path}-item-{index}-{child_ix}"), visit);
        }
    }
}

pub fn dialog_keys(specs: &[DialogSpec]) -> Vec<String> {
    specs.iter().map(|spec| spec.key.clone()).collect()
}

/// Latest Clojure dialog node for an open crate dialog.
///
/// `export-tree` rebuilds callback ids every render. The crate builder is
/// stored for the dialog's lifetime, so it must read this cell on each
/// `render_dialog_layer` paint instead of capturing ids from open time.
pub fn latest_dialog_spec(live: &RefCell<Vec<DialogSpec>>, key: &str) -> Option<DialogSpec> {
    live.borrow().iter().find(|spec| spec.key == key).cloned()
}

/// Queued native button/overlay handlers carry identity/intent, never callback ids.
/// Resolve against the installed tree immediately before sending the existing
/// Callback/CallbackBatch command. A queued round-trip must finish before
/// the next action can use the replacement callback registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueuedAction {
    ButtonClick {
        key: String,
    },
    DialogClose {
        key: String,
        ok: Option<bool>,
    },
    PopoverOpen {
        key: String,
        open: bool,
    },
    MenuSelect {
        key: String,
        item_path: Vec<String>,
    },
    /// NativeMenu / Command Action dispatch. `item_path` is the semantic
    /// path (submenu / group identities, then the leaf id).
    CljSelect {
        key: String,
        item_path: Vec<String>,
    },
    /// Command Kit `on_select` (highlight). Distinct from confirm.
    CommandSelect {
        key: String,
        item_path: Vec<String>,
    },
    /// Command Kit `on_confirm` after Action dispatch.
    CommandConfirm {
        key: String,
        item_path: Vec<String>,
    },
    CommandQuery {
        key: String,
        query: String,
    },
    CommandCancel {
        key: String,
    },
}

pub type ActionEmitter = Rc<dyn Fn(QueuedAction, &mut App)>;

/// Native Command value that left the queue in this callback batch.
///
/// Bound to the seq assigned at send time so a delayed flush (after
/// `wait_for_seq`) still installs the echo latch for the action that
/// was actually transmitted, not the one that originally tried to flush.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandEcho {
    Select { key: String, item_path: Vec<String> },
    Query { key: String, query: String },
}

impl CommandEcho {
    fn from_action(action: &QueuedAction) -> Option<Self> {
        match action {
            QueuedAction::CommandSelect { key, item_path } => Some(Self::Select {
                key: key.clone(),
                item_path: item_path.clone(),
            }),
            QueuedAction::CommandQuery { key, query } => Some(Self::Query {
                key: key.clone(),
                query: query.clone(),
            }),
            _ => None,
        }
    }
}

/// Calls plus Command echo metadata for one dequeued batch.
pub struct OutboundCallbacks {
    pub calls: Vec<protocol::CallbackCall>,
    pub command_echo: Option<CommandEcho>,
}

#[derive(Default)]
pub struct CallbackQueue {
    pending: VecDeque<QueuedAction>,
    wait_for_seq: Option<u64>,
}

impl CallbackQueue {
    pub fn push(&mut self, action: QueuedAction) {
        self.pending.push_back(action);
    }

    #[cfg(test)]
    pub fn next(&mut self, tree: &Node) -> Option<Vec<protocol::CallbackCall>> {
        self.next_outbound(tree).map(|outbound| outbound.calls)
    }

    /// Dequeue the next sendable batch, including Command echo metadata.
    ///
    /// `flush_callback_queue` binds that metadata to the seq it assigns,
    /// whether the flush ran from the original `on_select` / `on_query`
    /// or later from `HostEvent::Tree`.
    pub fn next_outbound(&mut self, tree: &Node) -> Option<OutboundCallbacks> {
        if self.wait_for_seq.is_some() {
            return None;
        }
        while let Some(action) = self.pending.pop_front() {
            let mut calls = action.resolve(tree);
            // Kit confirm is Action (CljSelect) then deferred on_confirm.
            // Drain the matching confirm into this batch so :on-change and
            // Kit :on-confirm share one callback generation.
            if let QueuedAction::CljSelect { key, .. } = &action
                && matches!(
                    self.pending.front(),
                    Some(QueuedAction::CommandConfirm { key: ck, .. }) if ck == key
                )
            {
                let confirm = self.pending.pop_front().unwrap();
                calls.extend(confirm.resolve(tree));
            }
            if !calls.is_empty() {
                return Some(OutboundCallbacks {
                    calls,
                    command_echo: CommandEcho::from_action(&action),
                });
            }
        }
        None
    }

    pub fn has_clj_select(&self, key: &str) -> bool {
        self.pending
            .iter()
            .any(|action| matches!(action, QueuedAction::CljSelect { key: k, .. } if k == key))
    }

    pub fn sent(&mut self, seq: u64) {
        self.wait_for_seq = Some(seq);
    }

    pub fn tree_installed(&mut self, seq: Option<u64>) {
        // An unrelated render must not release an action whose own batch
        // (and registry replacement) is still queued behind that render.
        if seq.is_some() && seq == self.wait_for_seq {
            self.wait_for_seq = None;
        }
    }

    pub fn clear(&mut self) {
        self.pending.clear();
        self.wait_for_seq = None;
    }
}

impl QueuedAction {
    fn resolve(&self, tree: &Node) -> Vec<protocol::CallbackCall> {
        let key = match self {
            Self::ButtonClick { key }
            | Self::DialogClose { key, .. }
            | Self::PopoverOpen { key, .. }
            | Self::MenuSelect { key, .. }
            | Self::CljSelect { key, .. }
            | Self::CommandSelect { key, .. }
            | Self::CommandConfirm { key, .. }
            | Self::CommandQuery { key, .. }
            | Self::CommandCancel { key } => key,
        };
        let mut found = None;
        walk_nodes(tree, "root", &mut |node, path| {
            let kind_matches = match self {
                Self::ButtonClick { .. } => node.kind == "button",
                Self::DialogClose { .. } => is_dialog_kind(&node.kind),
                Self::PopoverOpen { .. } => matches!(node.kind.as_str(), "popover" | "native-menu"),
                Self::MenuSelect { .. } => matches!(
                    node.kind.as_str(),
                    "dropdown-menu" | "context-menu" | "dropdown-button"
                ),
                Self::CljSelect { .. } => matches!(node.kind.as_str(), "native-menu" | "command"),
                Self::CommandSelect { .. }
                | Self::CommandConfirm { .. }
                | Self::CommandQuery { .. }
                | Self::CommandCancel { .. } => node.kind == "command",
            };
            if found.is_none() && kind_matches && node_key(node, path) == *key {
                found = Some(node.clone());
            }
        });
        if found.is_none() {
            if let Self::ButtonClick { .. } = self {
                found = node_at_static_path(tree, key);
            }
        }
        let Some(node) = found else { return Vec::new() };
        match self {
            Self::ButtonClick { .. } if node.kind == "button" && !node.disabled => node
                .on_click
                .map(|id| vec![protocol::CallbackCall::fire(id)])
                .unwrap_or_default(),
            Self::DialogClose { ok, .. } if node.open.unwrap_or(false) => {
                let first = match ok {
                    Some(true) => node.on_ok,
                    Some(false) => node.on_cancel,
                    None => None,
                };
                protocol::dialog_action_calls(first, node.on_close, node.on_open_change)
            }
            Self::PopoverOpen { open, .. } if node.open.unwrap_or(false) != *open => node
                .on_open_change
                .map(|id| vec![protocol::CallbackCall::with_value(id, json!(*open))])
                .unwrap_or_default(),
            Self::MenuSelect { item_path, .. } | Self::CljSelect { item_path, .. } => {
                let Some(item) = item_at_path(&node.items, item_path) else {
                    return Vec::new();
                };
                protocol::menu_selection_calls(item.on_click.clone(), node.on_change, item_path)
            }
            Self::CommandSelect { item_path, .. } => {
                if item_at_path(&node.items, item_path).is_none() {
                    return Vec::new();
                }
                node.on_select
                    .map(|id| {
                        vec![protocol::CallbackCall::with_value(
                            id,
                            protocol::menu_selection_payload(item_path),
                        )]
                    })
                    .unwrap_or_default()
            }
            Self::CommandConfirm { item_path, .. } => {
                if item_at_path(&node.items, item_path).is_none() {
                    return Vec::new();
                }
                node.on_confirm
                    .map(|id| {
                        vec![protocol::CallbackCall::with_value(
                            id,
                            protocol::menu_selection_payload(item_path),
                        )]
                    })
                    .unwrap_or_default()
            }
            Self::CommandQuery { query, .. } => node
                .on_query
                .map(|id| vec![protocol::CallbackCall::with_value(id, json!(query))])
                .unwrap_or_default(),
            Self::CommandCancel { .. } => node
                .on_cancel
                .map(|id| vec![protocol::CallbackCall::fire(id)])
                .unwrap_or_default(),
            // Removed/closed overlays and controlled-state echoes do not
            // represent a new semantic action and must not invoke anything.
            _ => Vec::new(),
        }
    }
}

/// One instance per native dialog opening, shared across live-spec repaints.
/// A second native mouse-down can reach the removed dialog before repaint.
#[derive(Default)]
pub struct DialogClose {
    ok: Option<bool>,
    dismissed: bool,
}

impl DialogClose {
    pub fn action(&mut self, ok: bool) -> bool {
        if self.dismissed {
            return false;
        }
        self.ok = Some(ok);
        true
    }

    pub fn take(&mut self, key: &str) -> Option<QueuedAction> {
        if self.dismissed {
            return None;
        }
        self.dismissed = true;
        Some(QueuedAction::DialogClose {
            key: key.into(),
            ok: self.ok,
        })
    }
}

/// Fill a popup menu from Clojure `{id, label}` rows (nested `:items` are submenus).
pub fn fill_popup_menu(
    mut menu: PopupMenu,
    items: &[Item],
    key: &str,
    item_path: &[String],
    emit: ActionEmitter,
    window: &mut Window,
    cx: &mut App,
) -> PopupMenu {
    for item in items {
        if item.is_separator() {
            menu = menu.item(PopupMenuItem::separator());
            continue;
        }
        if !item.items.is_empty() {
            let nested = item.items.clone();
            let emit = emit.clone();
            let key = key.to_string();
            let mut item_path = item_path.to_vec();
            item_path.push(item.id_or_label());
            let label = item.label_or_id();
            let mut entry = PopupMenuItem::submenu(
                label,
                PopupMenu::build(window, cx, {
                    let nested = nested.clone();
                    move |menu, window, cx| {
                        fill_popup_menu(menu, &nested, &key, &item_path, emit, window, cx)
                    }
                }),
            );
            if item.disabled {
                entry = entry.disabled(true);
            }
            if let Some(icon) = item.icon.as_deref().and_then(mapping::parse_icon) {
                entry = entry.icon(icon);
            }
            menu = menu.item(entry);
            continue;
        }

        let id = item.id_or_label();
        let mut entry = PopupMenuItem::new(item.label_or_id());
        if item.disabled {
            entry = entry.disabled(true);
        }
        if item.checked.unwrap_or(false) {
            entry = entry.checked(true);
        }
        if let Some(icon) = item.icon.as_deref().and_then(mapping::parse_icon) {
            entry = entry.icon(icon);
        }
        let emit = emit.clone();
        let key = key.to_string();
        let mut item_path = item_path.to_vec();
        item_path.push(id);
        entry = entry.on_click(move |_, _, cx| {
            emit(
                QueuedAction::MenuSelect {
                    key: key.clone(),
                    item_path: item_path.clone(),
                },
                cx,
            );
        });
        menu = menu.item(entry);
    }
    menu
}

/// Popover content runs as `Fn` during Popover paint while `RootView` is
/// borrowed, so it cannot consume `AnyElement`s from `render_node`. Rebuild
/// a small static tree (label / stack / button / separator) from cloned nodes.
///
/// `path` is a stable element-id prefix (`dialog-key/content`). Nested
/// children append `/index` so sibling stacks cannot collide. Button clicks
/// enqueue `QueuedAction::ButtonClick` with that path; ids are resolved
/// against the installed tree, never captured at paint.
pub fn paint_static(
    nodes: &[Node],
    emit: ActionEmitter,
    path: &str,
    cx: Option<&App>,
) -> gpui::AnyElement {
    v_flex()
        .gap(px(8.))
        .p(px(8.))
        .min_w(px(160.))
        .children(nodes.iter().enumerate().map(|(ix, node)| {
            paint_static_node(node, &static_child_path(path, ix), emit.clone(), cx)
        }))
        .into_any_element()
}

pub fn static_child_path(prefix: &str, index: usize) -> String {
    format!("{prefix}/{index}")
}

/// Paint a chart label node without overlay padding or min-width.
/// Clicks are ignored (radar spoke labels are not action hosts).
/// Kit accepts an arbitrary `AnyElement`; this paints the same RenderOnce
/// widgets as the main tree (badge, avatar, tag, …), not only the static
/// overlay subset used by dialogs.
pub fn paint_chart_label(node: &Node, path: &str) -> gpui::AnyElement {
    paint_chart_element(node, path, None)
}

fn chart_hex(text: Option<&str>) -> Option<Hsla> {
    text.and_then(|s| Hsla::parse_hex(s.trim()).ok())
}

/// Kit `Styled` keys. Used on the real widget. Same vocabulary as
/// `mapping::apply_visual_style` so overlay cells get truncate/nowrap.
fn chart_kit_style<E: Styled>(el: E, node: &Node) -> E {
    mapping::apply_visual_style(el, node)
}

/// Clojure box geometry (`:width` / `:height` / `:size` / `:flex`).
fn chart_outer_style<E: Styled>(el: E, node: &Node) -> E {
    mapping::apply_box_style(el, node)
}

fn chart_layout<E: Styled>(el: E, node: &Node) -> E {
    mapping::apply_styled(el, node)
}

fn chart_host(child: impl IntoElement, node: &Node, path: &str) -> gpui::AnyElement {
    chart_layout(div().id(SharedString::from(path.to_string())), node)
        .child(child)
        .into_any_element()
}

struct ChartPaint<'a> {
    cmd_tx: Option<&'a mpsc::Sender<Cmd>>,
    cx: Option<&'a App>,
}

impl crate::chat::NodePainter for ChartPaint<'_> {
    fn paint_node(&mut self, node: &Node, path: &str) -> gpui::AnyElement {
        paint_static_tree(node, path, self.cmd_tx, self.cx)
    }

    fn cmd_tx(&self) -> Option<mpsc::Sender<Cmd>> {
        self.cmd_tx.cloned()
    }

    fn app(&self) -> Option<&App> {
        self.cx
    }
}

pub(crate) fn paint_chart_element(node: &Node, path: &str, cx: Option<&App>) -> gpui::AnyElement {
    paint_static_tree(node, path, None, cx)
}

/// Paint a DataTable `render_td` widget. Same RenderOnce subset as radar
/// `:content` / scroller rows (progress, tag, badge, avatar, stacks, …),
/// not list / data-table / editor.
pub(crate) fn paint_table_cell(
    node: &Node,
    path: &str,
    cmd_tx: Option<&mpsc::Sender<Cmd>>,
    cx: Option<&App>,
) -> gpui::AnyElement {
    paint_static_tree(node, path, cmd_tx, cx)
}

pub(crate) fn paint_scroller_tree(
    node: &Node,
    path: &str,
    cmd_tx: &mpsc::Sender<Cmd>,
    cx: Option<&App>,
) -> gpui::AnyElement {
    paint_static_tree(node, path, Some(cmd_tx), cx)
}

fn paint_static_tree(
    node: &Node,
    path: &str,
    cmd_tx: Option<&mpsc::Sender<Cmd>>,
    cx: Option<&App>,
) -> gpui::AnyElement {
    if chat::is_chat_kind(&node.kind) {
        return chat::render_any(&mut ChartPaint { cmd_tx, cx }, node, path);
    }
    match node.kind.as_str() {
        "button" => {
            let mut button = Button::new(SharedString::from(path.to_string()));
            if let Some(label) = mapping::jump_button_visible_label(node) {
                button = button.label(label.to_string());
            }
            button = apply_button_chrome(button, node, cx);
            if let (Some(id), Some(tx)) = (node.on_click.clone(), cmd_tx) {
                let tx = tx.clone();
                button = button.on_click(move |_, _, _| {
                    let _ = tx.send(Cmd::Callback {
                        id: id.clone(),
                        value: None,
                        seq: None,
                    });
                });
            }
            button.into_any_element()
        }
        "hstack" => chart_layout(h_flex().gap(px(node.gap.unwrap_or(8.))), node)
            .children(node.children.iter().enumerate().map(|(child_ix, child)| {
                paint_static_tree(child, &static_child_path(path, child_ix), cmd_tx, cx)
            }))
            .into_any_element(),
        "vstack" => chart_layout(v_flex().gap(px(node.gap.unwrap_or(8.))), node)
            .children(node.children.iter().enumerate().map(|(child_ix, child)| {
                paint_static_tree(child, &static_child_path(path, child_ix), cmd_tx, cx)
            }))
            .into_any_element(),
        "spacer" => {
            let mut el = chart_layout(div().id(SharedString::from(path.to_string())), node);
            if node.size.is_none() && node.flex.is_none() {
                el = el.flex_1();
            }
            el.into_any_element()
        }
        "separator" => {
            let mut separator =
                if mapping::parse_axis(node.orientation.as_deref()) == Axis::Vertical {
                    Separator::vertical()
                } else {
                    Separator::horizontal()
                };
            if node.dashed {
                separator = separator.dashed();
            }
            if let Some(label) = node.text.clone().filter(|s| !s.is_empty()) {
                separator = separator.label(label);
            }
            chart_layout(separator, node).into_any_element()
        }
        "spinner" => {
            let spinner = mapping::apply_spinner_chrome(Spinner::new(), node);
            chart_host(spinner, node, path)
        }
        "tag" => {
            let mut tag =
                Tag::new().with_variant(mapping::parse_tag_variant(node.variant.as_deref()));
            if node.outline {
                tag = tag.outline();
            }
            tag = tag.with_size(mapping::parse_scale(node.control_size.as_deref()));
            chart_layout(tag.child(node.text.clone().unwrap_or_default()), node).into_any_element()
        }
        "alert" => {
            let message = node
                .message
                .clone()
                .or_else(|| node.text.clone())
                .unwrap_or_default();
            let id = SharedString::from(path.to_string());
            let mut alert = match node
                .variant
                .as_deref()
                .map(crate::catalog::normalize)
                .as_deref()
            {
                Some("info") => Alert::info(id, message),
                Some("success") => Alert::success(id, message),
                Some("warning") => Alert::warning(id, message),
                Some("error") | Some("danger") => Alert::error(id, message),
                _ => Alert::new(id, message),
            };
            alert = mapping::apply_alert_chrome(alert, node);
            chart_layout(alert, node).into_any_element()
        }
        "skeleton" => chart_layout(mapping::apply_skeleton_chrome(Skeleton::new(), node), node)
            .into_any_element(),
        "kbd" => {
            let text = node.text.clone().unwrap_or_default();
            match mapping::parse_keystroke(&text) {
                Some(stroke) => {
                    chart_layout(mapping::apply_kbd_chrome(Kbd::new(stroke), node), node)
                        .into_any_element()
                }
                None => chart_layout(div().child(text), node).into_any_element(),
            }
        }
        "link" => {
            let mut link = Link::new(SharedString::from(path.to_string()));
            if let Some(href) = node.href.clone().filter(|s| !s.is_empty()) {
                link = link.href(href);
            }
            if node.disabled {
                link = link.disabled(true);
            }
            let label = node
                .text
                .clone()
                .unwrap_or_else(|| node.href.clone().unwrap_or_default());
            chart_layout(link.child(label), node).into_any_element()
        }
        "badge" => {
            let mut badge = mapping::apply_badge_chrome(Badge::new(), node);
            badge = badge.children(node.children.iter().enumerate().map(|(child_ix, child)| {
                paint_static_tree(child, &static_child_path(path, child_ix), cmd_tx, cx)
            }));
            chart_host(badge, &mapping::badge_host_node(node), path)
        }
        "icon" => {
            let name = node.icon.as_deref().or(node.text.as_deref()).unwrap_or("");
            let icon = mapping::parse_icon(name).unwrap_or(IconName::Asterisk);
            chart_layout(Icon::new(icon), node).into_any_element()
        }
        "clipboard" => {
            let clip = Clipboard::new(SharedString::from(path.to_string()))
                .value(node.text.clone().unwrap_or_default());
            chart_host(clip, node, path)
        }
        "breadcrumb" => {
            let items = node.collection();
            let mut crumb = Breadcrumb::new();
            for item in items.iter() {
                crumb =
                    crumb.child(BreadcrumbItem::new(item.label_or_id()).disabled(item.disabled));
            }
            chart_layout(crumb, node).into_any_element()
        }
        "avatar" => chart_layout(kit_avatar(node), node).into_any_element(),
        "avatar-group" => {
            let group = chart_kit_style(kit_avatar_group(node), node);
            chart_outer_style(avatar_group_element(group, node), node).into_any_element()
        }
        "hover-card" => chart_layout(kit_hover_card(node, path, cx), node).into_any_element(),
        "progress" => chart_layout(
            mapping::apply_progress_chrome(
                Progress::new(SharedString::from(path.to_string())),
                node,
            ),
            node,
        )
        .into_any_element(),
        "progress-circle" => {
            let value = node.number_value().filter(|n| n.is_finite()).unwrap_or(0.0);
            let mut circle = ProgressCircle::new(SharedString::from(node_key(node, path)))
                .value(value)
                .loading(node.loading)
                .with_size(mapping::parse_scale(node.control_size.as_deref()));
            if let Some(color) = chart_hex(node.color.as_deref()) {
                circle = circle.color(color);
            }
            if let Some(label) = node
                .accessibility_label
                .as_deref()
                .filter(|s| !s.is_empty())
            {
                circle = circle.accessibility_label(label.to_string());
            }
            chart_layout(
                circle.children(node.children.iter().enumerate().map(|(child_ix, child)| {
                    paint_static_tree(child, &static_child_path(path, child_ix), cmd_tx, cx)
                })),
                node,
            )
            .into_any_element()
        }
        "pagination" => {
            let page = node
                .number_value()
                .filter(|n| n.is_finite())
                .map(|n| n.round().max(0.0) as usize)
                .unwrap_or(1);
            let total = node
                .total
                .filter(|n| n.is_finite())
                .map(|n| n.round().max(0.0) as usize)
                .unwrap_or(1);
            let mut pagination = Pagination::new(SharedString::from(node_key(node, path)))
                .current_page(page)
                .total_pages(total)
                .with_size(mapping::parse_scale(node.control_size.as_deref()))
                .disabled(node.disabled);
            if node.compact {
                pagination = pagination.compact();
            }
            if let Some(visible) = node
                .visible_pages
                .filter(|n| n.is_finite())
                .map(|n| n.round().max(0.0) as usize)
            {
                pagination = pagination.visible_pages(visible);
            }
            chart_layout(pagination, node).into_any_element()
        }
        "shimmer" => {
            let mut shimmer = ShimmerText::new(node.text.clone().unwrap_or_default());
            if let Some(id) = node.id.clone().filter(|s| !s.is_empty()) {
                shimmer = shimmer.id(id);
            }
            if let Some(secs) = node.duration.filter(|n| n.is_finite() && *n >= 0.0) {
                shimmer = shimmer.duration(Duration::from_secs_f32(secs));
            }
            if let Some(width) = node.spread_px.filter(|n| n.is_finite()) {
                shimmer = shimmer.spread(px(width));
            } else if let Some(rel) = node.spread.filter(|n| n.is_finite()) {
                shimmer = shimmer.spread(rel);
            }
            if node.reverse {
                shimmer = shimmer.reverse(true);
            }
            if node.once {
                shimmer = shimmer.once(true);
            }
            if let Some(color) = chart_hex(node.highlight_color.as_deref()) {
                shimmer = shimmer.highlight_color(color);
            }
            chart_layout(shimmer, node).into_any_element()
        }
        "group-box" => {
            let mut box_ = GroupBox::new()
                .id(SharedString::from(path.to_string()))
                .with_variant(mapping::parse_group_variant(node.variant.as_deref()));
            if let Some(title) = node.title.clone() {
                box_ = box_.title(title);
            }
            chart_layout(box_, node)
                .children(node.children.iter().enumerate().map(|(child_ix, child)| {
                    paint_static_tree(child, &static_child_path(path, child_ix), cmd_tx, cx)
                }))
                .into_any_element()
        }
        "label" => chart_layout(mapping::kit_label(node), node).into_any_element(),
        "nav-page" => chart_layout(v_flex().gap(px(node.gap.unwrap_or(8.))), node)
            .children(node.children.iter().enumerate().map(|(child_ix, child)| {
                paint_static_tree(child, &static_child_path(path, child_ix), cmd_tx, cx)
            }))
            .into_any_element(),
        _ if !node.children.is_empty() => {
            chart_layout(v_flex().gap(px(node.gap.unwrap_or(8.))), node)
                .children(node.children.iter().enumerate().map(|(child_ix, child)| {
                    paint_static_tree(child, &static_child_path(path, child_ix), cmd_tx, cx)
                }))
                .into_any_element()
        }
        _ => chart_layout(
            div()
                .id(SharedString::from(path.to_string()))
                .child(node.text.clone().unwrap_or_default()),
            node,
        )
        .into_any_element(),
    }
}

/// Relative path under a `paint_static` prefix (`0`, `0/1`, …).
fn static_rel<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    path.strip_prefix(prefix)?
        .strip_prefix('/')
        .filter(|rel| !rel.is_empty())
}

fn node_at_static_rel<'a>(nodes: &'a [Node], rel: &str) -> Option<&'a Node> {
    let mut current = nodes;
    let mut parts = rel.split('/');
    let last = parts.next_back()?;
    for part in parts {
        let index: usize = part.parse().ok()?;
        let node = current.get(index)?;
        if node.kind != "hstack" && node.kind != "vstack" {
            return None;
        }
        current = node.children.as_slice();
    }
    let index: usize = last.parse().ok()?;
    current.get(index)
}

/// Locate the node `paint_static` would paint at `path`.
///
/// Overlay content uses `{overlay-key}/content/{index}/…`; sheet footers use
/// `{overlay-key}/footer/0/…`. Dock panels use `{dock-key}/panel/{index}/…`.
fn node_at_static_path(tree: &Node, path: &str) -> Option<Node> {
    let mut found = None;
    walk_nodes(tree, "root", &mut |node, walk_path| {
        if found.is_some() {
            return;
        }
        let key = node_key(node, walk_path);
        match node.kind.as_str() {
            "dialog" | "alert-dialog" | "popover" | "sheet" => {
                if let Some(rel) = static_rel(path, &format!("{key}/content")) {
                    found = node_at_static_rel(&node.children, rel).cloned();
                } else if node.kind == "sheet" {
                    if let Some(footer) = node.footer.as_deref() {
                        if let Some(rel) = static_rel(path, &format!("{key}/footer")) {
                            found = node_at_static_rel(std::slice::from_ref(footer), rel).cloned();
                        }
                    }
                }
            }
            "dock" => {
                for (index, item) in node.collection().iter().enumerate() {
                    let Some(content) = item.content.as_deref() else {
                        continue;
                    };
                    let Some(rel) = static_rel(path, &format!("{key}/panel/{index}")) else {
                        continue;
                    };
                    let nodes = if content.children.is_empty() {
                        std::slice::from_ref(content)
                    } else {
                        content.children.as_slice()
                    };
                    found = node_at_static_rel(nodes, rel).cloned();
                    if found.is_some() {
                        break;
                    }
                }
            }
            _ => {}
        }
    });
    found
}

fn paint_static_node(
    node: &Node,
    path: &str,
    emit: ActionEmitter,
    cx: Option<&App>,
) -> gpui::AnyElement {
    match node.kind.as_str() {
        "button" => {
            let mut button = Button::new(SharedString::from(path.to_string()));
            if let Some(label) = mapping::jump_button_visible_label(node) {
                button = button.label(label.to_string());
            }
            button = apply_button_chrome(button, node, cx);
            if node.on_click.is_some() {
                let emit = emit.clone();
                let key = path.to_string();
                button = button.on_click(move |_, _, cx| {
                    emit(QueuedAction::ButtonClick { key: key.clone() }, cx);
                });
            }
            button.into_any_element()
        }
        "hstack" => h_flex()
            .gap(px(node.gap.unwrap_or(8.)))
            .children(node.children.iter().enumerate().map(|(child_ix, child)| {
                paint_static_node(child, &static_child_path(path, child_ix), emit.clone(), cx)
            }))
            .into_any_element(),
        "vstack" => v_flex()
            .gap(px(node.gap.unwrap_or(8.)))
            .children(node.children.iter().enumerate().map(|(child_ix, child)| {
                paint_static_node(child, &static_child_path(path, child_ix), emit.clone(), cx)
            }))
            .into_any_element(),
        "separator" => gpui_component::separator::Separator::horizontal().into_any_element(),
        "icon" => {
            let name = node.icon.as_deref().or(node.text.as_deref()).unwrap_or("");
            let icon = mapping::parse_icon(name).unwrap_or(IconName::Asterisk);
            Icon::new(icon).into_any_element()
        }
        _ => div()
            .id(SharedString::from(path.to_string()))
            .child(node.text.clone().unwrap_or_default())
            .into_any_element(),
    }
}

pub(crate) fn apply_button_chrome(button: Button, node: &Node, cx: Option<&App>) -> Button {
    mapping::apply_button_chrome(button, node, cx)
}

pub(crate) fn apply_dropdown_button_chrome(
    mut dropdown: DropdownButton,
    node: &Node,
    cx: Option<&App>,
) -> DropdownButton {
    let chrome = mapping::button_chrome(
        node.variant.as_deref(),
        node.primary,
        node.outline,
        node.selected,
        node.control_size.as_deref(),
    );
    dropdown = mapping::apply_named_button_variant(dropdown, chrome.variant);
    dropdown = mapping::apply_custom_button_variant(dropdown, node, cx);
    if chrome.outline {
        dropdown = dropdown.outline();
    }
    if chrome.selected {
        dropdown = dropdown.selected(true);
    }
    if let Some(size) = chrome.size {
        dropdown = dropdown.with_size(size);
    }
    if node.disabled {
        dropdown = dropdown.disabled(true);
    }
    dropdown
}

/// Build a `Button` trigger for popover / dropdown-menu. Triggers must be
/// `Selectable + IntoElement`; `AnyElement` does not qualify.
pub fn trigger_button(node: Option<&Node>, key: &str, cx: Option<&App>) -> Button {
    let button = Button::new(SharedString::from(format!("{key}-trigger")));
    let Some(n) = node else {
        return button.label("Open");
    };
    let mut button = apply_button_chrome(button, n, cx);
    if let Some(label) = n
        .text
        .as_deref()
        .or(n.title.as_deref())
        .filter(|s| !s.is_empty())
    {
        button = button.label(label.to_string());
    } else if n.icon.is_none() {
        button = button.label("Open");
    }
    button
}

/// Apply a dialog builder to a crate `Dialog` using the latest spec.
pub fn configure_dialog(
    mut dialog: gpui_component::dialog::Dialog,
    node: &Node,
    children: Vec<gpui::AnyElement>,
) -> gpui_component::dialog::Dialog {
    if let Some(title) = node.title.clone() {
        dialog = dialog.title(title);
    }
    let confirm = matches!(
        node.variant
            .as_deref()
            .map(crate::catalog::normalize)
            .as_deref(),
        Some("confirm")
    );
    dialog = dialog.button_props(mapping::dialog_button_props(node, confirm));
    if confirm {
        if let Some(closable) = node.overlay_closable {
            dialog = dialog.overlay_closable(closable);
        }
    } else {
        dialog = dialog.overlay_closable(overlay_closable(node));
    }
    if let Some(close) = node.close_button {
        dialog = dialog.close_button(close);
    }
    if let Some(keyboard) = node.keyboard {
        dialog = dialog.keyboard(keyboard);
    }
    if let Some(width) = node.width {
        dialog = dialog.width(px(width));
    }
    dialog.extend(children);
    dialog
}

/// Apply an alert-dialog builder. Backdrop dismiss is never enabled.
pub fn configure_alert_dialog(
    mut alert: AlertDialog,
    node: &Node,
    children: Vec<gpui::AnyElement>,
) -> AlertDialog {
    if let Some(title) = node.title.clone() {
        alert = alert.title(title);
    }
    if let Some(text) = node
        .message
        .clone()
        .or_else(|| node.text.clone())
        .filter(|s| !s.is_empty())
    {
        alert = alert.description(text);
    }
    let confirm = node
        .variant
        .as_deref()
        .map(crate::catalog::normalize)
        .as_deref()
        == Some("confirm");
    alert = alert.button_props(mapping::dialog_button_props(node, confirm));
    if let Some(close) = node.close_button {
        alert = alert.close_button(close);
    }
    if let Some(keyboard) = node.keyboard {
        alert = alert.keyboard(keyboard);
    }
    if let Some(width) = node.width {
        alert = alert.width(px(width));
    }
    alert.extend(children);
    alert
}

/// Click-outside (and Escape) dismiss. Omitted means true.
pub fn overlay_closable(node: &Node) -> bool {
    node.overlay_closable.unwrap_or(true)
}

/// Crate closed the dialog (overlay / Escape / X) while Clojure still has
/// `:open? true`. Re-opening on the next `RootView::render` would make
/// dismiss look like a no-op until the callback tree arrives.
pub fn crate_dismiss_waiting_for_clojure(
    wanted_keys: &[String],
    dialog_keys: &[String],
    crate_open: bool,
) -> bool {
    !crate_open && !wanted_keys.is_empty() && wanted_keys == dialog_keys
}

/// Acknowledge closed dialogs on every installed tree, even if that tree is
/// superseded before paint. Otherwise a quick close -> reopen can leave the
/// same key waiting forever for a closed state that the renderer skipped.
pub fn acknowledge_dialog_tree(dialog_keys: &mut Vec<String>, tree: &Node) {
    let open = collect_open_dialogs(tree);
    dialog_keys.retain(|key| open.iter().any(|spec| &spec.key == key));
}

/// Binding Kit 0.6 Dialog callbacks. The crate itself sequences them:
///
/// * OK: `on_ok` (false keeps the dialog open), then `on_close`
/// * Cancel, Escape, close button, overlay click: `on_cancel`, then `on_close`
///
/// Those closures run synchronously for one user action. Record the action
/// and emit once from `on_close`; its current ids form one CallbackBatch.
pub fn bind_dialog_callbacks(
    dialog: gpui_component::dialog::Dialog,
    key: String,
    emit: ActionEmitter,
    close: Rc<RefCell<DialogClose>>,
) -> gpui_component::dialog::Dialog {
    dialog
        .on_ok({
            let close = close.clone();
            move |_, _, _| close.borrow_mut().action(true)
        })
        .on_cancel({
            let close = close.clone();
            move |_, _, _| close.borrow_mut().action(false)
        })
        .on_close(move |_, _, cx| {
            if let Some(action) = close.borrow_mut().take(&key) {
                emit(action, cx);
            }
        })
}

pub fn bind_alert_dialog_callbacks(
    alert: AlertDialog,
    key: String,
    emit: ActionEmitter,
    close: Rc<RefCell<DialogClose>>,
) -> AlertDialog {
    alert
        .on_ok({
            let close = close.clone();
            move |_, _, _| close.borrow_mut().action(true)
        })
        .on_cancel({
            let close = close.clone();
            move |_, _, _| close.borrow_mut().action(false)
        })
        .on_close(move |_, _, cx| {
            if let Some(action) = close.borrow_mut().take(&key) {
                emit(action, cx);
            }
        })
}

#[derive(Clone, Debug)]
pub struct SheetSpec {
    pub key: String,
    pub node: Node,
}

/// Crate `Root` holds one sheet. Last open sheet in tree order wins.
pub fn collect_open_sheet(root: &Node) -> Option<SheetSpec> {
    let mut found = None;
    walk_nodes(root, "root", &mut |node, path| {
        if node.kind == "sheet" && node.open.unwrap_or(false) {
            found = Some(SheetSpec {
                key: node_key(node, path),
                node: node.clone(),
            });
        }
    });
    found
}

pub fn latest_sheet_spec(live: &RefCell<Option<SheetSpec>>, key: &str) -> Option<SheetSpec> {
    live.borrow()
        .as_ref()
        .filter(|spec| spec.key == key)
        .cloned()
}

pub fn configure_sheet(
    mut sheet: gpui_component::sheet::Sheet,
    node: &Node,
    children: Vec<gpui::AnyElement>,
    footer: Option<gpui::AnyElement>,
) -> gpui_component::sheet::Sheet {
    if let Some(title) = node.title.clone() {
        sheet = sheet.title(title);
    }
    sheet = sheet.overlay_closable(overlay_closable(node));
    sheet = sheet.overlay(node.overlay.unwrap_or(true));
    sheet = sheet.resizable(node.resizable.unwrap_or(true));
    let size = node.size.or(node.width).or(node.height).unwrap_or(350.0);
    sheet = sheet.size(px(size));
    if let Some(footer) = footer {
        sheet = sheet.footer(footer);
    }
    sheet.extend(children);
    sheet
}

pub fn bind_sheet_callbacks(
    sheet: gpui_component::sheet::Sheet,
    node: &Node,
    cmd_tx: mpsc::Sender<Cmd>,
) -> gpui_component::sheet::Sheet {
    let on_close = node.on_close.clone();
    let on_open_change = node.on_open_change.clone();
    sheet.on_close(move |_, _, _| {
        let mut calls = Vec::new();
        if let Some(id) = on_close.clone() {
            calls.push(protocol::CallbackCall::fire(id));
        }
        if let Some(id) = on_open_change.clone() {
            calls.push(protocol::CallbackCall::with_value(id, json!(false)));
        }
        protocol::send_callbacks(&cmd_tx, calls);
    })
}

#[derive(Clone, Debug)]
pub struct NotificationSpec {
    pub key: String,
    pub node: Node,
}

/// Presence in the tree means show, unless `:open` is explicitly false.
pub fn collect_notifications(root: &Node) -> Vec<NotificationSpec> {
    let mut out = Vec::new();
    walk_nodes(root, "root", &mut |node, path| {
        if node.kind == "notification" && node.open.unwrap_or(true) {
            out.push(NotificationSpec {
                key: node_key(node, path),
                node: node.clone(),
            });
        }
    });
    out
}

pub fn notification_fingerprint(node: &Node) -> String {
    format!(
        "{}|{}|{}|{:?}|{}|{}",
        node.title.as_deref().unwrap_or(""),
        node.message
            .as_deref()
            .or(node.text.as_deref())
            .unwrap_or(""),
        node.variant.as_deref().unwrap_or(""),
        node.autohide,
        node.icon.as_deref().unwrap_or(""),
        node.placement.as_deref().unwrap_or("")
    )
}

/// Current `:on-click` for a still-mounted notification.
///
/// Fingerprint ignores callback ids, so an unchanged toast stays mounted
/// across `export-tree`. The crate `on_click` closure must call this at
/// click time (keyed by notification id) instead of capturing `cb-N`.
pub fn live_notification_click(
    slots: &HashMap<String, Option<String>>,
    key: &str,
) -> Option<String> {
    slots.get(key).cloned().flatten()
}

pub fn notification_autohide(node: &Node) -> bool {
    node.autohide.unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_bridge::{CommandEchoLatch, should_apply_command_echo};
    use serde_json::json;
    use std::collections::HashMap;

    fn node(value: serde_json::Value) -> Node {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn collect_skips_closed_and_nested_open_dialogs() {
        let tree = node(json!({
            "type": "window",
            "children": [
                {"type": "dialog", "open": false, "title": "Hidden", "id": "hidden"},
                {
                    "type": "vstack",
                    "children": [
                        {
                            "type": "dialog",
                            "open": true,
                            "id": "ask",
                            "title": "Ask",
                            "children": [{"type": "label", "text": "Really?"}]
                        }
                    ]
                }
            ]
        }));
        let specs = collect_open_dialogs(&tree);
        assert_eq!(dialog_keys(&specs), vec!["ask".to_string()]);
        assert!(specs[0].node.contains_text("Really?"));
    }

    #[test]
    fn collect_uses_path_when_id_is_missing() {
        let tree = node(json!({
            "type": "window",
            "children": [
                {"type": "dialog", "open": true, "title": "A"},
                {"type": "dialog", "open": true, "title": "B"}
            ]
        }));
        let keys = dialog_keys(&collect_open_dialogs(&tree));
        assert_eq!(keys, vec!["root-0".to_string(), "root-1".to_string()]);
    }

    #[test]
    fn separator_items_are_detected() {
        let sep: Item = serde_json::from_value(json!({"separator": true})).unwrap();
        let dash: Item = serde_json::from_value(json!({"id": "-"})).unwrap();
        let copy: Item = serde_json::from_value(json!({"id": "copy", "label": "Copy"})).unwrap();
        assert!(sep.is_separator());
        assert!(dash.is_separator());
        assert!(!copy.is_separator());
    }

    #[test]
    fn overlay_closable_defaults_true_and_can_opt_out() {
        let omitted = node(json!({"type": "dialog", "open": true}));
        let off = node(json!({"type": "dialog", "open": true, "overlay-closable": false}));
        assert!(overlay_closable(&omitted));
        assert!(!overlay_closable(&off));
    }

    #[test]
    fn crate_dismiss_waits_when_keys_still_match() {
        let keys = vec!["ask".to_string()];
        assert!(crate_dismiss_waiting_for_clojure(&keys, &keys, false));
        assert!(!crate_dismiss_waiting_for_clojure(&keys, &[], false));
        assert!(!crate_dismiss_waiting_for_clojure(&keys, &keys, true));
        assert!(!crate_dismiss_waiting_for_clojure(&[], &keys, false));
    }

    #[test]
    fn live_dialog_spec_uses_latest_callback_generation() {
        let live = RefCell::new(vec![DialogSpec {
            key: "ask".into(),
            node: node(json!({
                "type": "dialog",
                "open": true,
                "on-ok": "cb-7",
                "on-close": "cb-8"
            })),
        }]);
        assert_eq!(
            latest_dialog_spec(&live, "ask").and_then(|spec| spec.node.on_ok.clone()),
            Some("cb-7".into())
        );
        live.borrow_mut()[0].node.on_ok = Some("cb-19".into());
        live.borrow_mut()[0].node.on_close = Some("cb-20".into());
        let spec = latest_dialog_spec(&live, "ask").unwrap();
        assert_eq!(spec.node.on_ok.as_deref(), Some("cb-19"));
        assert_eq!(spec.node.on_close.as_deref(), Some("cb-20"));
        assert!(latest_dialog_spec(&live, "other").is_none());
    }

    #[test]
    fn static_child_paths_are_unique_across_nested_stacks() {
        let a = static_child_path("ask/content", 0);
        let nested_a = static_child_path(&a, 0);
        let nested_b = static_child_path(&a, 1);
        let sibling = static_child_path("ask/content", 1);
        let sibling_child = static_child_path(&sibling, 0);
        let ids = [a, nested_a, nested_b, sibling, sibling_child];
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn collect_open_sheet_takes_the_last_open() {
        let tree = node(json!({
            "type": "window",
            "children": [
                {"type": "sheet", "open": false, "id": "hidden", "title": "No"},
                {"type": "sheet", "open": true, "id": "first", "title": "A"},
                {"type": "sheet", "open": true, "id": "second", "title": "B"}
            ]
        }));
        let spec = collect_open_sheet(&tree).unwrap();
        assert_eq!(spec.key, "second");
        assert!(spec.node.contains_text("B"));
    }

    #[test]
    fn notification_fingerprint_ignores_callback_ids() {
        let first = node(json!({
            "type": "notification",
            "id": "saved",
            "title": "Saved",
            "message": "ok",
            "autohide": false,
            "on-click": "cb-10",
            "on-close": "cb-11"
        }));
        let later = node(json!({
            "type": "notification",
            "id": "saved",
            "title": "Saved",
            "message": "ok",
            "autohide": false,
            "on-click": "cb-44",
            "on-close": "cb-45"
        }));
        assert_eq!(
            notification_fingerprint(&first),
            notification_fingerprint(&later)
        );
        let moved = node(json!({
            "type": "notification",
            "id": "saved",
            "title": "Saved",
            "message": "ok",
            "autohide": false,
            "placement": "bottom-right",
            "icon": "bell"
        }));
        assert_ne!(
            notification_fingerprint(&first),
            notification_fingerprint(&moved)
        );
    }

    #[test]
    fn notification_click_uses_current_id_after_export() {
        let mut slots = HashMap::new();
        slots.insert("saved".into(), Some("cb-10".into()));
        assert_eq!(
            live_notification_click(&slots, "saved").as_deref(),
            Some("cb-10")
        );
        slots.insert("saved".into(), Some("cb-44".into()));
        assert_eq!(
            live_notification_click(&slots, "saved").as_deref(),
            Some("cb-44")
        );
        assert_ne!(
            live_notification_click(&slots, "saved").as_deref(),
            Some("cb-10")
        );
        slots.insert("saved".into(), None);
        assert_eq!(live_notification_click(&slots, "saved"), None);
    }

    #[test]
    fn collect_notifications_skips_explicitly_closed() {
        let tree = node(json!({
            "type": "window",
            "children": [
                {"type": "notification", "id": "ok", "message": "Saved"},
                {"type": "notification", "id": "gone", "open": false, "message": "Hide"}
            ]
        }));
        let notes = collect_notifications(&tree);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].key, "ok");
        assert!(notification_autohide(&notes[0].node));
    }

    #[test]
    fn collect_open_dialogs_includes_alert_dialog() {
        let tree = node(json!({
            "type": "window",
            "children": [
                {"type": "alert-dialog", "id": "ask", "open": true, "title": "Alert"}
            ]
        }));
        assert_eq!(
            dialog_keys(&collect_open_dialogs(&tree)),
            vec!["ask".to_string()]
        );
    }

    #[test]
    fn alert_dialog_close_and_body_button_resolve() {
        let tree = node(json!({"type": "window", "children": [{
            "type": "alert-dialog", "id": "ask", "open": true,
            "on-ok": "cb-ok", "on-cancel": "cb-cancel",
            "on-close": "cb-close", "on-open-change": "cb-open",
            "children": [
                {"type": "label", "text": "Undo?"},
                {"type": "button", "text": "Retry", "on-click": "cb-retry"}
            ]
        }]}));
        let ids = |calls: Vec<protocol::CallbackCall>| {
            calls.into_iter().map(|call| call.id).collect::<Vec<_>>()
        };
        let mut queue = CallbackQueue::default();
        queue.push(QueuedAction::DialogClose {
            key: "ask".into(),
            ok: Some(true),
        });
        assert_eq!(
            ids(queue.next(&tree).unwrap()),
            vec!["cb-ok", "cb-close", "cb-open"]
        );
        queue.push(QueuedAction::DialogClose {
            key: "ask".into(),
            ok: Some(false),
        });
        assert_eq!(
            ids(queue.next(&tree).unwrap()),
            vec!["cb-cancel", "cb-close", "cb-open"]
        );
        queue.push(QueuedAction::DialogClose {
            key: "ask".into(),
            ok: None,
        });
        assert_eq!(ids(queue.next(&tree).unwrap()), vec!["cb-close", "cb-open"]);
        queue.push(QueuedAction::ButtonClick {
            key: "ask/content/1".into(),
        });
        assert_eq!(ids(queue.next(&tree).unwrap()), vec!["cb-retry"]);
    }

    #[test]
    fn dropdown_button_menu_and_trigger_resolve() {
        let tree = node(json!({"type": "window", "children": [{
            "type": "dropdown-button",
            "id": "export",
            "on-change": "cb-menu",
            "items": [{"id": "csv", "label": "CSV", "on-click": "cb-csv"}],
            "trigger": {"type": "button", "text": "Export", "on-click": "cb-action"}
        }]}));
        let ids = |calls: Vec<protocol::CallbackCall>| {
            calls.into_iter().map(|call| call.id).collect::<Vec<_>>()
        };
        let mut queue = CallbackQueue::default();
        queue.push(QueuedAction::MenuSelect {
            key: "export".into(),
            item_path: vec!["csv".into()],
        });
        assert_eq!(ids(queue.next(&tree).unwrap()), vec!["cb-csv", "cb-menu"]);
        queue.push(QueuedAction::ButtonClick {
            key: "root-0-trigger".into(),
        });
        assert_eq!(ids(queue.next(&tree).unwrap()), vec!["cb-action"]);
    }

    #[test]
    fn clj_select_resolves_live_native_menu_and_command_callbacks() {
        let tree = node(json!({"type": "window", "children": [
            {"type": "native-menu", "id": "edit-menu", "open": true,
             "on-change": "cb-menu", "on-open-change": "cb-open",
             "items": [
                {"id": "copy", "label": "Copy", "on-click": "cb-copy"},
                {"id": "share", "label": "Share", "items": [
                    {"id": "link", "label": "Copy link", "on-click": "cb-link"}
                ]}
             ]},
            {"type": "command", "id": "palette", "on-change": "cb-cmd",
             "on-query": "cb-query", "on-cancel": "cb-cancel",
             "on-select": "cb-sel", "on-confirm": "cb-confirm",
             "items": [
                {"label": "Edit", "items": [{"id": "find", "label": "Find"}]}
             ]}
        ]}));
        let ids = |calls: Vec<protocol::CallbackCall>| {
            calls.into_iter().map(|call| call.id).collect::<Vec<_>>()
        };
        let mut queue = CallbackQueue::default();
        queue.push(QueuedAction::CljSelect {
            key: "edit-menu".into(),
            item_path: vec!["copy".into()],
        });
        let copy = queue.next(&tree).unwrap();
        assert_eq!(ids(copy.clone()), vec!["cb-copy", "cb-menu"]);
        assert_eq!(copy[1].value, Some(json!("copy")));
        queue.push(QueuedAction::CljSelect {
            key: "edit-menu".into(),
            item_path: vec!["share".into(), "link".into()],
        });
        let link = queue.next(&tree).unwrap();
        assert_eq!(ids(link.clone()), vec!["cb-link", "cb-menu"]);
        assert_eq!(link[1].value, Some(json!(["share", "link"])));
        queue.push(QueuedAction::CljSelect {
            key: "palette".into(),
            item_path: vec!["Edit".into(), "find".into()],
        });
        let cmd = queue.next(&tree).unwrap();
        assert_eq!(ids(cmd.clone()), vec!["cb-cmd"]);
        assert_eq!(cmd[0].value, Some(json!(["Edit", "find"])));
        queue.push(QueuedAction::CommandSelect {
            key: "palette".into(),
            item_path: vec!["Edit".into(), "find".into()],
        });
        let sel = queue.next(&tree).unwrap();
        assert_eq!(ids(sel.clone()), vec!["cb-sel"]);
        assert_eq!(sel[0].value, Some(json!(["Edit", "find"])));
        queue.push(QueuedAction::CljSelect {
            key: "palette".into(),
            item_path: vec!["Edit".into(), "find".into()],
        });
        queue.push(QueuedAction::CommandConfirm {
            key: "palette".into(),
            item_path: vec!["Edit".into(), "find".into()],
        });
        let confirm = queue.next(&tree).unwrap();
        assert_eq!(ids(confirm.clone()), vec!["cb-cmd", "cb-confirm"]);
        assert_eq!(confirm[0].value, Some(json!(["Edit", "find"])));
        assert_eq!(confirm[1].value, Some(json!(["Edit", "find"])));
        queue.push(QueuedAction::CommandQuery {
            key: "palette".into(),
            query: "fi".into(),
        });
        let query = queue.next(&tree).unwrap();
        assert_eq!(query[0].id, "cb-query");
        assert_eq!(query[0].value, Some(json!("fi")));
        queue.push(QueuedAction::CommandCancel {
            key: "palette".into(),
        });
        assert_eq!(ids(queue.next(&tree).unwrap()), vec!["cb-cancel"]);
        queue.push(QueuedAction::PopoverOpen {
            key: "edit-menu".into(),
            open: false,
        });
        let close = queue.next(&tree).unwrap();
        assert_eq!(close[0].id, "cb-open");
        assert_eq!(close[0].value, Some(json!(false)));
    }

    #[test]
    fn clj_select_skips_disabled_leaves() {
        let tree = node(json!({"type": "window", "children": [{
            "type": "native-menu", "id": "edit-menu", "on-change": "cb-menu",
            "items": [{"id": "copy", "label": "Copy", "disabled": true, "on-click": "cb-copy"}]
        }]}));
        let mut queue = CallbackQueue::default();
        queue.push(QueuedAction::CljSelect {
            key: "edit-menu".into(),
            item_path: vec!["copy".into()],
        });
        assert!(queue.next(&tree).is_none());
    }

    #[test]
    fn clj_select_uses_exact_path_when_leaf_ids_collide() {
        let tree = node(json!({"type": "window", "children": [
            {"type": "native-menu", "id": "os-menu", "on-change": "cb-os",
             "items": [
                {"id": "file", "label": "File", "items": [
                    {"id": "open", "label": "Open file", "on-click": "cb-file-open"}
                ]},
                {"id": "project", "label": "Project", "items": [
                    {"id": "open", "label": "Open project", "on-click": "cb-project-open"}
                ]}
             ]},
            {"type": "command", "id": "palette", "on-change": "cb-cmd",
             "on-select": "cb-sel", "on-confirm": "cb-confirm",
             "items": [
                {"id": "file", "label": "File", "items": [
                    {"id": "open", "label": "Open file", "on-click": "cb-file-cmd"}
                ]},
                {"id": "project", "label": "Project", "items": [
                    {"id": "open", "label": "Open project", "on-click": "cb-project-cmd"}
                ]}
             ]}
        ]}));
        let ids = |calls: Vec<protocol::CallbackCall>| {
            calls.into_iter().map(|call| call.id).collect::<Vec<_>>()
        };
        let mut queue = CallbackQueue::default();
        queue.push(QueuedAction::CljSelect {
            key: "os-menu".into(),
            item_path: vec!["project".into(), "open".into()],
        });
        let os_project = queue.next(&tree).unwrap();
        assert_eq!(ids(os_project.clone()), vec!["cb-project-open", "cb-os"]);
        assert_eq!(os_project[1].value, Some(json!(["project", "open"])));
        queue.push(QueuedAction::CljSelect {
            key: "os-menu".into(),
            item_path: vec!["file".into(), "open".into()],
        });
        let os_file = queue.next(&tree).unwrap();
        assert_eq!(ids(os_file.clone()), vec!["cb-file-open", "cb-os"]);
        assert_eq!(os_file[1].value, Some(json!(["file", "open"])));
        queue.push(QueuedAction::CljSelect {
            key: "palette".into(),
            item_path: vec!["project".into(), "open".into()],
        });
        let cmd_project = queue.next(&tree).unwrap();
        assert_eq!(ids(cmd_project.clone()), vec!["cb-project-cmd", "cb-cmd"]);
        assert_eq!(cmd_project[1].value, Some(json!(["project", "open"])));
        queue.push(QueuedAction::CljSelect {
            key: "palette".into(),
            item_path: vec!["file".into(), "open".into()],
        });
        let cmd_file = queue.next(&tree).unwrap();
        assert_eq!(ids(cmd_file.clone()), vec!["cb-file-cmd", "cb-cmd"]);
        assert_eq!(cmd_file[1].value, Some(json!(["file", "open"])));
        queue.push(QueuedAction::CommandSelect {
            key: "palette".into(),
            item_path: vec!["project".into(), "open".into()],
        });
        let sel = queue.next(&tree).unwrap();
        assert_eq!(ids(sel.clone()), vec!["cb-sel"]);
        assert_eq!(sel[0].value, Some(json!(["project", "open"])));
        queue.push(QueuedAction::CommandConfirm {
            key: "palette".into(),
            item_path: vec!["project".into(), "open".into()],
        });
        let confirm = queue.next(&tree).unwrap();
        assert_eq!(ids(confirm.clone()), vec!["cb-confirm"]);
        assert_eq!(confirm[0].value, Some(json!(["project", "open"])));
        queue.push(QueuedAction::CljSelect {
            key: "os-menu".into(),
            item_path: vec!["open".into()],
        });
        assert!(
            queue.next(&tree).is_none(),
            "a one-element path must not depth-first the first duplicate leaf"
        );
    }

    #[test]
    fn command_select_without_on_select_emits_nothing() {
        let tree = node(json!({"type": "window", "children": [{
            "type": "command", "id": "palette",
            "items": [
                {"id": "file", "label": "File", "items": [
                    {"id": "open", "label": "Open file"}
                ]},
                {"id": "project", "label": "Project", "items": [
                    {"id": "open", "label": "Open project"}
                ]}
            ]
        }]}));
        let mut queue = CallbackQueue::default();
        queue.push(QueuedAction::CommandSelect {
            key: "palette".into(),
            item_path: vec!["project".into(), "open".into()],
        });
        assert!(
            queue.next(&tree).is_none(),
            "no Clojure :on-select means no callback and no echo to wait for"
        );
    }

    #[test]
    fn queued_command_query_echo_binds_when_the_blocked_batch_is_sent() {
        let tree = node(json!({"type": "window", "children": [{
            "type": "command", "id": "palette",
            "on-query": "cb-query",
            "items": [{"id": "find", "label": "Find"}]
        }]}));
        let mut queue = CallbackQueue::default();
        queue.push(QueuedAction::CommandQuery {
            key: "palette".into(),
            query: "f".into(),
        });
        let first = queue.next_outbound(&tree).unwrap();
        assert_eq!(first.calls[0].value, Some(json!("f")));
        assert_eq!(
            first.command_echo,
            Some(CommandEcho::Query {
                key: "palette".into(),
                query: "f".into(),
            })
        );
        queue.sent(7);

        queue.push(QueuedAction::CommandQuery {
            key: "palette".into(),
            query: "fi".into(),
        });
        assert!(
            queue.next_outbound(&tree).is_none(),
            "an in-flight callback batch must not send the later query"
        );

        queue.tree_installed(Some(7));
        let second = queue.next_outbound(&tree).unwrap();
        assert_eq!(second.calls[0].value, Some(json!("fi")));
        assert_eq!(
            second.command_echo,
            Some(CommandEcho::Query {
                key: "palette".into(),
                query: "fi".into(),
            }),
            "the delayed flush must still describe the query that was actually sent"
        );
        queue.sent(8);

        let latch_a = CommandEchoLatch {
            seq: 7,
            value: "f".to_string(),
        };
        let latch_b = CommandEchoLatch {
            seq: 8,
            value: "fi".to_string(),
        };
        assert!(
            should_apply_command_echo(Some(&latch_a), Some(&"fi".to_string()), Some(7)),
            "keeping A's latch would let seq 7 overwrite the later native query"
        );
        assert!(
            !should_apply_command_echo(Some(&latch_b), Some(&"fi".to_string()), Some(7)),
            "B's latch must protect native fi from A's tree"
        );
        assert!(
            should_apply_command_echo(Some(&latch_b), Some(&"fi".to_string()), Some(8)),
            "B's matching tree is then authoritative"
        );
        assert!(should_apply_command_echo(
            Some(&latch_b),
            Some(&"find".to_string()),
            Some(8)
        ));
    }

    #[test]
    fn queued_command_select_echo_binds_when_the_blocked_batch_is_sent() {
        let tree = node(json!({"type": "window", "children": [{
            "type": "command", "id": "palette",
            "on-select": "cb-sel",
            "items": [
                {"id": "file", "label": "File", "items": [
                    {"id": "open", "label": "Open file"}
                ]},
                {"id": "project", "label": "Project", "items": [
                    {"id": "open", "label": "Open project"}
                ]}
            ]
        }]}));
        let file = vec!["file".to_string(), "open".to_string()];
        let project = vec!["project".to_string(), "open".to_string()];
        let mut queue = CallbackQueue::default();
        queue.push(QueuedAction::CommandSelect {
            key: "palette".into(),
            item_path: file.clone(),
        });
        let first = queue.next_outbound(&tree).unwrap();
        assert_eq!(first.calls[0].value, Some(json!(["file", "open"])));
        assert_eq!(
            first.command_echo,
            Some(CommandEcho::Select {
                key: "palette".into(),
                item_path: file.clone(),
            })
        );
        queue.sent(7);

        queue.push(QueuedAction::CommandSelect {
            key: "palette".into(),
            item_path: project.clone(),
        });
        assert!(
            queue.next_outbound(&tree).is_none(),
            "an in-flight callback batch must not send the later highlight"
        );

        queue.tree_installed(Some(7));
        let second = queue.next_outbound(&tree).unwrap();
        assert_eq!(second.calls[0].value, Some(json!(["project", "open"])));
        assert_eq!(
            second.command_echo,
            Some(CommandEcho::Select {
                key: "palette".into(),
                item_path: project.clone(),
            }),
            "the delayed flush must still describe the highlight that was actually sent"
        );
        queue.sent(8);

        let latch_a = CommandEchoLatch {
            seq: 7,
            value: file.clone(),
        };
        let latch_b = CommandEchoLatch {
            seq: 8,
            value: project.clone(),
        };
        assert!(
            should_apply_command_echo(Some(&latch_a), Some(&project), Some(7)),
            "keeping A's latch would let seq 7 overwrite the later native highlight"
        );
        assert!(
            !should_apply_command_echo(Some(&latch_b), Some(&project), Some(7)),
            "B's latch must protect native project/open from A's tree"
        );
        assert!(
            should_apply_command_echo(Some(&latch_b), Some(&project), Some(8)),
            "B's matching tree is then authoritative"
        );
        assert!(
            should_apply_command_echo(Some(&latch_b), Some(&file), Some(8)),
            "the matching seq may echo, reject, or replace the highlight"
        );
    }

    #[test]
    fn collect_native_menus_includes_closed() {
        let tree = node(json!({
            "type": "window",
            "children": [
                {"type": "native-menu", "id": "edit", "open": false,
                 "items": [{"id": "copy", "label": "Copy"}]},
                {"type": "native-menu", "open": true,
                 "items": [{"id": "paste", "label": "Paste"}]}
            ]
        }));
        let menus = collect_native_menus(&tree);
        assert_eq!(menus.len(), 2);
        assert_eq!(menus[0].0, "edit");
        assert!(!menus[0].1.open.unwrap_or(false));
        assert_eq!(menus[1].0, "root-1");
        assert!(menus[1].1.open.unwrap_or(false));
    }

    #[test]
    fn queued_static_overlay_buttons_resolve_content_and_footer_paths() {
        let tree = node(json!({"type": "window", "children": [
            {"type": "dialog", "id": "ask", "open": true, "children": [
                {"type": "label", "text": "body"},
                {"type": "button", "text": "Save", "on-click": "cb-dialog"}
            ]},
            {"type": "popover", "id": "hint", "open": true, "children": [
                {"type": "hstack", "children": [
                    {"type": "button", "on-click": "cb-nested-a"},
                    {"type": "button", "on-click": "cb-nested-b"}
                ]}
            ]},
            {"type": "sheet", "id": "inspect", "open": true,
             "children": [{"type": "button", "text": "Ping", "on-click": "cb-sheet"}],
             "footer": {"type": "button", "text": "Done", "on-click": "cb-footer"}}
        ]}));
        for (key, expected) in [
            ("ask/content/1", "cb-dialog"),
            ("hint/content/0/1", "cb-nested-b"),
            ("inspect/content/0", "cb-sheet"),
            ("inspect/footer/0", "cb-footer"),
        ] {
            let mut queue = CallbackQueue::default();
            queue.push(QueuedAction::ButtonClick { key: key.into() });
            assert_eq!(queue.next(&tree).unwrap()[0].id, expected, "{key}");
        }
    }

    #[test]
    fn paint_chart_label_accepts_badge_and_avatar() {
        let badge = node(json!({
            "type": "badge",
            "count": 3,
            "children": [{"type": "label", "text": "N"}]
        }));
        let _ = paint_chart_label(&badge, "radar-label/0");
        let avatar = node(json!({
            "type": "avatar",
            "text": "AB",
            "src": "https://example.com/ab.png"
        }));
        let _ = paint_chart_label(&avatar, "radar-label/1");
        let tag = node(json!({"type": "tag", "text": "Hot", "variant": "primary"}));
        let _ = paint_chart_label(&tag, "radar-label/2");
        let group = node(json!({
            "type": "avatar-group",
            "limit": 2,
            "ellipsis": true,
            "children": [
                {"type": "avatar", "text": "Ada"},
                {"type": "avatar", "text": "Grace"},
                {"type": "avatar", "text": "Alan"}
            ]
        }));
        let _ = paint_chart_label(&group, "radar-label/3");
        let hover = node(json!({
            "type": "hover-card",
            "id": "hint",
            "open-delay": 0.2,
            "trigger": {"type": "label", "text": "@ada"},
            "children": [{"type": "label", "text": "Ada Lovelace"}]
        }));
        let _ = paint_chart_label(&hover, "radar-label/4");
    }

    #[test]
    fn paint_table_cell_accepts_progress_tag_and_stack() {
        let progress = node(json!({"type": "progress", "value": 72, "width": 120}));
        let _ = paint_table_cell(&progress, "table/td/0/1", None, None);
        let tag = node(json!({"type": "tag", "text": "stable", "variant": "success"}));
        let _ = paint_table_cell(&tag, "table/td/0/2", None, None);
        let row = node(json!({
            "type": "hstack",
            "gap": 8,
            "children": [
                {"type": "avatar", "text": "Ada"},
                {"type": "label", "text": "Ada"}
            ]
        }));
        let _ = paint_table_cell(&row, "table/td/1/0", None, None);
        let loading = node(json!({
            "type": "progress",
            "value": 10,
            "loading": true,
            "color": "#3366ff",
            "control-size": "small"
        }));
        let _ = paint_table_cell(&loading, "table/td/2/0", None, None);
        let badge = node(json!({
            "type": "badge",
            "icon": "check",
            "max": 9,
            "color": "#22c55e",
            "children": [{"type": "label", "text": "N"}]
        }));
        let _ = paint_table_cell(&badge, "table/td/2/1", None, None);
    }

    #[test]
    fn chart_kit_id_prefers_non_empty_node_id() {
        let named = node(json!({
            "type": "progress-circle",
            "id": "upload",
            "value": 40
        }));
        assert_eq!(node_key(&named, "radar-label/0"), "upload");
        let empty = node(json!({
            "type": "pagination",
            "id": "",
            "value": 2,
            "total": 4
        }));
        assert_eq!(node_key(&empty, "radar-label/1"), "radar-label/1");
        let omitted = node(json!({"type": "progress-circle", "value": 10}));
        assert_eq!(node_key(&omitted, "radar-label/2"), "radar-label/2");
        let _ = paint_chart_label(&named, "radar-label/0");
        let _ = paint_chart_label(&empty, "radar-label/1");
        let _ = paint_chart_label(
            &node(json!({
                "type": "shimmer",
                "id": "think",
                "text": "Thinking…"
            })),
            "radar-label/3",
        );
        let _ = paint_chart_label(
            &node(json!({
                "type": "shimmer",
                "text": "Indexing a/very/long/scan/path.rs",
                "truncate": true,
                "flex": 1
            })),
            "radar-label/4",
        );
        let _ = paint_chart_label(
            &node(json!({
                "type": "label",
                "text": "Hello World",
                "secondary": "Ada",
                "text-overflow": "ellipsis-middle",
                "width": 120
            })),
            "radar-label/5",
        );
    }

    #[test]
    fn avatar_group_content_width_matches_kit_overlap() {
        let three_medium = node(json!({
            "type": "avatar-group",
            "children": [
                {"type": "avatar", "text": "Ada"},
                {"type": "avatar", "text": "Grace"},
                {"type": "avatar", "text": "Alan"}
            ]
        }));
        assert_eq!(
            avatar_group_content_width(&three_medium),
            48.0 + 48.0 * 0.7 * 2.0
        );

        let small_overflow = node(json!({
            "type": "avatar-group",
            "limit": 4,
            "ellipsis": true,
            "control-size": "small",
            "children": [
                {"type": "avatar", "text": "Ada"},
                {"type": "avatar", "text": "Grace"},
                {"type": "avatar", "text": "Alan"},
                {"type": "avatar", "text": "Barbara"},
                {"type": "avatar", "text": "Rich"}
            ]
        }));
        // 4 small faces with 30% overlap, plus ⋯ and ml_1 (4px).
        assert_eq!(
            avatar_group_content_width(&small_overflow),
            24.0 * 4.0 - 24.0 * 0.3 * 3.0 + 24.0 + 4.0
        );
        assert_eq!(avatar_group_flex_width(&three_medium), 48.0 * 3.0);
        assert_eq!(avatar_group_flex_width(&small_overflow), 24.0 * 5.0 + 4.0);

        let empty = node(json!({"type": "avatar-group"}));
        assert_eq!(avatar_group_content_width(&empty), 0.0);
        assert_eq!(avatar_group_flex_width(&empty), 0.0);
    }

    #[test]
    fn avatar_group_clj_styles_split_kit_group_from_clip_wrap() {
        let plain = node(json!({
            "type": "avatar-group",
            "children": [
                {"type": "avatar", "text": "Ada"},
                {"type": "avatar", "text": "Grace"},
                {"type": "avatar", "text": "Alan"}
            ]
        }));
        let styled = node(json!({
            "type": "avatar-group",
            "gap": 8,
            "padding": 4,
            "width": 200,
            "height": 64,
            "flex": 1,
            "children": [
                {"type": "avatar", "text": "Ada"},
                {"type": "avatar", "text": "Grace"},
                {"type": "avatar", "text": "Alan"}
            ]
        }));
        let split = avatar_group_style_split(&styled);
        assert_eq!(split.kit_gap, Some(8.0));
        assert_eq!(split.kit_padding, Some(4.0));
        assert_eq!(split.wrap_width, Some(200.0));
        assert_eq!(split.wrap_height, Some(64.0));
        assert!(split.wrap_flex_fill);
        // Workaround geometry is Kit overlap, not `:gap` on a one-child wrap.
        assert_eq!(split.wrap_workaround_w, avatar_group_content_width(&plain));
        assert_eq!(split.wrap_workaround_h, 48.0);
        assert_eq!(
            avatar_group_content_width(&styled),
            avatar_group_content_width(&plain)
        );
        let _ = avatar_group_element(kit_avatar_group(&styled), &styled);
    }
}
