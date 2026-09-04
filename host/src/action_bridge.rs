//! Generic host GPUI Action carrying stable Clojure semantic identity.
//!
//! Kit `NativeMenu` and `CommandItem::action` require a real `Box<dyn Action>`.
//! Clojure never sees that type. The Action stores a widget slot key plus an
//! item id — never a generated `cb-N`. An unrelated `export-tree` while an OS
//! menu is open cannot stale the Action; dispatch resolves live callbacks
//! against the installed tree.

use crate::mapping;
use crate::protocol::Item;
use gpui::{Action, Pixels, Point, Window, point, px};
use gpui_component::{
    Disableable as _,
    command::{Command, CommandGroup, CommandItem},
    native_menu::NativeMenu,
};
use gpui_kit as gpui;
use gpui_kit::component as gpui_component;
use serde::Deserialize;

/// Host Action for NativeMenu / CommandItem (and later keybindings).
///
/// `slot` is the widget key (`:id` or tree path). `item` is the wire item id.
#[derive(Action, Clone, Debug, Default, PartialEq, Deserialize)]
#[action(namespace = clj_gpui, no_json)]
pub struct CljAction {
    pub slot: String,
    pub item: String,
}

impl CljAction {
    pub fn new(slot: impl Into<String>, item: impl Into<String>) -> Self {
        Self {
            slot: slot.into(),
            item: item.into(),
        }
    }

    pub fn boxed(&self) -> Box<dyn Action> {
        Box::new(self.clone())
    }
}

/// Leaf Actions that would be attached to a NativeMenu / Command snapshot.
///
/// Submenu / group wrappers are skipped; separators are skipped. Callback ids
/// on the items are ignored — they are not part of the Action.
pub fn clj_leaf_actions(items: &[Item], slot: &str) -> Vec<CljAction> {
    let mut out = Vec::new();
    collect_leaf_actions(items, slot, &mut out);
    out
}

fn collect_leaf_actions(items: &[Item], slot: &str, out: &mut Vec<CljAction>) {
    for item in items {
        if item.is_separator() {
            continue;
        }
        if !item.items.is_empty() {
            collect_leaf_actions(&item.items, slot, out);
            continue;
        }
        out.push(CljAction::new(slot, item.id_or_label()));
    }
}

/// Materialize Clojure's semantic menu tree into Kit `NativeMenu`.
///
/// This is a presentation snapshot: labels, order, nesting, disabled/checked,
/// and icons are copied as they are now. Selecting an item dispatches
/// `CljAction { slot, item }` so Clojure remains the owner of toggled state.
pub fn fill_native_menu(items: &[Item], slot: &str) -> NativeMenu {
    let mut menu = NativeMenu::new();
    for item in items {
        menu = push_native_item(menu, item, slot);
    }
    menu
}

fn push_native_item(menu: NativeMenu, item: &Item, slot: &str) -> NativeMenu {
    if item.is_separator() {
        return menu.separator();
    }
    if !item.items.is_empty() {
        return menu.submenu(item.label_or_id(), fill_native_menu(&item.items, slot));
    }
    let action = CljAction::new(slot, item.id_or_label()).boxed();
    let label = item.label_or_id();
    let disabled = item.disabled;
    let checked = item.checked.unwrap_or(false);
    let icon = item.icon.as_deref().and_then(mapping::parse_icon);
    // Kit's public NativeMenu builders cannot combine check with icon or
    // with disabled. Prefer icon, then check, then disabled.
    match icon {
        Some(icon) if disabled => menu.menu_with_icon_disabled(label, icon, true, action),
        Some(icon) => menu.menu_with_icon(label, icon, action),
        None if checked => menu.menu_with_check(label, true, action),
        None if disabled => menu.menu_with_disabled(label, true, action),
        None => menu.menu(label, action),
    }
}

pub fn native_menu_position(node: &crate::protocol::Node, window: &Window) -> Point<Pixels> {
    match node.position.as_deref() {
        Some([x, y, ..]) => point(px(*x), px(*y)),
        _ => window.mouse_position(),
    }
}

pub fn native_menu_should_show(was_open: bool, open: bool) -> bool {
    open && !was_open
}

pub fn command_item(item: &Item, slot: &str) -> CommandItem {
    let mut cmd = CommandItem::new().label(item.label_or_id());
    if let Some(icon) = item.icon.as_deref().and_then(mapping::parse_icon) {
        cmd = cmd.icon(icon);
    }
    if item.checked.unwrap_or(false) {
        cmd = cmd.checked(true);
    }
    if item.disabled {
        cmd = cmd.disabled(true);
    }
    if !item.keywords.is_empty() {
        cmd = cmd.keywords(item.keywords.clone());
    }
    cmd.action(CljAction::new(slot, item.id_or_label()).boxed())
}

/// Push Clojure-owned entries onto a Kit `Command`. Groups are nested `:items`.
pub fn apply_command_entries(mut command: Command, items: &[Item], slot: &str) -> Command {
    for item in items {
        if item.is_separator() {
            command = command.separator();
            continue;
        }
        if !item.items.is_empty() {
            let mut group = CommandGroup::new().label(item.label_or_id());
            for child in &item.items {
                if child.is_separator() {
                    continue;
                }
                group = group.item(command_item(child, slot));
            }
            command = command.group(group);
            continue;
        }
        command = command.item(command_item(item, slot));
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Item;

    fn item(id: &str, label: &str) -> Item {
        Item {
            id: Some(id.into()),
            label: Some(label.into()),
            ..Item::default()
        }
    }

    #[test]
    fn leaf_actions_use_slot_and_item_id_not_callback_ids() {
        let nested = Item {
            id: Some("share".into()),
            label: Some("Share".into()),
            items: vec![Item {
                id: Some("link".into()),
                label: Some("Copy link".into()),
                on_click: Some("cb-99".into()),
                ..Item::default()
            }],
            on_click: Some("cb-12".into()),
            ..Item::default()
        };
        let copy = Item {
            id: Some("copy".into()),
            label: Some("Copy".into()),
            on_click: Some("cb-7".into()),
            checked: Some(true),
            ..Item::default()
        };
        let actions = clj_leaf_actions(
            &[
                copy,
                Item {
                    separator: true,
                    ..Item::default()
                },
                nested,
            ],
            "edit-menu",
        );
        assert_eq!(
            actions,
            vec![
                CljAction::new("edit-menu", "copy"),
                CljAction::new("edit-menu", "link"),
            ]
        );
        for action in &actions {
            assert!(
                !action.slot.starts_with("cb-") && !action.item.starts_with("cb-"),
                "Action must not capture a generated callback id: {action:?}"
            );
        }
    }

    #[test]
    fn fill_native_menu_snapshot_is_empty_without_leaves() {
        assert!(fill_native_menu(&[], "edit").is_empty());
        assert!(!fill_native_menu(&[item("copy", "Copy")], "edit").is_empty());
    }

    #[test]
    fn rising_edge_show_is_once_per_open_request() {
        assert!(native_menu_should_show(false, true));
        assert!(!native_menu_should_show(true, true));
        assert!(!native_menu_should_show(true, false));
        assert!(!native_menu_should_show(false, false));
    }

    #[test]
    fn command_item_action_matches_leaf_identity() {
        let row = Item {
            id: Some("wrap".into()),
            label: Some("Word wrap".into()),
            checked: Some(true),
            keywords: vec!["line".into()],
            on_click: Some("cb-3".into()),
            ..Item::default()
        };
        let actions = clj_leaf_actions(&[row], "palette");
        assert_eq!(actions, vec![CljAction::new("palette", "wrap")]);
    }
}
