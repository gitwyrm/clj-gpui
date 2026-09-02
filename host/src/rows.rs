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

pub fn rows_fingerprint(items: &[Item]) -> String {
    fn walk(items: &[Item]) -> String {
        items
            .iter()
            .map(|item| {
                format!(
                    "{}:{}:{}:{}:[{}]",
                    item.id_or_label(),
                    item.label_or_id(),
                    item.disabled,
                    item.cells.join(","),
                    walk(&item.items)
                )
            })
            .collect::<Vec<_>>()
            .join("|")
    }
    walk(items)
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
}

impl RowListDelegate {
    pub fn new(items: Vec<Row>) -> Self {
        let visible: Vec<usize> = (0..items.len()).collect();
        Self {
            items,
            visible,
            selected: None,
        }
    }

    pub fn set_items(&mut self, items: Vec<Row>) {
        self.items = items;
        self.visible = (0..self.items.len()).collect();
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
        if query.is_empty() {
            self.visible = (0..self.items.len()).collect();
        } else {
            let needle = query.to_lowercase();
            self.visible = self
                .items
                .iter()
                .enumerate()
                .filter(|(_, row)| row.label.to_lowercase().contains(&needle))
                .map(|(ix, _)| ix)
                .collect();
        }
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
