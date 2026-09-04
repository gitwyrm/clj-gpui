//! Generic host GPUI Action carrying stable Clojure semantic identity.
//!
//! Kit `NativeMenu` and `CommandItem::action` require a real `Box<dyn Action>`.
//! Clojure never sees that type. The Action stores a widget slot key plus a
//! semantic `item_path` — never a generated `cb-N`. Nested NativeMenu submenus
//! and Command groups append their identities so two leaves that share an id
//! (`file/open` vs `project/open`) dispatch as distinct Actions. An unrelated
//! `export-tree` while an OS menu is open cannot stale the Action; dispatch
//! resolves live callbacks against the installed tree.

use crate::mapping;
use crate::protocol::Item;
use gpui::{Action, Pixels, Point, Window, point, px};
use gpui_component::{
    Disableable as _, IndexPath,
    command::{Command, CommandGroup, CommandItem},
    native_menu::NativeMenu,
};
use gpui_kit as gpui;
use gpui_kit::component as gpui_component;
use serde::Deserialize;
use serde_json::Value;

/// Host Action for NativeMenu / CommandItem (and later keybindings).
///
/// `slot` is the widget key (`:id` or tree path). `item_path` is the
/// semantic path from the menu/command root to the leaf (submenu / group
/// identities, then the leaf id). Ungrouped leaves are a one-element path.
#[derive(Action, Clone, Debug, Default, PartialEq, Deserialize)]
#[action(namespace = clj_gpui, no_json)]
pub struct CljAction {
    pub slot: String,
    pub item_path: Vec<String>,
}

impl CljAction {
    pub fn new(slot: impl Into<String>, item_path: Vec<String>) -> Self {
        Self {
            slot: slot.into(),
            item_path,
        }
    }

    pub fn boxed(&self) -> Box<dyn Action> {
        Box::new(self.clone())
    }
}

/// Leaf Actions that would be attached to a NativeMenu / Command snapshot.
///
/// Submenu / group wrappers are skipped as leaves; their identity is
/// prepended to each descendant path. Separators are skipped. Callback ids
/// on the items are ignored — they are not part of the Action.
#[cfg(test)]
pub fn clj_leaf_actions(items: &[Item], slot: &str) -> Vec<CljAction> {
    let mut out = Vec::new();
    collect_leaf_actions(items, slot, &[], &mut out);
    out
}

#[cfg(test)]
fn collect_leaf_actions(items: &[Item], slot: &str, prefix: &[String], out: &mut Vec<CljAction>) {
    for item in items {
        if item.is_separator() {
            continue;
        }
        let mut path = prefix.to_vec();
        path.push(item.id_or_label());
        if !item.items.is_empty() {
            collect_leaf_actions(&item.items, slot, &path, out);
            continue;
        }
        out.push(CljAction::new(slot, path));
    }
}

/// Materialize Clojure's semantic menu tree into Kit `NativeMenu`.
///
/// This is a presentation snapshot: labels, order, nesting, disabled/checked,
/// and icons are copied as they are now. Selecting an item dispatches
/// `CljAction { slot, item_path }` so Clojure remains the owner of toggled
/// state. Nested submenus append their identity to `item_path`.
/// Disabled wins over checked when Kit cannot represent both (no icon).
pub fn fill_native_menu(items: &[Item], slot: &str) -> NativeMenu {
    // Kit's NativeMenu::submenu() always stores disabled: false. The public
    // From<gpui::Menu> conversion can represent a disabled submenu, but
    // gpui::MenuItem has no icon, so that snapshot drops leaf icons.
    if native_tree_has_disabled_submenu(items) {
        NativeMenu::from(gpui_menu_from_items(items, slot, &[]))
    } else {
        fill_native_menu_at(items, slot, &[])
    }
}

pub(crate) fn native_tree_has_disabled_submenu(items: &[Item]) -> bool {
    items.iter().any(|item| {
        !item.items.is_empty() && (item.disabled || native_tree_has_disabled_submenu(&item.items))
    })
}

fn gpui_menu_from_items(items: &[Item], slot: &str, prefix: &[String]) -> gpui::Menu {
    gpui::Menu::new("").items(gpui_menu_items(items, slot, prefix))
}

pub(crate) fn gpui_menu_items(
    items: &[Item],
    slot: &str,
    prefix: &[String],
) -> Vec<gpui::MenuItem> {
    items
        .iter()
        .filter_map(|item| gpui_menu_item(item, slot, prefix))
        .collect()
}

fn gpui_menu_item(item: &Item, slot: &str, prefix: &[String]) -> Option<gpui::MenuItem> {
    if item.is_separator() {
        return Some(gpui::MenuItem::separator());
    }
    if !item.items.is_empty() {
        let mut path = prefix.to_vec();
        path.push(item.id_or_label());
        let submenu = gpui::Menu::new(item.label_or_id())
            .disabled(item.disabled)
            .items(gpui_menu_items(&item.items, slot, &path));
        return Some(gpui::MenuItem::submenu(submenu));
    }
    let mut path = prefix.to_vec();
    path.push(item.id_or_label());
    Some(
        gpui::MenuItem::action(item.label_or_id(), CljAction::new(slot, path))
            .disabled(item.disabled)
            .checked(item.checked.unwrap_or(false)),
    )
}

fn fill_native_menu_at(items: &[Item], slot: &str, prefix: &[String]) -> NativeMenu {
    let mut menu = NativeMenu::new();
    for item in items {
        menu = push_native_item(menu, item, slot, prefix);
    }
    menu
}

fn push_native_item(menu: NativeMenu, item: &Item, slot: &str, prefix: &[String]) -> NativeMenu {
    if item.is_separator() {
        return menu.separator();
    }
    if !item.items.is_empty() {
        let mut path = prefix.to_vec();
        path.push(item.id_or_label());
        return menu.submenu(
            item.label_or_id(),
            fill_native_menu_at(&item.items, slot, &path),
        );
    }
    let mut path = prefix.to_vec();
    path.push(item.id_or_label());
    let action = CljAction::new(slot, path).boxed();
    let label = item.label_or_id();
    let icon = item.icon.as_deref().and_then(mapping::parse_icon);
    match (native_leaf_kind(item), icon) {
        (NativeLeafKind::IconDisabled, Some(icon)) => {
            menu.menu_with_icon_disabled(label, icon, true, action)
        }
        (NativeLeafKind::Icon, Some(icon)) => menu.menu_with_icon(label, icon, action),
        (NativeLeafKind::Disabled, _) => menu.menu_with_disabled(label, true, action),
        (NativeLeafKind::Check, _) => menu.menu_with_check(label, true, action),
        (NativeLeafKind::Plain, _)
        | (NativeLeafKind::IconDisabled, None)
        | (NativeLeafKind::Icon, None) => menu.menu(label, action),
    }
}

/// Kit public NativeMenu builders cannot combine a check mark with an
/// icon or with disabled. Prefer behavior over decoration: disabled
/// wins over checked when no icon is present (the check mark is
/// dropped). An icon still wins over both, including icon+disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeLeafKind {
    IconDisabled,
    Icon,
    Disabled,
    Check,
    Plain,
}

pub(crate) fn native_leaf_kind(item: &Item) -> NativeLeafKind {
    let has_icon = item.icon.as_deref().and_then(mapping::parse_icon).is_some();
    let disabled = item.disabled;
    let checked = item.checked.unwrap_or(false);
    match (has_icon, disabled, checked) {
        (true, true, _) => NativeLeafKind::IconDisabled,
        (true, false, _) => NativeLeafKind::Icon,
        (false, true, _) => NativeLeafKind::Disabled,
        (false, false, true) => NativeLeafKind::Check,
        (false, false, false) => NativeLeafKind::Plain,
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

fn command_item(item: &Item, slot: &str, item_path: Vec<String>) -> CommandItem {
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
    cmd.action(CljAction::new(slot, item_path).boxed())
}

/// Push Clojure-owned entries onto a Kit `Command`. Groups are nested `:items`.
/// Group identity is prepended to each child's Action path; ungrouped leaves
/// use a one-element path. Separators inside a group are skipped (same as
/// the IndexPath mapper).
pub fn apply_command_entries(mut command: Command, items: &[Item], slot: &str) -> Command {
    for item in items {
        if item.is_separator() {
            command = command.separator();
            continue;
        }
        if !item.items.is_empty() {
            let group_id = item.id_or_label();
            let mut group = CommandGroup::new().label(item.label_or_id());
            for child in &item.items {
                if child.is_separator() {
                    continue;
                }
                let path = vec![group_id.clone(), child.id_or_label()];
                group = group.item(command_item(child, slot, path));
            }
            command = command.group(group);
            continue;
        }
        command = command.item(command_item(item, slot, vec![item.id_or_label()]));
    }
    command
}

/// Kit `IndexPath` for a semantic command path, matching `update_matches`.
///
/// Ungrouped `CommandEntry::Item` rows occupy section 0. Groups follow an
/// implicit ungrouped section when both forms are mixed. Separators do not
/// occupy a section or row. A one-element path is an ungrouped leaf, or the
/// first grouped leaf with that id when no ungrouped leaf matches (so
/// `:selected :find` still works under a group). A two-element path is
/// exact group + leaf.
pub fn command_index_path(items: &[Item], path: &[String]) -> Option<IndexPath> {
    if path.is_empty() {
        return None;
    }
    let has_ungrouped = items
        .iter()
        .any(|item| !item.is_separator() && item.items.is_empty());
    let mut ungrouped_ix = 0usize;
    let mut group_ix = 0usize;
    let mut grouped_fallback = None;
    for item in items {
        if item.is_separator() {
            continue;
        }
        if item.items.is_empty() {
            if path.len() == 1 && item.id_or_label() == path[0] {
                return Some(IndexPath::new(ungrouped_ix).section(0));
            }
            ungrouped_ix += 1;
            continue;
        }
        let section_ix = group_ix + usize::from(has_ungrouped);
        group_ix += 1;
        let mut item_ix = 0usize;
        for child in &item.items {
            if child.is_separator() {
                continue;
            }
            if path.len() == 2 && item.id_or_label() == path[0] && child.id_or_label() == path[1] {
                return Some(IndexPath::new(item_ix).section(section_ix));
            }
            if path.len() == 1 && child.id_or_label() == path[0] && grouped_fallback.is_none() {
                grouped_fallback = Some(IndexPath::new(item_ix).section(section_ix));
            }
            item_ix += 1;
        }
    }
    grouped_fallback
}

/// Semantic path for a Kit command `IndexPath`, inverse of [`command_index_path`].
pub fn command_item_path(items: &[Item], index: IndexPath) -> Option<Vec<String>> {
    let has_ungrouped = items
        .iter()
        .any(|item| !item.is_separator() && item.items.is_empty());
    let mut ungrouped_ix = 0usize;
    let mut group_ix = 0usize;
    for item in items {
        if item.is_separator() {
            continue;
        }
        if item.items.is_empty() {
            if index.section == 0 && index.row == ungrouped_ix {
                return Some(vec![item.id_or_label()]);
            }
            ungrouped_ix += 1;
            continue;
        }
        let section_ix = group_ix + usize::from(has_ungrouped);
        group_ix += 1;
        if index.section != section_ix {
            continue;
        }
        let mut item_ix = 0usize;
        for child in &item.items {
            if child.is_separator() {
                continue;
            }
            if index.row == item_ix {
                return Some(vec![item.id_or_label(), child.id_or_label()]);
            }
            item_ix += 1;
        }
    }
    None
}

/// Callback-generation latch for a native Command echo (`:on-select` / `:on-query`).
///
/// `seq` is the host callback seq sent with that native event. Unrelated
/// trees (`tree_seq != seq`, including `request-render` with `None`) must
/// not release it. The tree for that exact seq consumes it, and Clojure's
/// value then wins even when it differs from what we emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandEchoLatch<T> {
    pub seq: u64,
    pub value: T,
}

/// Whether a controlled Command value should override native state.
pub fn should_apply_command_echo<T: PartialEq>(
    latch: Option<&CommandEchoLatch<T>>,
    current: Option<&T>,
    tree_seq: Option<u64>,
) -> bool {
    match latch {
        Some(latch) if tree_seq == Some(latch.seq) => true,
        Some(latch) if current == Some(&latch.value) => false,
        _ => true,
    }
}

/// Kit `set_selected_index` looks up `desired` in the currently installed
/// matched list. An empty (first paint) or stale (replaced items) list
/// cannot represent the new model's path, so `Command::render` must
/// `install_model` before controlled selection sync.
#[cfg(test)]
pub fn command_selection_on_matched(
    matched: &[IndexPath],
    desired: Option<IndexPath>,
) -> Option<IndexPath> {
    desired.filter(|want| matched.iter().any(|have| have == want))
}

/// Controlled Command highlight from node `value`.
///
/// `None` is omitted (leave native highlight). An empty vec is JSON `null`
/// (clear). A string is a one-element path; a JSON array is an exact path.
pub fn command_value_path(value: Option<&Value>) -> Option<Vec<String>> {
    match value {
        None => None,
        Some(Value::Null) => Some(Vec::new()),
        Some(Value::Array(items)) => Some(
            items
                .iter()
                .filter_map(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    Value::Bool(b) => Some(b.to_string()),
                    _ => None,
                })
                .collect(),
        ),
        Some(Value::String(s)) => Some(vec![s.clone()]),
        Some(Value::Number(n)) => Some(vec![n.to_string()]),
        Some(Value::Bool(b)) => Some(vec![b.to_string()]),
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Item, menu_selection_payload};
    use serde_json::json;

    fn item(id: &str, label: &str) -> Item {
        Item {
            id: Some(id.into()),
            label: Some(label.into()),
            ..Item::default()
        }
    }

    fn group(id: &str, label: &str, children: Vec<Item>) -> Item {
        Item {
            id: Some(id.into()),
            label: Some(label.into()),
            items: children,
            ..Item::default()
        }
    }

    #[test]
    fn leaf_actions_use_slot_and_item_path_not_callback_ids() {
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
                CljAction::new("edit-menu", vec!["copy".into()]),
                CljAction::new("edit-menu", vec!["share".into(), "link".into()]),
            ]
        );
        for action in &actions {
            assert!(
                !action.slot.starts_with("cb-")
                    && action
                        .item_path
                        .iter()
                        .all(|segment| !segment.starts_with("cb-")),
                "Action must not capture a generated callback id: {action:?}"
            );
        }
    }

    #[test]
    fn duplicate_native_menu_leaf_ids_keep_submenu_identity() {
        let file = group("file", "File", vec![item("open", "Open file")]);
        let project = group("project", "Project", vec![item("open", "Open project")]);
        let actions = clj_leaf_actions(&[file, project], "os-menu");
        assert_eq!(
            actions,
            vec![
                CljAction::new("os-menu", vec!["file".into(), "open".into()]),
                CljAction::new("os-menu", vec!["project".into(), "open".into()]),
            ]
        );
    }

    #[test]
    fn duplicate_command_group_leaf_ids_keep_group_identity() {
        let file = group("file", "File", vec![item("open", "Open file")]);
        let project = group("project", "Project", vec![item("open", "Open project")]);
        let actions = clj_leaf_actions(&[file, project], "palette");
        assert_eq!(
            actions,
            vec![
                CljAction::new("palette", vec!["file".into(), "open".into()]),
                CljAction::new("palette", vec!["project".into(), "open".into()]),
            ]
        );
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
        assert_eq!(
            actions,
            vec![CljAction::new("palette", vec!["wrap".into()])]
        );
    }

    #[test]
    fn command_index_path_matches_kit_sections() {
        let items = vec![
            item("copy", "Copy"),
            Item {
                separator: true,
                ..Item::default()
            },
            item("wrap", "Wrap"),
            group(
                "edit",
                "Edit",
                vec![
                    item("find", "Find"),
                    Item {
                        separator: true,
                        ..Item::default()
                    },
                    item("replace", "Replace"),
                ],
            ),
        ];
        assert_eq!(
            command_index_path(&items, &["copy".into()]),
            Some(IndexPath::new(0).section(0))
        );
        assert_eq!(
            command_index_path(&items, &["wrap".into()]),
            Some(IndexPath::new(1).section(0))
        );
        assert_eq!(
            command_index_path(&items, &["edit".into(), "find".into()]),
            Some(IndexPath::new(0).section(1))
        );
        assert_eq!(
            command_index_path(&items, &["edit".into(), "replace".into()]),
            Some(IndexPath::new(1).section(1))
        );
        assert_eq!(
            command_index_path(&items, &["find".into()]),
            Some(IndexPath::new(0).section(1))
        );
        assert_eq!(
            command_item_path(&items, IndexPath::new(1).section(1)).as_deref(),
            Some(&["edit".to_string(), "replace".to_string()][..])
        );
    }

    #[test]
    fn command_index_path_disambiguates_duplicate_group_leaves() {
        let items = vec![
            group("file", "File", vec![item("open", "Open file")]),
            group("project", "Project", vec![item("open", "Open project")]),
        ];
        assert_eq!(
            command_index_path(&items, &["file".into(), "open".into()]),
            Some(IndexPath::new(0).section(0))
        );
        assert_eq!(
            command_index_path(&items, &["project".into(), "open".into()]),
            Some(IndexPath::new(0).section(1))
        );
        assert_eq!(
            command_index_path(&items, &["open".into()]),
            Some(IndexPath::new(0).section(0)),
            "a one-element path is the first grouped leaf"
        );
        assert_eq!(
            command_item_path(&items, IndexPath::new(0).section(1)).as_deref(),
            Some(&["project".to_string(), "open".to_string()][..])
        );
        let echoed = menu_selection_payload(&["project".into(), "open".into()]);
        assert_eq!(echoed, json!(["project", "open"]));
        let selected = command_value_path(Some(&echoed)).unwrap();
        assert_eq!(
            command_index_path(&items, &selected),
            Some(IndexPath::new(0).section(1)),
            "echoing the grouped payload as :selected must stay on project/open"
        );
        assert_eq!(
            command_index_path(&items, &["open".into()]),
            Some(IndexPath::new(0).section(0)),
            "a one-element echo would jump to the first grouped open"
        );
    }

    #[test]
    fn command_select_echo_latch_lives_until_the_callback_seq() {
        let file = vec!["file".to_string(), "open".to_string()];
        let project = vec!["project".to_string(), "open".to_string()];
        assert!(
            should_apply_command_echo::<Vec<String>>(None, Some(&project), Some(1)),
            "no :on-select means no latch; Clojure :selected must restore"
        );
        let latch = CommandEchoLatch {
            seq: 7,
            value: project.clone(),
        };
        assert!(
            !should_apply_command_echo(Some(&latch), Some(&project), None),
            "an unrelated request-render must not release the latch"
        );
        assert!(
            !should_apply_command_echo(Some(&latch), Some(&project), Some(3)),
            "a different callback seq must not release the latch"
        );
        assert!(
            should_apply_command_echo(Some(&latch), Some(&project), Some(7)),
            "the matching seq consumes the latch; Clojure may reject project/open"
        );
        assert!(should_apply_command_echo(
            Some(&latch),
            Some(&file),
            Some(3)
        ));
        let query = CommandEchoLatch {
            seq: 7,
            value: "fi".to_string(),
        };
        assert!(!should_apply_command_echo(
            Some(&query),
            Some(&"fi".to_string()),
            None
        ));
        assert!(should_apply_command_echo(
            Some(&query),
            Some(&"fi".to_string()),
            Some(7)
        ));
        assert!(
            should_apply_command_echo(Some(&query), Some(&"fi".to_string()), Some(7))
                && should_apply_command_echo(Some(&query), Some(&"find".to_string()), Some(7)),
            "the matching seq may echo, reject, or transform the typed query"
        );
    }

    #[test]
    fn controlled_selection_requires_the_installed_command_model() {
        let items = vec![
            item("copy", "Copy"),
            group("edit", "Edit", vec![item("find", "Find")]),
        ];
        let desired = command_index_path(&items, &["find".into()]);
        assert_eq!(
            command_selection_on_matched(&[], desired),
            None,
            "first paint still has the empty default model"
        );
        let installed = [IndexPath::new(0).section(0), IndexPath::new(0).section(1)];
        assert_eq!(
            command_selection_on_matched(&installed, desired),
            desired,
            "after install_model the IndexPath is in matched"
        );

        let previous = vec![item("copy", "Copy"), item("wrap", "Wrap")];
        let next = vec![
            group("file", "File", vec![item("open", "Open file")]),
            group("project", "Project", vec![item("open", "Open project")]),
        ];
        let new_desired = command_index_path(&next, &["project".into(), "open".into()]);
        let old_matched = [IndexPath::new(0).section(0), IndexPath::new(1).section(0)];
        assert_eq!(
            command_index_path(&previous, &["project".into(), "open".into()]),
            None
        );
        assert_eq!(
            command_selection_on_matched(&old_matched, new_desired),
            None,
            "the previous model's matched list cannot represent project/open"
        );
        let new_matched = [IndexPath::new(0).section(0), IndexPath::new(0).section(1)];
        assert_eq!(
            command_selection_on_matched(&new_matched, new_desired),
            new_desired
        );
    }

    #[test]
    fn disabled_submenu_is_represented_on_the_gpui_menu_bridge() {
        let mut share = group("share", "Share", vec![item("link", "Copy link")]);
        share.disabled = true;
        let items = vec![item("copy", "Copy"), share];
        assert!(native_tree_has_disabled_submenu(&items));
        let gpui_items = gpui_menu_items(&items, "edit-menu", &[]);
        assert_eq!(gpui_items.len(), 2);
        assert!(!gpui_items[0].is_disabled());
        assert!(
            gpui_items[1].is_disabled(),
            "Kit NativeMenu::submenu() drops disabled; From<gpui::Menu> keeps it"
        );
        let bridged = NativeMenu::from(gpui::Menu::new("").items(gpui_items));
        assert!(
            !bridged.is_empty(),
            "From<gpui::Menu> is the NativeMenu path that keeps submenu.disabled"
        );
        assert!(!fill_native_menu(&items, "edit-menu").is_empty());
        let enabled = vec![group("share", "Share", vec![item("link", "Copy link")])];
        assert!(!native_tree_has_disabled_submenu(&enabled));
    }

    #[test]
    fn native_leaf_disabled_wins_over_checked_when_no_icon() {
        let mut checked_disabled = item("wrap", "Word wrap");
        checked_disabled.checked = Some(true);
        checked_disabled.disabled = true;
        assert_eq!(
            native_leaf_kind(&checked_disabled),
            NativeLeafKind::Disabled,
            "Kit cannot combine check+disabled; keep the leaf inert"
        );
        let mut checked = item("wrap", "Word wrap");
        checked.checked = Some(true);
        assert_eq!(native_leaf_kind(&checked), NativeLeafKind::Check);
        let mut icon_disabled = item("copy", "Copy");
        icon_disabled.icon = Some("copy".into());
        icon_disabled.disabled = true;
        icon_disabled.checked = Some(true);
        assert_eq!(
            native_leaf_kind(&icon_disabled),
            NativeLeafKind::IconDisabled
        );
        let mut icon = item("copy", "Copy");
        icon.icon = Some("copy".into());
        icon.checked = Some(true);
        assert_eq!(native_leaf_kind(&icon), NativeLeafKind::Icon);
        assert_eq!(
            native_leaf_kind(&item("copy", "Copy")),
            NativeLeafKind::Plain
        );
    }

    #[test]
    fn command_value_path_reads_omitted_null_string_and_array() {
        assert_eq!(command_value_path(None), None);
        assert_eq!(command_value_path(Some(&json!(null))), Some(vec![]));
        assert_eq!(
            command_value_path(Some(&json!("find"))),
            Some(vec!["find".into()])
        );
        assert_eq!(
            command_value_path(Some(&json!(["project", "open"]))),
            Some(vec!["project".into(), "open".into()])
        );
    }
}
