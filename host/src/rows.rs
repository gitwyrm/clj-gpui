//! Row-delegate protocol: Clojure sends `{id, label}` / `{id, cells}` rows;
//! a table cell is a string or a nested widget node. Rust owns
//! virtualization, search, and selection. Callbacks send wire ids.

use crate::protocol::{Cmd, Item, TableCell};
use gpui::{
    App, Context, IntoElement, ParentElement, SharedString, Task, TextAlign, Window, div, px,
};
use gpui_component::{
    IndexPath,
    list::{ListDelegate, ListItem, ListState},
    table::{Column, ColumnGroup, TableDelegate, TableState},
    tree::TreeItem,
};
use gpui_kit as gpui;
use gpui_kit::component as gpui_component;
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::mpsc;

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: String,
    pub label: String,
    pub disabled: bool,
    pub cells: Vec<TableCell>,
}

impl Row {
    pub fn from_item(item: &Item) -> Self {
        let label = item.label_or_id();
        let cells = if item.cells.is_empty() {
            vec![TableCell::text(label.clone())]
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
        item.span.hash(hasher);
        item.align.hash(hasher);
        item.selectable.hash(hasher);
        item.checked.hash(hasher);
        item.icon.hash(hasher);
        item.separator.hash(hasher);
        hash_items(&item.items, hasher);
    }
}

/// Column ids/count/order only. Label, width, align, and selectable are
/// `column_definition_fingerprint` so a metadata-only Clojure update after
/// a header drag does not snap native order back to the tree.
pub fn column_identity_fingerprint(items: &[Item]) -> u64 {
    let mut hasher = DefaultHasher::new();
    items.len().hash(&mut hasher);
    for item in items {
        item.id_or_label().hash(&mut hasher);
    }
    hasher.finish()
}

/// Column label/width/align/selectable (full Item hash).
pub fn column_definition_fingerprint(items: &[Item]) -> u64 {
    rows_fingerprint(items)
}

/// Header-group identity only. Row-only Clojure updates must not use a
/// combined table fingerprint that would also rewrite native columns.
pub fn header_groups_fingerprint(groups: &[Vec<Item>]) -> u64 {
    let mut hasher = DefaultHasher::new();
    groups.len().hash(&mut hasher);
    for row in groups {
        hash_items(row, &mut hasher);
    }
    hasher.finish()
}

/// Kit `TableDelegate::group_headers`: each inner vec is one header
/// row of `ColumnGroup { label, span }`. Empty is `None` (no groups).
pub fn header_groups_from_items(rows: &[Vec<Item>]) -> Vec<Vec<ColumnGroup>> {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|item| ColumnGroup::new(item.label_or_id(), item.span.max(1) as usize))
                .collect()
        })
        .filter(|row: &Vec<ColumnGroup>| !row.is_empty())
        .collect()
}

/// Programmatic `TableState::dump` replay token. Same shape as
/// nav-stack `replace-generation` / scroller `scroll-generation`.
pub fn table_export_generation(value: Option<&Value>) -> Option<String> {
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

/// Controlled DataTable selection. String is a row id (existing).
/// `{"row","col"}` or `[row, col]` is Kit `set_selected_cell`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableSelectionWanted {
    Clear,
    Row(String),
    Cell { row: String, col: String },
}

fn json_id(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

pub fn table_selection_wanted(value: Option<&Value>) -> TableSelectionWanted {
    match value {
        None | Some(Value::Null) => TableSelectionWanted::Clear,
        Some(Value::String(s)) => {
            if s.is_empty() {
                TableSelectionWanted::Clear
            } else {
                TableSelectionWanted::Row(s.clone())
            }
        }
        Some(Value::Number(n)) => TableSelectionWanted::Row(n.to_string()),
        Some(Value::Array(items)) if items.len() == 2 => {
            match (json_id(&items[0]), json_id(&items[1])) {
                (Some(row), Some(col)) => TableSelectionWanted::Cell { row, col },
                _ => TableSelectionWanted::Clear,
            }
        }
        Some(Value::Object(map)) => {
            let row = map.get("row").and_then(json_id);
            let col = map.get("col").and_then(json_id);
            match (row, col) {
                (Some(row), Some(col)) => TableSelectionWanted::Cell { row, col },
                (Some(row), None) => TableSelectionWanted::Row(row),
                _ => TableSelectionWanted::Clear,
            }
        }
        Some(other) => TableSelectionWanted::Row(other.to_string()),
    }
}

/// Cell maps / `[row, col]` only drive Kit `set_selected_cell` when
/// `cell-selectable` is on. Otherwise the row id is used so a leftover
/// cell payload cannot switch Kit into cell mode.
pub fn table_selection_for_mode(
    value: Option<&Value>,
    cell_selectable: bool,
) -> TableSelectionWanted {
    match table_selection_wanted(value) {
        TableSelectionWanted::Cell { row, .. } if !cell_selectable => {
            TableSelectionWanted::Row(row)
        }
        other => other,
    }
}

/// Native selection update for a controlled table `:value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableSelectionSync {
    Keep,
    SelectRow(usize),
    SelectCell(usize, usize),
    Clear,
}

pub fn table_selection_sync(
    wanted: &TableSelectionWanted,
    selected_row: Option<usize>,
    selected_cell: Option<(usize, usize)>,
    row_index: impl Fn(&str) -> Option<usize>,
    col_index: impl Fn(&str) -> Option<usize>,
    row_exists: impl Fn(&str) -> bool,
    col_exists: impl Fn(&str) -> bool,
) -> TableSelectionSync {
    match wanted {
        TableSelectionWanted::Clear => {
            if selected_row.is_some() || selected_cell.is_some() {
                TableSelectionSync::Clear
            } else {
                TableSelectionSync::Keep
            }
        }
        TableSelectionWanted::Row(id) => {
            if let Some(ix) = row_index(id) {
                if selected_cell.is_none() && selected_row == Some(ix) {
                    TableSelectionSync::Keep
                } else {
                    TableSelectionSync::SelectRow(ix)
                }
            } else if row_exists(id) {
                TableSelectionSync::Keep
            } else if selected_row.is_some() || selected_cell.is_some() {
                TableSelectionSync::Clear
            } else {
                TableSelectionSync::Keep
            }
        }
        TableSelectionWanted::Cell { row, col } => match (row_index(row), col_index(col)) {
            (Some(r), Some(c)) => {
                if selected_cell == Some((r, c)) {
                    TableSelectionSync::Keep
                } else {
                    TableSelectionSync::SelectCell(r, c)
                }
            }
            _ if row_exists(row) && col_exists(col) => TableSelectionSync::Keep,
            _ if selected_row.is_some() || selected_cell.is_some() => TableSelectionSync::Clear,
            _ => TableSelectionSync::Keep,
        },
    }
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
            if let Some(align) = column_align(item.align.as_deref()) {
                col.align = align;
            }
            if let Some(selectable) = item.selectable {
                col = col.selectable(selectable);
            }
            col
        })
        .collect()
}

/// Kit `TableDelegate::move_column`: reorder columns and matching cells
/// so `cell_text` / `dump` follow the native header order after a drag.
/// Short `:cells` vectors are padded to the column count first so a
/// trailing move cannot leave a present cell under the wrong header.
pub fn move_table_column(columns: &mut Vec<Column>, rows: &mut [Row], col_ix: usize, to_ix: usize) {
    if col_ix == to_ix || col_ix >= columns.len() || to_ix >= columns.len() {
        return;
    }
    let n = columns.len();
    let col = columns.remove(col_ix);
    columns.insert(to_ix, col);
    for row in rows {
        if row.cells.len() < n {
            row.cells.resize(n, TableCell::default());
        }
        let cell = row.cells.remove(col_ix);
        row.cells.insert(to_ix, cell);
    }
}

/// Rebuild `cells` so index `i` belongs to `native_columns[i]`.
/// `cells[j]` is the value for `source_columns[j]` (Clojure order).
pub fn remap_row_cells(
    source_columns: &[Column],
    native_columns: &[Column],
    cells: &[TableCell],
) -> Vec<TableCell> {
    native_columns
        .iter()
        .map(|native| {
            source_columns
                .iter()
                .position(|src| src.key.as_ref() == native.key.as_ref())
                .and_then(|ix| cells.get(ix).cloned())
                .unwrap_or_default()
        })
        .collect()
}

pub fn remap_rows_to_columns(
    source_columns: &[Column],
    native_columns: &[Column],
    mut rows: Vec<Row>,
) -> Vec<Row> {
    for row in &mut rows {
        row.cells = remap_row_cells(source_columns, native_columns, &row.cells);
    }
    rows
}

/// Rebuild Clojure column objects so they follow `native_columns` id order.
pub fn remap_columns_to_native_order(
    clojure_columns: &[Column],
    native_columns: &[Column],
) -> Vec<Column> {
    native_columns
        .iter()
        .map(|native| {
            clojure_columns
                .iter()
                .find(|src| src.key.as_ref() == native.key.as_ref())
                .cloned()
                .unwrap_or_else(|| native.clone())
        })
        .collect()
}

/// Which Kit `TableState` call to make after a Clojure table tree.
///
/// Kit stores user-resized widths in internal `col_groups`.
/// `TableState::refresh()` rebuilds those from delegate `Column::width`,
/// so a row-only or header-group update must not use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableRefreshKind {
    /// Column identity or definition changed. Clojure `:width` wins.
    Refresh,
    /// Header groups changed. Keep runtime column widths.
    RefreshHeaderLayout,
    /// Rows only. Keep runtime column widths and column objects.
    Notify,
}

pub fn table_refresh_kind(
    identity_changed: bool,
    definition_changed: bool,
    rows_changed: bool,
    groups_changed: bool,
) -> Option<TableRefreshKind> {
    if identity_changed || definition_changed {
        Some(TableRefreshKind::Refresh)
    } else if groups_changed {
        Some(TableRefreshKind::RefreshHeaderLayout)
    } else if rows_changed {
        Some(TableRefreshKind::Notify)
    } else {
        None
    }
}

/// `identity_changed`: Clojure column ids/count/order changed — Clojure
/// order wins. Otherwise keep host-owned order (header drag) and remap
/// Clojure column objects and row cells onto it by id.
pub fn merge_table_data(
    native_columns: &[Column],
    clojure_columns: Vec<Column>,
    clojure_rows: Vec<Row>,
    identity_changed: bool,
) -> (Vec<Column>, Vec<Row>) {
    if identity_changed {
        (clojure_columns, clojure_rows)
    } else {
        let rows = remap_rows_to_columns(&clojure_columns, native_columns, clojure_rows);
        let cols = remap_columns_to_native_order(&clojure_columns, native_columns);
        (cols, rows)
    }
}

fn column_align(align: Option<&str>) -> Option<TextAlign> {
    match align?.trim().to_ascii_lowercase().as_str() {
        "right" | "end" => Some(TextAlign::Right),
        "center" => Some(TextAlign::Center),
        "left" | "start" => Some(TextAlign::Left),
        _ => None,
    }
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
    pub header_groups: Vec<Vec<ColumnGroup>>,
    /// Table slot key; prefixes `render_td` element ids.
    path: String,
    cmd_tx: Option<mpsc::Sender<Cmd>>,
}

impl RowTableDelegate {
    pub fn new(columns: Vec<Column>, rows: Vec<Row>) -> Self {
        Self {
            columns,
            rows,
            header_groups: Vec::new(),
            path: String::new(),
            cmd_tx: None,
        }
    }

    pub fn with_header_groups(mut self, groups: Vec<Vec<ColumnGroup>>) -> Self {
        self.header_groups = groups;
        self
    }

    pub fn with_cell_host(mut self, path: impl Into<String>, cmd_tx: mpsc::Sender<Cmd>) -> Self {
        self.path = path.into();
        self.cmd_tx = Some(cmd_tx);
        self
    }

    pub fn col_id_at(&self, col: usize) -> Option<String> {
        self.columns.get(col).map(|col| col.key.to_string())
    }

    pub fn col_index_of(&self, id: &str) -> Option<usize> {
        self.columns.iter().position(|col| col.key.as_ref() == id)
    }

    pub fn contains_col(&self, id: &str) -> bool {
        self.col_index_of(id).is_some()
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

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn group_headers(&self, _: &App) -> Option<Vec<Vec<ColumnGroup>>> {
        if self.header_groups.is_empty() {
            None
        } else {
            Some(self.header_groups.clone())
        }
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let path = format!("{}/td/{row_ix}/{col_ix}", self.path);
        match self.rows.get(row_ix).and_then(|row| row.cells.get(col_ix)) {
            Some(TableCell::Node(node)) => {
                crate::overlay::paint_table_cell(node, &path, self.cmd_tx.as_ref())
            }
            Some(TableCell::Text(text)) => div()
                .child(SharedString::from(text.clone()))
                .into_any_element(),
            None => div().into_any_element(),
        }
    }

    fn cell_text(&self, row_ix: usize, col_ix: usize, _: &App) -> String {
        self.rows
            .get(row_ix)
            .and_then(|row| row.cells.get(col_ix))
            .map(TableCell::export_text)
            .unwrap_or_default()
    }

    fn move_column(
        &mut self,
        col_ix: usize,
        to_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) {
        move_table_column(&mut self.columns, &mut self.rows, col_ix, to_ix);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn items(value: serde_json::Value) -> Vec<Item> {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn table_cells_fall_back_to_label() {
        let rows = rows_from_items(&items(json!([
            {"id": "ada", "cells": ["Ada", "Clojure"]},
            {"id": "grace", "label": "Grace"}
        ])));
        assert_eq!(
            rows[0].cells,
            vec![TableCell::text("Ada"), TableCell::text("Clojure")]
        );
        assert_eq!(rows[1].cells, vec![TableCell::text("Grace")]);
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
    fn table_widget_cells_keep_nodes_and_export_text() {
        let rows = rows_from_items(&items(json!([{
            "id": "ada",
            "cells": [
                "Ada",
                {"type": "progress", "value": 72},
                {"type": "tag", "text": "stable"}
            ]
        }])));
        assert_eq!(rows[0].cells[0], TableCell::text("Ada"));
        assert_eq!(rows[0].cells[1].export_text(), "72");
        assert_eq!(
            rows[0].cells[1].as_node().map(|n| n.kind.as_str()),
            Some("progress")
        );
        assert_eq!(rows[0].cells[2].export_text(), "stable");

        let mut cols = columns_from_items(&items(json!([
            {"id": "name", "label": "Name"},
            {"id": "done", "label": "Done"},
            {"id": "status", "label": "Status"}
        ])));
        let mut moved = rows.clone();
        move_table_column(&mut cols, &mut moved, 1, 2);
        assert_eq!(moved[0].cells[0], TableCell::text("Ada"));
        assert_eq!(moved[0].cells[1].export_text(), "stable");
        assert_eq!(moved[0].cells[2].export_text(), "72");
        assert_eq!(
            moved[0].cells[2].as_node().map(|n| n.kind.as_str()),
            Some("progress")
        );

        let a = items(json!([{"id": "ada", "cells": [{"type": "progress", "value": 40}]}]));
        let b = items(json!([{"id": "ada", "cells": [{"type": "progress", "value": 80}]}]));
        assert_ne!(rows_fingerprint(&a), rows_fingerprint(&b));
        let same = items(json!([{"id": "ada", "cells": [{"type": "progress", "value": 40}]}]));
        assert_eq!(rows_fingerprint(&a), rows_fingerprint(&same));
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
        assert_eq!(
            column_identity_fingerprint(&narrow),
            column_identity_fingerprint(&wide)
        );
        assert_ne!(
            column_definition_fingerprint(&narrow),
            column_definition_fingerprint(&wide)
        );
    }

    #[test]
    fn column_identity_fingerprint_ignores_definition_metadata() {
        let base = items(json!([
            {"id": "name", "label": "Name", "width": 80, "align": "start"},
            {"id": "lang", "label": "Lang"}
        ]));
        let relabeled = items(json!([
            {"id": "name", "label": "Name", "width": 80, "align": "start"},
            {"id": "lang", "label": "Language"}
        ]));
        let aligned = items(json!([
            {"id": "name", "label": "Name", "width": 80, "align": "end"},
            {"id": "lang", "label": "Lang"}
        ]));
        let unselectable = items(json!([
            {"id": "name", "label": "Name", "width": 80, "align": "start", "selectable": false},
            {"id": "lang", "label": "Lang"}
        ]));
        let reordered = items(json!([
            {"id": "lang", "label": "Lang"},
            {"id": "name", "label": "Name", "width": 80, "align": "start"}
        ]));
        assert_eq!(
            column_identity_fingerprint(&base),
            column_identity_fingerprint(&relabeled)
        );
        assert_eq!(
            column_identity_fingerprint(&base),
            column_identity_fingerprint(&aligned)
        );
        assert_eq!(
            column_identity_fingerprint(&base),
            column_identity_fingerprint(&unselectable)
        );
        assert_ne!(
            column_identity_fingerprint(&base),
            column_identity_fingerprint(&reordered)
        );
        assert_ne!(
            column_definition_fingerprint(&base),
            column_definition_fingerprint(&relabeled)
        );
        assert_ne!(
            column_definition_fingerprint(&base),
            column_definition_fingerprint(&aligned)
        );
        assert_ne!(
            column_definition_fingerprint(&base),
            column_definition_fingerprint(&unselectable)
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

    #[test]
    fn header_groups_span_and_cell_text_and_selection() {
        let groups = header_groups_from_items(&[items(json!([
            {"label": "Identity", "span": 2},
            {"label": "Work"}
        ]))]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
        assert_eq!(groups[0][0].span, 2);
        assert_eq!(groups[0][1].span, 1);

        let columns = columns_from_items(&items(json!([
            {"id": "name", "label": "Name", "align": "end", "selectable": false},
            {"id": "lang", "label": "Lang"}
        ])));
        assert_eq!(columns[0].align, TextAlign::Right);
        assert!(!columns[0].selectable);
        assert!(columns[1].selectable);
        let rows = rows_from_items(&items(json!([
            {"id": "ada", "cells": ["Ada", "Clojure"]}
        ])));
        let delegate = RowTableDelegate::new(columns, rows).with_header_groups(groups);
        assert_eq!(delegate.rows[0].cells[1], TableCell::text("Clojure"));
        assert_eq!(delegate.col_index_of("lang"), Some(1));
        assert_eq!(
            table_selection_wanted(Some(&json!({"row": "ada", "col": "lang"}))),
            TableSelectionWanted::Cell {
                row: "ada".into(),
                col: "lang".into()
            }
        );
        assert_eq!(
            table_selection_wanted(Some(&json!(["ada", "lang"]))),
            TableSelectionWanted::Cell {
                row: "ada".into(),
                col: "lang".into()
            }
        );
        assert_eq!(
            table_selection_wanted(Some(&json!("ada"))),
            TableSelectionWanted::Row("ada".into())
        );
        assert_eq!(
            table_selection_for_mode(Some(&json!({"row": "ada", "col": "lang"})), false),
            TableSelectionWanted::Row("ada".into())
        );
        assert_eq!(
            table_selection_for_mode(Some(&json!({"row": "ada", "col": "lang"})), true),
            TableSelectionWanted::Cell {
                row: "ada".into(),
                col: "lang".into()
            }
        );
        assert_eq!(
            table_selection_wanted(Some(&Value::Null)),
            TableSelectionWanted::Clear
        );
        assert_eq!(
            table_selection_sync(
                &TableSelectionWanted::Cell {
                    row: "ada".into(),
                    col: "lang".into()
                },
                Some(0),
                None,
                |_| Some(0),
                |_| Some(1),
                |_| true,
                |_| true,
            ),
            TableSelectionSync::SelectCell(0, 1)
        );
        assert_eq!(
            table_selection_sync(
                &TableSelectionWanted::Cell {
                    row: "ada".into(),
                    col: "lang".into()
                },
                None,
                Some((0, 1)),
                |_| Some(0),
                |_| Some(1),
                |_| true,
                |_| true,
            ),
            TableSelectionSync::Keep
        );
        assert!(table_export_generation(Some(&json!(3))).as_deref() == Some("3"));
        assert!(table_export_generation(Some(&json!(""))).is_none());
        let grouped = items(json!([{"label": "Identity", "span": 2}]));
        assert_ne!(
            header_groups_fingerprint(&[grouped.clone()]),
            header_groups_fingerprint(&[])
        );
        let mut moved_cols = columns_from_items(&items(json!([
            {"id": "name", "label": "Name"},
            {"id": "lang", "label": "Lang"}
        ])));
        let mut moved_rows = rows_from_items(&items(json!([
            {"id": "ada", "cells": ["Ada", "Clojure"]}
        ])));
        move_table_column(&mut moved_cols, &mut moved_rows, 0, 1);
        assert_eq!(moved_cols[0].key.as_ref(), "lang");
        assert_eq!(
            moved_rows[0].cells,
            vec![TableCell::text("Clojure"), TableCell::text("Ada")]
        );
    }

    fn column_keys(columns: &[Column]) -> Vec<String> {
        columns.iter().map(|col| col.key.to_string()).collect()
    }

    #[test]
    fn move_table_column_pads_short_rows() {
        let mut cols = columns_from_items(&items(json!([
            {"id": "a", "label": "A"},
            {"id": "b", "label": "B"},
            {"id": "c", "label": "C"}
        ])));
        let mut rows = rows_from_items(&items(json!([{"id": "r", "cells": ["A"]}])));
        move_table_column(&mut cols, &mut rows, 0, 2);
        assert_eq!(column_keys(&cols), vec!["b", "c", "a"]);
        assert_eq!(
            rows[0].cells,
            vec![
                TableCell::text(""),
                TableCell::text(""),
                TableCell::text("A")
            ]
        );
    }

    #[test]
    fn row_only_merge_keeps_native_column_order() {
        let clojure_cols = columns_from_items(&items(json!([
            {"id": "name", "label": "Name"},
            {"id": "lang", "label": "Lang"}
        ])));
        let mut native = clojure_cols.clone();
        let mut rows = rows_from_items(&items(json!([
            {"id": "ada", "cells": ["Ada", "Clojure"]}
        ])));
        move_table_column(&mut native, &mut rows, 0, 1);
        assert_eq!(column_keys(&native), vec!["lang", "name"]);
        assert_eq!(
            rows[0].cells,
            vec![TableCell::text("Clojure"), TableCell::text("Ada")]
        );

        let updated = rows_from_items(&items(json!([
            {"id": "grace", "cells": ["Grace", "Rust"]},
            {"id": "alan", "cells": ["Alan", "Go"]}
        ])));
        let (kept, remapped) = merge_table_data(&native, clojure_cols.clone(), updated, false);
        assert_eq!(column_keys(&kept), vec!["lang", "name"]);
        assert_eq!(remapped[0].id, "grace");
        assert_eq!(
            remapped[0].cells,
            vec![TableCell::text("Rust"), TableCell::text("Grace")]
        );
        assert_eq!(
            remapped[1].cells,
            vec![TableCell::text("Go"), TableCell::text("Alan")]
        );

        let (replaced, clojure_order) =
            merge_table_data(&native, clojure_cols, remapped.clone(), true);
        assert_eq!(column_keys(&replaced), vec!["name", "lang"]);
        assert_eq!(
            clojure_order[0].cells,
            vec![TableCell::text("Rust"), TableCell::text("Grace")]
        );
    }

    #[test]
    fn column_definition_merge_keeps_native_order() {
        let clojure_cols = columns_from_items(&items(json!([
            {"id": "name", "label": "Name", "width": 80, "align": "start"},
            {"id": "lang", "label": "Lang", "width": 100}
        ])));
        let clojure_rows = rows_from_items(&items(json!([
            {"id": "ada", "cells": ["Ada", "Clojure"]}
        ])));
        let mut native = clojure_cols.clone();
        let mut native_rows = clojure_rows.clone();
        move_table_column(&mut native, &mut native_rows, 0, 1);
        assert_eq!(column_keys(&native), vec!["lang", "name"]);
        assert_eq!(
            native_rows[0].cells,
            vec![TableCell::text("Clojure"), TableCell::text("Ada")]
        );

        // Clojure still sends cells in tree column order. Merge remaps them
        // (and rebuilt column objects) onto the host-owned header order.
        let relabeled = columns_from_items(&items(json!([
            {"id": "name", "label": "Name", "width": 80, "align": "start"},
            {"id": "lang", "label": "Language", "width": 100}
        ])));
        let (cols, remapped) = merge_table_data(&native, relabeled, clojure_rows.clone(), false);
        assert_eq!(column_keys(&cols), vec!["lang", "name"]);
        assert_eq!(cols[0].name.as_ref(), "Language");
        assert_eq!(cols[1].name.as_ref(), "Name");
        assert_eq!(
            remapped[0].cells,
            vec![TableCell::text("Clojure"), TableCell::text("Ada")]
        );

        let restyled = columns_from_items(&items(json!([
            {"id": "name", "label": "Name", "width": 140, "align": "end", "selectable": false},
            {"id": "lang", "label": "Language", "width": 100}
        ])));
        let (cols, remapped) = merge_table_data(&native, restyled, clojure_rows.clone(), false);
        assert_eq!(column_keys(&cols), vec!["lang", "name"]);
        assert_eq!(cols[0].name.as_ref(), "Language");
        assert_eq!(cols[1].width, px(140.));
        assert_eq!(cols[1].align, TextAlign::Right);
        assert!(!cols[1].selectable);
        assert_eq!(
            remapped[0].cells,
            vec![TableCell::text("Clojure"), TableCell::text("Ada")]
        );

        let replaced = columns_from_items(&items(json!([
            {"id": "name", "label": "Name"},
            {"id": "dialect", "label": "Dialect"}
        ])));
        let replaced_rows = rows_from_items(&items(json!([
            {"id": "ada", "cells": ["Ada", "Lisp"]}
        ])));
        let (cols, clojure_order) = merge_table_data(&native, replaced, replaced_rows, true);
        assert_eq!(column_keys(&cols), vec!["name", "dialect"]);
        assert_eq!(
            clojure_order[0].cells,
            vec![TableCell::text("Ada"), TableCell::text("Lisp")]
        );
    }

    /// Kit `TableState::refresh` rebuilds `col_groups` from `Column::width`.
    /// Row-only and header-group updates must not take that path.
    #[test]
    fn native_column_width_survives_row_and_header_group_sync() {
        assert_eq!(
            table_refresh_kind(false, false, true, false),
            Some(TableRefreshKind::Notify)
        );
        assert_eq!(
            table_refresh_kind(false, false, false, true),
            Some(TableRefreshKind::RefreshHeaderLayout)
        );
        assert_eq!(
            table_refresh_kind(false, false, true, true),
            Some(TableRefreshKind::RefreshHeaderLayout)
        );
        assert_eq!(
            table_refresh_kind(false, true, false, false),
            Some(TableRefreshKind::Refresh)
        );
        assert_eq!(
            table_refresh_kind(true, false, true, false),
            Some(TableRefreshKind::Refresh)
        );
        assert_eq!(table_refresh_kind(false, false, false, false), None);

        let cols = items(json!([{"id": "name", "label": "Name", "width": 100}]));
        let rows_a = items(json!([{"id": "ada", "cells": ["Ada"]}]));
        let rows_b = items(json!([{"id": "grace", "cells": ["Grace"]}]));
        let groups_a = vec![items(json!([{"label": "Identity", "span": 1}]))];
        let groups_b = vec![items(json!([{"label": "People", "span": 1}]))];
        let wide = items(json!([{"id": "name", "label": "Name", "width": 140}]));

        // Native resize lives in Kit col_groups, not the Clojure :width.
        let mut runtime_width = 180.0;
        let clojure_width = 100.0;

        let row_only = table_refresh_kind(
            false,
            false,
            rows_fingerprint(&rows_a) != rows_fingerprint(&rows_b),
            false,
        );
        apply_kit_col_groups(&mut runtime_width, clojure_width, row_only);
        assert_eq!(runtime_width, 180.0);

        let groups_only = table_refresh_kind(
            false,
            false,
            false,
            header_groups_fingerprint(&groups_a) != header_groups_fingerprint(&groups_b),
        );
        apply_kit_col_groups(&mut runtime_width, clojure_width, groups_only);
        assert_eq!(runtime_width, 180.0);

        let definition = table_refresh_kind(
            column_identity_fingerprint(&cols) != column_identity_fingerprint(&wide),
            column_definition_fingerprint(&cols) != column_definition_fingerprint(&wide),
            false,
            false,
        );
        apply_kit_col_groups(&mut runtime_width, 140.0, definition);
        assert_eq!(runtime_width, 140.0);
    }

    /// Kit `prepare_col_groups` copies `Column::width` into runtime groups.
    fn apply_kit_col_groups(
        runtime_width: &mut f32,
        column_width: f32,
        kind: Option<TableRefreshKind>,
    ) {
        if kind == Some(TableRefreshKind::Refresh) {
            *runtime_width = column_width;
        }
    }
}
