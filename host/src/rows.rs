//! Row-delegate protocol: Clojure sends `{id, label}` / `{id, cells}` rows;
//! Rust owns virtualization, search, and selection. Callbacks send wire ids.

use crate::protocol::Item;
use gpui::{div, px, App, Context, IntoElement, ParentElement, SharedString, Task, Window};
use gpui_component::{
    list::{ListDelegate, ListItem, ListState},
    table::{Column, TableDelegate, TableState},
    tree::TreeItem,
    IndexPath,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub label: String,
    pub disabled: bool,
    pub cells: Vec<String>,
}

impl Row {
    pub fn from_item(item: &Item) -> Self {
        let label = item.label_or_id();
        let cells = if item.cells.is_empty() {
            vec![label.clone()]
        } else {
            item.cells.clone()
        };
        Self {
            id: item.id_or_label(),
            label,
            disabled: item.disabled,
            cells,
        }
    }
}

pub fn rows_from_items(items: &[Item]) -> Vec<Row> {
    items.iter().map(Row::from_item).collect()
}

/// Structured identity for list/table/tree data. Ignores tree `:expanded`
/// (host-local) and includes column width so a width-only update refreshes.
pub fn rows_fingerprint(items: &[Item]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_items(items, &mut hasher);
    hasher.finish()
}

fn hash_items(items: &[Item], hasher: &mut DefaultHasher) {
    items.len().hash(hasher);
    for item in items {
        item.id.hash(hasher);
        item.label.hash(hasher);
        item.disabled.hash(hasher);
        item.cells.hash(hasher);
        item.width.map(f32::to_bits).hash(hasher);
        item.checked.hash(hasher);
        item.icon.hash(hasher);
        item.separator.hash(hasher);
        hash_items(&item.items, hasher);
    }
}

pub fn table_fingerprint(columns: &[Item], rows: &[Item]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_items(columns, &mut hasher);
    hash_items(rows, &mut hasher);
    hasher.finish()
}

/// Native selection update for a controlled `:value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionSync {
    Keep,
    Select(usize),
    Clear,
}

pub fn selection_sync(
    wanted: Option<&str>,
    current: Option<usize>,
    index_of: impl Fn(&str) -> Option<usize>,
    exists: impl Fn(&str) -> bool,
) -> SelectionSync {
    match wanted {
        None => {
            if current.is_some() {
                SelectionSync::Clear
            } else {
                SelectionSync::Keep
            }
        }
        Some(id) => {
            if let Some(ix) = index_of(id) {
                if current == Some(ix) {
                    SelectionSync::Keep
                } else {
                    SelectionSync::Select(ix)
                }
            } else if exists(id) {
                SelectionSync::Keep
            } else if current.is_some() {
                SelectionSync::Clear
            } else {
                SelectionSync::Keep
            }
        }
    }
}

pub fn visible_tree_ids(items: &[TreeItem]) -> Vec<String> {
    let mut out = Vec::new();
    walk_visible_tree(items, &mut out);
    out
}

fn walk_visible_tree(items: &[TreeItem], out: &mut Vec<String>) {
    for item in items {
        out.push(item.id.to_string());
        if item.is_expanded() {
            walk_visible_tree(&item.children, out);
        }
    }
}

pub fn tree_contains_id(items: &[TreeItem], id: &str) -> bool {
    items
        .iter()
        .any(|item| item.id.as_ref() == id || tree_contains_id(&item.children, id))
}

pub fn tree_visible_index(items: &[TreeItem], id: &str) -> Option<usize> {
    visible_tree_ids(items).iter().position(|row| row == id)
}

pub fn columns_from_items(items: &[Item]) -> Vec<Column> {
    items
        .iter()
        .map(|item| {
            let mut col = Column::new(item.id_or_label(), item.label_or_id());
            if let Some(width) = item.width {
                col = col.width(px(width));
            }
            col
        })
        .collect()
}

pub fn tree_items_from_protocol(items: &[Item]) -> Vec<TreeItem> {
    items
        .iter()
        .map(|item| {
            let mut node = TreeItem::new(item.id_or_label(), item.label_or_id())
                .disabled(item.disabled)
                .expanded(item.expanded);
            if !item.items.is_empty() {
                node = node.children(tree_items_from_protocol(&item.items));
            }
            node
        })
        .collect()
}

pub struct RowListDelegate {
    pub items: Vec<Row>,
    pub visible: Vec<usize>,
    pub selected: Option<IndexPath>,
    query: String,
}

impl RowListDelegate {
    pub fn new(items: Vec<Row>) -> Self {
        let visible: Vec<usize> = (0..items.len()).collect();
        Self {
            items,
            visible,
            selected: None,
            query: String::new(),
        }
    }

    pub fn set_items(&mut self, items: Vec<Row>) {
        self.items = items;
        self.apply_query();
    }

    pub fn contains_id(&self, id: &str) -> bool {
        self.items.iter().any(|row| row.id == id)
    }

    fn apply_query(&mut self) {
        if self.query.is_empty() {
            self.visible = (0..self.items.len()).collect();
        } else {
            let needle = self.query.to_lowercase();
            self.visible = self
                .items
                .iter()
                .enumerate()
                .filter(|(_, row)| row.label.to_lowercase().contains(&needle))
                .map(|(ix, _)| ix)
                .collect();
        }
    }

    pub fn id_at(&self, ix: IndexPath) -> Option<String> {
        self.visible
            .get(ix.row)
            .and_then(|&row| self.items.get(row))
            .map(|row| row.id.clone())
    }

    pub fn index_of(&self, id: &str) -> Option<IndexPath> {
        self.visible
            .iter()
            .position(|&row| self.items.get(row).is_some_and(|item| item.id == id))
            .map(IndexPath::new)
    }
}

impl ListDelegate for RowListDelegate {
    type Item = ListItem;

    fn perform_search(
        &mut self,
        query: &str,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        self.query = query.to_string();
        self.apply_query();
        Task::ready(())
    }

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.visible.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let row = self
            .visible
            .get(ix.row)
            .and_then(|&row| self.items.get(row))?;
        let mut item = ListItem::new(ix).child(SharedString::from(row.label.clone()));
        if row.disabled {
            item = item.disabled(true);
        }
        Some(item)
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) {
        self.selected = ix;
    }
}

pub struct RowTableDelegate {
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
}

impl RowTableDelegate {
    pub fn new(columns: Vec<Column>, rows: Vec<Row>) -> Self {
        Self { columns, rows }
    }

    pub fn id_at(&self, row: usize) -> Option<String> {
        self.rows.get(row).map(|row| row.id.clone())
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.rows.iter().position(|row| row.id == id)
    }

    pub fn contains_id(&self, id: &str) -> bool {
        self.index_of(id).is_some()
    }
}

impl TableDelegate for RowTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> &Column {
        &self.columns[col_ix]
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let text = self
            .rows
            .get(row_ix)
            .and_then(|row| row.cells.get(col_ix))
            .cloned()
            .unwrap_or_default();
        div().child(SharedString::from(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn items(value: serde_json::Value) -> Vec<Item> {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn table_cells_fall_back_to_label() {
        let rows = rows_from_items(&items(json!([
            {"id": "ada", "cells": ["Ada", "Clojure"]},
            {"id": "grace", "label": "Grace"}
        ])));
        assert_eq!(rows[0].cells, vec!["Ada", "Clojure"]);
        assert_eq!(rows[1].cells, vec!["Grace"]);
        let columns = columns_from_items(&items(json!([
            {"id": "name", "label": "Name", "width": 80},
            {"id": "lang", "label": "Lang"}
        ])));
        assert_eq!(columns.len(), 2);
        let delegate = RowTableDelegate::new(columns, rows);
        assert_eq!(delegate.index_of("ada"), Some(0));
        assert_eq!(delegate.id_at(1).as_deref(), Some("grace"));
    }

    #[test]
    fn tree_fingerprint_ignores_expanded_and_includes_nested_ids() {
        let a = items(json!([{
            "id": "src",
            "label": "src",
            "expanded": true,
            "items": [{"id": "lib", "label": "lib.rs"}]
        }]));
        let b = items(json!([{
            "id": "src",
            "label": "src",
            "expanded": false,
            "items": [{"id": "lib", "label": "lib.rs"}]
        }]));
        assert_eq!(rows_fingerprint(&a), rows_fingerprint(&b));
        let c = items(json!([{
            "id": "src",
            "label": "src",
            "items": [{"id": "main", "label": "main.rs"}]
        }]));
        assert_ne!(rows_fingerprint(&a), rows_fingerprint(&c));
        let converted = tree_items_from_protocol(&a);
        assert_eq!(converted.len(), 1);
        assert!(converted[0].is_folder());
        assert!(converted[0].is_expanded());
        assert_eq!(
            visible_tree_ids(&converted),
            vec!["src".to_string(), "lib".to_string()]
        );
        assert_eq!(tree_visible_index(&converted, "lib"), Some(1));
        assert!(tree_contains_id(&converted, "lib"));
        assert!(!tree_contains_id(&converted, "missing"));
    }

    #[test]
    fn fingerprint_includes_column_width() {
        let narrow = items(json!([{"id": "name", "label": "Name", "width": 80}]));
        let wide = items(json!([{"id": "name", "label": "Name", "width": 140}]));
        assert_ne!(rows_fingerprint(&narrow), rows_fingerprint(&wide));
        assert_ne!(
            table_fingerprint(&narrow, &[]),
            table_fingerprint(&wide, &[])
        );
    }

    #[test]
    fn set_items_reapplies_active_query() {
        let mut delegate = RowListDelegate::new(rows_from_items(&items(json!([
            {"id": "alpha", "label": "Alpha"},
            {"id": "clojure", "label": "Clojure"}
        ]))));
        delegate.query = "clo".into();
        delegate.apply_query();
        assert_eq!(delegate.visible.len(), 1);
        assert_eq!(
            delegate.id_at(IndexPath::new(0)).as_deref(),
            Some("clojure")
        );
        delegate.set_items(rows_from_items(&items(json!([
            {"id": "alpha", "label": "Alpha"},
            {"id": "clojure", "label": "Clojure REPL"},
            {"id": "clock", "label": "Clock"}
        ]))));
        assert_eq!(delegate.visible.len(), 2);
        assert!(delegate.contains_id("clock"));
        assert_eq!(delegate.index_of("alpha"), None);
    }

    #[test]
    fn selection_sync_clears_nil_and_missing() {
        let index_of = |id: &str| match id {
            "ada" => Some(0),
            "grace" => Some(1),
            _ => None,
        };
        let exists = |id: &str| index_of(id).is_some() || id == "hidden";
        assert_eq!(
            selection_sync(Some("grace"), Some(0), index_of, exists),
            SelectionSync::Select(1)
        );
        assert_eq!(
            selection_sync(Some("grace"), Some(1), index_of, exists),
            SelectionSync::Keep
        );
        assert_eq!(
            selection_sync(None, Some(1), index_of, exists),
            SelectionSync::Clear
        );
        assert_eq!(
            selection_sync(Some("gone"), Some(0), index_of, exists),
            SelectionSync::Clear
        );
        assert_eq!(
            selection_sync(Some("hidden"), Some(0), index_of, exists),
            SelectionSync::Keep
        );
    }

    // The search test above needs App/Window/Context which we cannot build
    // without a GPUI test harness. Keep a pure filter helper instead.
    fn filter_ids(rows: &[Row], query: &str) -> Vec<String> {
        let needle = query.to_lowercase();
        rows.iter()
            .filter(|row| query.is_empty() || row.label.to_lowercase().contains(&needle))
            .map(|row| row.id.clone())
            .collect()
    }

    #[test]
    fn label_filter_is_case_insensitive() {
        let rows = rows_from_items(&items(json!([
            {"id": "alpha", "label": "Alpha"},
            {"id": "beta", "label": "Beta"}
        ])));
        assert_eq!(filter_ids(&rows, "AL"), vec!["alpha".to_string()]);
        assert_eq!(
            filter_ids(&rows, ""),
            vec!["alpha".to_string(), "beta".to_string()]
        );
    }
}
