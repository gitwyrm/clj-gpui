//! Overlay family: dialogs (WindowExt layer), popovers, and popup menus.
//!
//! Dialogs are not ordinary tree children. gpui-component paints them from
//! `Root` via `WindowExt::open_dialog`. The host collects open dialogs from
//! the Clojure tree and syncs that stack on the next frame so `RootView::render`
//! never re-enters `Root::update`.

use crate::mapping;
use crate::protocol::{Cmd, Item, Node};
use gpui::{div, px, App, IntoElement, ParentElement, SharedString, Styled, Window};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{PopupMenu, PopupMenuItem},
    v_flex, Disableable as _,
};
use serde_json::json;
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
    walk_dialogs(root, "root", &mut out);
    out
}

fn walk_dialogs(node: &Node, path: &str, out: &mut Vec<DialogSpec>) {
    if node.kind == "dialog" && node.open.unwrap_or(false) {
        out.push(DialogSpec {
            key: node_key(node, path),
            node: node.clone(),
        });
    }
    if let Some(trigger) = node.trigger.as_ref() {
        walk_dialogs(trigger, &format!("{path}-trigger"), out);
    }
    for (index, child) in node.children.iter().enumerate() {
        walk_dialogs(child, &format!("{path}-{index}"), out);
    }
    for (index, item) in node.items.iter().enumerate() {
        if let Some(content) = item.content.as_ref() {
            walk_dialogs(content, &format!("{path}-item-{index}"), out);
        }
        for (child_ix, child) in item.children.iter().enumerate() {
            walk_dialogs(child, &format!("{path}-item-{index}-{child_ix}"), out);
        }
    }
}

pub fn dialog_keys(specs: &[DialogSpec]) -> Vec<String> {
    specs.iter().map(|spec| spec.key.clone()).collect()
}

/// Fill a popup menu from Clojure `{id, label}` rows (nested `:items` are submenus).
pub fn fill_popup_menu(
    mut menu: PopupMenu,
    items: &[Item],
    cmd_tx: mpsc::Sender<Cmd>,
    on_change: Option<String>,
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
            let cmd_tx = cmd_tx.clone();
            let on_change = on_change.clone();
            let label = item.label_or_id();
            let mut entry = PopupMenuItem::submenu(
                label,
                PopupMenu::build(window, cx, {
                    let nested = nested.clone();
                    move |menu, window, cx| {
                        fill_popup_menu(menu, &nested, cmd_tx, on_change, window, cx)
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
        let cmd_tx = cmd_tx.clone();
        let on_change = on_change.clone();
        let item_click = item.on_click.clone();
        entry = entry.on_click(move |_, _, _| {
            if let Some(callback_id) = item_click.clone() {
                let _ = cmd_tx.send(Cmd::Callback {
                    id: callback_id,
                    value: None,
                    seq: None,
                });
            }
            if let Some(callback_id) = on_change.clone() {
                let _ = cmd_tx.send(Cmd::Callback {
                    id: callback_id,
                    value: Some(json!(id.clone())),
                    seq: None,
                });
            }
        });
        menu = menu.item(entry);
    }
    menu
}

/// Popover content runs as `Fn` during Popover paint while `RootView` is
/// borrowed, so it cannot consume `AnyElement`s from `render_node`. Rebuild
/// a small static tree (label / stack / button / divider) from cloned nodes.
pub fn paint_static(nodes: &[Node], cmd_tx: mpsc::Sender<Cmd>) -> gpui::AnyElement {
    v_flex()
        .gap(px(8.))
        .p(px(8.))
        .min_w(px(160.))
        .children(
            nodes
                .iter()
                .enumerate()
                .map(|(ix, node)| paint_static_node(node, ix, cmd_tx.clone())),
        )
        .into_any_element()
}

fn paint_static_node(node: &Node, ix: usize, cmd_tx: mpsc::Sender<Cmd>) -> gpui::AnyElement {
    match node.kind.as_str() {
        "button" => {
            let label = node.text.clone().unwrap_or_default();
            let mut button =
                Button::new(SharedString::from(format!("static-btn-{ix}"))).label(label);
            if node.primary || node.variant.as_deref() == Some("primary") {
                button = button.primary();
            }
            if let Some(callback_id) = node.on_click.clone() {
                button = button.on_click(move |_, _, _| {
                    let _ = cmd_tx.send(Cmd::Callback {
                        id: callback_id.clone(),
                        value: None,
                        seq: None,
                    });
                });
            }
            button.into_any_element()
        }
        "hstack" => h_flex()
            .gap(px(node.gap.unwrap_or(8.)))
            .children(
                node.children
                    .iter()
                    .enumerate()
                    .map(|(child_ix, child)| paint_static_node(child, child_ix, cmd_tx.clone())),
            )
            .into_any_element(),
        "vstack" => v_flex()
            .gap(px(node.gap.unwrap_or(8.)))
            .children(
                node.children
                    .iter()
                    .enumerate()
                    .map(|(child_ix, child)| paint_static_node(child, child_ix, cmd_tx.clone())),
            )
            .into_any_element(),
        "divider" => gpui_component::divider::Divider::horizontal().into_any_element(),
        _ => div()
            .child(node.text.clone().unwrap_or_default())
            .into_any_element(),
    }
}

/// Build a `Button` trigger for popover / dropdown-menu. Triggers must be
/// `Selectable + IntoElement`; `AnyElement` does not qualify.
pub fn trigger_button(node: Option<&Node>, key: &str) -> Button {
    let (label, primary, variant, disabled) = match node {
        Some(n) if n.kind == "button" || n.kind == "label" => (
            n.text.clone().unwrap_or_else(|| "Open".into()),
            n.primary,
            n.variant.clone(),
            n.disabled,
        ),
        Some(n) => (
            n.text
                .clone()
                .or_else(|| n.title.clone())
                .unwrap_or_else(|| "Open".into()),
            n.primary,
            n.variant.clone(),
            n.disabled,
        ),
        None => ("Open".into(), false, None, false),
    };
    let mut button = Button::new(SharedString::from(format!("{key}-trigger"))).label(label);
    match variant.as_deref() {
        Some("primary") => button = button.primary(),
        Some("ghost") => button = button.ghost(),
        Some("outline") => button = button.outline(),
        Some("danger") => button = button.danger(),
        Some("text") => button = button.text(),
        _ if primary => button = button.primary(),
        _ => {}
    }
    if disabled {
        button = button.disabled(true);
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
    match node.variant.as_deref().map(crate::catalog::normalize) {
        Some(name) if name == "confirm" => dialog = dialog.confirm(),
        Some(name) if name == "alert" => dialog = dialog.alert(),
        _ => {}
    }
    // confirm()/alert() set overlay_closable(false). Overlay dismiss is the
    // default for Dialog::new and for this host unless Clojure opts out.
    dialog = dialog.overlay_closable(overlay_closable(node));
    if let Some(width) = node.width {
        dialog = dialog.width(px(width));
    }
    dialog.extend(children);
    dialog
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

pub fn bind_dialog_callbacks(
    dialog: gpui_component::dialog::Dialog,
    node: &Node,
    cmd_tx: mpsc::Sender<Cmd>,
) -> gpui_component::dialog::Dialog {
    let on_ok = node.on_ok.clone();
    let on_cancel = node.on_cancel.clone();
    let on_close = node.on_close.clone();
    let on_open_change = node.on_open_change.clone();
    let tx_ok = cmd_tx.clone();
    let tx_cancel = cmd_tx.clone();
    let tx_close = cmd_tx;
    dialog
        .on_ok(move |_, _, _| {
            if let Some(id) = on_ok.clone() {
                let _ = tx_ok.send(Cmd::Callback {
                    id,
                    value: None,
                    seq: None,
                });
            }
            true
        })
        .on_cancel(move |_, _, _| {
            if let Some(id) = on_cancel.clone() {
                let _ = tx_cancel.send(Cmd::Callback {
                    id,
                    value: None,
                    seq: None,
                });
            }
            true
        })
        .on_close(move |_, _, _| {
            if let Some(id) = on_close.clone() {
                let _ = tx_close.send(Cmd::Callback {
                    id,
                    value: None,
                    seq: None,
                });
            }
            if let Some(id) = on_open_change.clone() {
                let _ = tx_close.send(Cmd::Callback {
                    id,
                    value: Some(json!(false)),
                    seq: None,
                });
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
