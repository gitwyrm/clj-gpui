//! Overlay family: dialogs, sheets, notifications, popovers, and popup menus.
//!
//! Dialogs and sheets are not ordinary tree children. gpui-component paints
//! them from `Root` via `WindowExt`. The host collects open overlays from the
//! Clojure tree and syncs on the next frame so `RootView::render` never
//! re-enters `Root`. Notifications are a stack on `Root.notification`.

use crate::mapping;
use crate::protocol::{self, Cmd, Item, Node};
use gpui::{
    App, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, Window, div, px,
};
use gpui_component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
    dialog::{AlertDialog, DialogButtonProps},
    h_flex,
    menu::{PopupMenu, PopupMenuItem},
    v_flex,
};
use gpui_kit as gpui;
use gpui_kit::component as gpui_component;
use serde_json::json;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::mpsc;

#[derive(Clone, Debug)]
pub struct DialogSpec {
    pub key: String,
    pub node: Node,
}

pub fn node_key(node: &Node, path: &str) -> String {
    node.id
        .as_deref()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.to_string())
}

pub fn collect_open_dialogs(root: &Node) -> Vec<DialogSpec> {
    let mut out = Vec::new();
    walk_nodes(root, "root", &mut |node, path| {
        if matches!(node.kind.as_str(), "dialog" | "alert-dialog") && node.open.unwrap_or(false) {
            out.push(DialogSpec {
                key: node_key(node, path),
                node: node.clone(),
            });
        }
    });
    out
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
    ButtonClick { key: String },
    DialogClose { key: String, ok: Option<bool> },
    PopoverOpen { key: String, open: bool },
    MenuSelect { key: String, item_path: Vec<String> },
}

pub type ActionEmitter = Rc<dyn Fn(QueuedAction, &mut App)>;

#[derive(Default)]
pub struct CallbackQueue {
    pending: VecDeque<QueuedAction>,
    wait_for_seq: Option<u64>,
}

impl CallbackQueue {
    pub fn push(&mut self, action: QueuedAction) {
        self.pending.push_back(action);
    }

    pub fn next(&mut self, tree: &Node) -> Option<Vec<protocol::CallbackCall>> {
        if self.wait_for_seq.is_some() {
            return None;
        }
        while let Some(action) = self.pending.pop_front() {
            let calls = action.resolve(tree);
            if !calls.is_empty() {
                return Some(calls);
            }
        }
        None
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
            | Self::MenuSelect { key, .. } => key,
        };
        let mut found = None;
        walk_nodes(tree, "root", &mut |node, path| {
            let kind_matches = match self {
                Self::ButtonClick { .. } => node.kind == "button",
                Self::DialogClose { .. } => node.kind == "dialog",
                Self::PopoverOpen { .. } => node.kind == "popover",
                Self::MenuSelect { .. } => {
                    node.kind == "dropdown-menu" || node.kind == "context-menu"
                }
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
            Self::MenuSelect { item_path, .. } => {
                let mut items = node.items.as_slice();
                let mut selected = None;
                for identity in item_path {
                    let Some(item) = items.iter().find(|item| item.id_or_label() == *identity)
                    else {
                        return Vec::new();
                    };
                    if item.disabled || item.is_separator() {
                        return Vec::new();
                    }
                    selected = Some(item);
                    items = &item.items;
                }
                let Some(item) = selected.filter(|item| item.items.is_empty()) else {
                    return Vec::new();
                };
                protocol::menu_selection_calls(
                    item.on_click.clone(),
                    node.on_change,
                    item.id_or_label(),
                )
            }
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
pub fn paint_static(nodes: &[Node], emit: ActionEmitter, path: &str) -> gpui::AnyElement {
    v_flex()
        .gap(px(8.))
        .p(px(8.))
        .min_w(px(160.))
        .children(
            nodes.iter().enumerate().map(|(ix, node)| {
                paint_static_node(node, &static_child_path(path, ix), emit.clone())
            }),
        )
        .into_any_element()
}

pub fn static_child_path(prefix: &str, index: usize) -> String {
    format!("{prefix}/{index}")
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
            "dialog" | "popover" | "sheet" => {
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

fn paint_static_node(node: &Node, path: &str, emit: ActionEmitter) -> gpui::AnyElement {
    match node.kind.as_str() {
        "button" => {
            let label = node.text.clone().unwrap_or_default();
            let mut button = Button::new(SharedString::from(path.to_string())).label(label);
            button = apply_button_chrome(button, node);
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
                paint_static_node(child, &static_child_path(path, child_ix), emit.clone())
            }))
            .into_any_element(),
        "vstack" => v_flex()
            .gap(px(node.gap.unwrap_or(8.)))
            .children(node.children.iter().enumerate().map(|(child_ix, child)| {
                paint_static_node(child, &static_child_path(path, child_ix), emit.clone())
            }))
            .into_any_element(),
        "separator" => gpui_component::separator::Separator::horizontal().into_any_element(),
        _ => div()
            .id(SharedString::from(path.to_string()))
            .child(node.text.clone().unwrap_or_default())
            .into_any_element(),
    }
}

fn apply_button_chrome(mut button: Button, node: &Node) -> Button {
    match node.variant.as_deref() {
        Some("primary") => button = button.primary(),
        Some("ghost") => button = button.ghost(),
        Some("outline") => button = button.outline(),
        Some("danger") => button = button.danger(),
        Some("text") => button = button.text(),
        _ if node.primary => button = button.primary(),
        _ => {}
    }
    if node.disabled {
        button = button.disabled(true);
    }
    button
}

/// Build a `Button` trigger for popover / dropdown-menu. Triggers must be
/// `Selectable + IntoElement`; `AnyElement` does not qualify.
pub fn trigger_button(node: Option<&Node>, key: &str) -> Button {
    let button = Button::new(SharedString::from(format!("{key}-trigger")));
    let Some(n) = node else {
        return button.label("Open");
    };
    let label = n
        .text
        .clone()
        .or_else(|| n.title.clone())
        .unwrap_or_else(|| "Open".into());
    apply_button_chrome(button.label(label), n)
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
    match node.variant.as_deref().map(crate::catalog::normalize) {
        Some(name) if name == "confirm" => {
            dialog = dialog.button_props(DialogButtonProps::default().show_cancel(true));
            if let Some(closable) = node.overlay_closable {
                dialog = dialog.overlay_closable(closable);
            }
        }
        _ => {
            dialog = dialog.overlay_closable(overlay_closable(node));
        }
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
    if node
        .variant
        .as_deref()
        .map(crate::catalog::normalize)
        .as_deref()
        == Some("confirm")
    {
        alert = alert.confirm();
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
        "{}|{}|{}|{:?}",
        node.title.as_deref().unwrap_or(""),
        node.message
            .as_deref()
            .or(node.text.as_deref())
            .unwrap_or(""),
        node.variant.as_deref().unwrap_or(""),
        node.autohide
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
}
