//! Row-delegate protocol: Clojure sends `{id, label}` / `{id, cells}` rows;
//! a table cell is a string or a supported RenderOnce node. Rust owns
//! virtualization, search, and selection. Callbacks send wire ids.

use crate::protocol::{Cmd, Item, TableCell};
use gpui::{
    App, Context, IntoElement, ParentElement, SharedString, Styled, Task, TextAlign, Window, div,
    px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, IndexPath, h_flex,
    list::{ListDelegate, ListItem, ListState},
    table::{Column, ColumnGroup, ColumnSort, TableDelegate, TableEvent, TableState},
    tree::TreeItem,
};
use gpui_kit as gpui;
use gpui_kit::component as gpui_component;
use serde_json::{Value, json};
use std::cell::Cell;
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
        item.sort.hash(hasher);
        item.fixed.hash(hasher);
        item.resizable.hash(hasher);
        item.movable.hash(hasher);
        item.min_width.map(f32::to_bits).hash(hasher);
        item.max_width.map(f32::to_bits).hash(hasher);
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

/// Column label/width/align/selectable/sort/fixed/resize (full Item hash).
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

/// Host-tracked logical table selection. Kit's `SelectionMode` is private
/// and `selected_cell()` / `selected_row()` return stored fields even after
/// the active mode has switched, so `is_some()` is not the active mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TableSelectionMode {
    #[default]
    None,
    Row,
    Cell,
    Column,
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
    mode: TableSelectionMode,
    selected_row: Option<usize>,
    selected_cell: Option<(usize, usize)>,
    row_index: impl Fn(&str) -> Option<usize>,
    col_index: impl Fn(&str) -> Option<usize>,
    row_exists: impl Fn(&str) -> bool,
    col_exists: impl Fn(&str) -> bool,
) -> TableSelectionSync {
    match wanted {
        TableSelectionWanted::Clear => {
            if matches!(mode, TableSelectionMode::None) {
                TableSelectionSync::Keep
            } else {
                TableSelectionSync::Clear
            }
        }
        TableSelectionWanted::Row(id) => {
            if let Some(ix) = row_index(id) {
                if mode == TableSelectionMode::Row && selected_row == Some(ix) {
                    TableSelectionSync::Keep
                } else {
                    TableSelectionSync::SelectRow(ix)
                }
            } else if row_exists(id) {
                TableSelectionSync::Keep
            } else if matches!(mode, TableSelectionMode::None) {
                TableSelectionSync::Keep
            } else {
                TableSelectionSync::Clear
            }
        }
        TableSelectionWanted::Cell { row, col } => match (row_index(row), col_index(col)) {
            (Some(r), Some(c)) => {
                if mode == TableSelectionMode::Cell && selected_cell == Some((r, c)) {
                    TableSelectionSync::Keep
                } else {
                    TableSelectionSync::SelectCell(r, c)
                }
            }
            _ if row_exists(row) && col_exists(col) => TableSelectionSync::Keep,
            _ if matches!(mode, TableSelectionMode::None) => TableSelectionSync::Keep,
            _ => TableSelectionSync::Clear,
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
            if let Some(sort) = column_sort(item.sort.as_deref()) {
                col = col.sort(sort);
            }
            if column_fixed(item.fixed.as_deref()) {
                col = col.fixed_left();
            }
            if let Some(resizable) = item.resizable {
                col = col.resizable(resizable);
            }
            if let Some(movable) = item.movable {
                col = col.movable(movable);
            }
            if let Some(min_width) = item.min_width.filter(|w| w.is_finite() && *w > 0.0) {
                col = col.min_width(px(min_width));
            }
            if let Some(max_width) = item.max_width.filter(|w| w.is_finite() && *w > 0.0) {
                col = col.max_width(px(max_width));
            }
            col
        })
        .collect()
}

fn column_sort(value: Option<&str>) -> Option<ColumnSort> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "asc" | "ascending" => Some(ColumnSort::Ascending),
        "desc" | "descending" => Some(ColumnSort::Descending),
        "default" | "true" | "sortable" => Some(ColumnSort::Default),
        "false" | "none" | "" => None,
        _ => Some(ColumnSort::Default),
    }
}

fn column_fixed(value: Option<&str>) -> bool {
    matches!(
        value.map(|s| s.trim().to_ascii_lowercase()),
        Some(name) if name == "left" || name == "true" || name == "1"
    )
}

fn cell_sort_key(row: &Row, col_ix: usize) -> String {
    row.cells
        .get(col_ix)
        .map(TableCell::export_text)
        .unwrap_or_default()
}

fn cmp_cell(a: &Row, b: &Row, col_ix: usize) -> std::cmp::Ordering {
    let sa = cell_sort_key(a, col_ix);
    let sb = cell_sort_key(b, col_ix);
    match (sa.parse::<f64>(), sb.parse::<f64>()) {
        (Ok(na), Ok(nb)) => na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal),
        _ => sa.to_lowercase().cmp(&sb.to_lowercase()),
    }
}

/// Sort displayed rows. `Default` restores `source_order` (last Clojure tree).
pub fn sort_table_rows(rows: &mut [Row], col_ix: usize, sort: ColumnSort, source_order: &[String]) {
    match sort {
        ColumnSort::Default => rows.sort_by_key(|row| {
            source_order
                .iter()
                .position(|id| id == &row.id)
                .unwrap_or(usize::MAX)
        }),
        ColumnSort::Ascending => rows.sort_by(|a, b| cmp_cell(a, b, col_ix)),
        ColumnSort::Descending => rows.sort_by(|a, b| cmp_cell(b, a, col_ix)),
    }
}

/// After a Clojure column merge, re-apply an active Asc/Desc so row order
/// matches header chrome. Default / no-sort restores `source_order`.
pub fn apply_active_column_sort(columns: &[Column], rows: &mut Vec<Row>, source_order: &[String]) {
    if let Some((ix, sort)) = columns.iter().enumerate().find_map(|(ix, col)| {
        col.sort
            .filter(|sort| !matches!(sort, ColumnSort::Default))
            .map(|sort| (ix, sort))
    }) {
        sort_table_rows(rows, ix, sort, source_order);
    } else {
        sort_table_rows(rows, 0, ColumnSort::Default, source_order);
    }
}

/// Kit stores `selected_row` / `selected_cell` as indices. Native sort
/// reorders displayed rows; remap those indices by stable row id so the
/// same Clojure row (and cell) stays selected.
pub fn remap_table_selection_after_sort(
    old_ids: &[String],
    new_ids: &[String],
    selected_row: Option<usize>,
    selected_cell: Option<(usize, usize)>,
) -> (Option<usize>, Option<(usize, usize)>) {
    let map_row = |ix: usize| -> Option<usize> {
        old_ids
            .get(ix)
            .and_then(|id| new_ids.iter().position(|next| next == id))
    };
    (
        selected_row.and_then(map_row),
        selected_cell.and_then(|(row, col)| map_row(row).map(|row| (row, col))),
    )
}

/// Kit index update after a native sort. Logical row/cell identity is
/// unchanged; callers must suppress Clojure `:on-change`.
///
/// Kit `selected_cell()` returns the stored field even after
/// `set_selected_row` (and `selected_row` survives `set_selected_cell`).
/// Callers pass the host-tracked logical `mode` rather than inferring
/// it from which slots are `Some`.
pub fn table_sort_selection_sync(
    old_ids: &[String],
    new_ids: &[String],
    mode: TableSelectionMode,
    selected_row: Option<usize>,
    selected_cell: Option<(usize, usize)>,
) -> TableSelectionSync {
    let (next_row, next_cell) =
        remap_table_selection_after_sort(old_ids, new_ids, selected_row, selected_cell);
    match mode {
        TableSelectionMode::Cell => {
            if selected_cell == next_cell {
                TableSelectionSync::Keep
            } else {
                match next_cell {
                    Some((row, col)) => TableSelectionSync::SelectCell(row, col),
                    None => TableSelectionSync::Clear,
                }
            }
        }
        TableSelectionMode::Row => {
            if selected_row == next_row {
                TableSelectionSync::Keep
            } else {
                match next_row {
                    Some(row) => TableSelectionSync::SelectRow(row),
                    None => TableSelectionSync::Clear,
                }
            }
        }
        TableSelectionMode::Column | TableSelectionMode::None => TableSelectionSync::Keep,
    }
}

/// Kit `SelectionMode` is private. Public `TableEvent`s are the source
/// of truth for the host-tracked logical mode.
pub fn table_selection_mode_from_kit_event(event: &TableEvent) -> Option<TableSelectionMode> {
    match event {
        TableEvent::SelectRow(_) => Some(TableSelectionMode::Row),
        TableEvent::SelectCell(_, _) => Some(TableSelectionMode::Cell),
        TableEvent::SelectColumn(_) => Some(TableSelectionMode::Column),
        TableEvent::ClearSelection => Some(TableSelectionMode::None),
        _ => None,
    }
}

/// Native sort remaps indices only. Any Kit select/clear from that remap
/// is internal bookkeeping, not an application selection change.
pub fn table_sort_remap_suppresses_on_change(action: TableSelectionSync) -> bool {
    !matches!(action, TableSelectionSync::Keep)
}

/// Reset the load-more latch when the collection or flags change, or when
/// a callback appears (`None` → `Some`). A present `cb-N` becoming
/// `cb-N+1` is Clojure sanitizer churn, not a new handler.
fn load_more_latch_resets(
    collection_changed: bool,
    has_more: bool,
    loading: bool,
    had_callback: bool,
    has_callback: bool,
) -> bool {
    collection_changed || !has_more || loading || (!had_callback && has_callback)
}

fn send_load_more_if_ready(
    has_more: bool,
    loading: bool,
    load_more_sent: &mut bool,
    cmd_tx: &Option<mpsc::Sender<Cmd>>,
    on_load_more: &Option<String>,
) {
    if !has_more || loading || *load_more_sent {
        return;
    }
    let Some(tx) = cmd_tx else {
        return;
    };
    let Some(id) = on_load_more.clone() else {
        return;
    };
    *load_more_sent = true;
    let _ = tx.send(Cmd::Callback {
        id,
        value: None,
        seq: None,
    });
}

fn column_sort_name(sort: ColumnSort) -> &'static str {
    match sort {
        ColumnSort::Ascending => "asc",
        ColumnSort::Descending => "desc",
        ColumnSort::Default => "default",
    }
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
    loading: bool,
    has_more: bool,
    load_more_threshold: usize,
    empty: Option<String>,
    on_load_more: Option<String>,
    load_more_sent: bool,
    cmd_tx: Option<std::sync::mpsc::Sender<Cmd>>,
}

impl RowListDelegate {
    pub fn new(items: Vec<Row>) -> Self {
        let visible: Vec<usize> = (0..items.len()).collect();
        Self {
            items,
            visible,
            selected: None,
            query: String::new(),
            loading: false,
            has_more: false,
            load_more_threshold: 20,
            empty: None,
            on_load_more: None,
            load_more_sent: false,
            cmd_tx: None,
        }
    }

    pub fn with_host(mut self, cmd_tx: std::sync::mpsc::Sender<Cmd>) -> Self {
        self.cmd_tx = Some(cmd_tx);
        self
    }

    pub fn sync_chrome(
        &mut self,
        loading: bool,
        has_more: bool,
        load_more_threshold: usize,
        empty: Option<String>,
        on_load_more: Option<String>,
        collection_changed: bool,
    ) {
        let reset = load_more_latch_resets(
            collection_changed,
            has_more,
            loading,
            self.on_load_more.is_some(),
            on_load_more.is_some(),
        );
        self.loading = loading;
        self.has_more = has_more;
        self.load_more_threshold = load_more_threshold;
        self.empty = empty;
        self.on_load_more = on_load_more;
        if reset {
            self.load_more_sent = false;
        }
    }

    #[cfg(test)]
    pub fn load_more_sent(&self) -> bool {
        self.load_more_sent
    }

    #[cfg(test)]
    pub fn mark_load_more_sent(&mut self) {
        self.load_more_sent = true;
    }

    #[cfg(test)]
    pub fn fire_load_more(&mut self) {
        send_load_more_if_ready(
            self.has_more,
            self.loading,
            &mut self.load_more_sent,
            &self.cmd_tx,
            &self.on_load_more,
        );
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

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        let el = h_flex()
            .size_full()
            .justify_center()
            .text_color(cx.theme().muted_foreground.opacity(0.6));
        if let Some(text) = self.empty.as_deref().filter(|s| !s.is_empty()) {
            el.child(text.to_string()).into_any_element()
        } else {
            el.child(Icon::new(IconName::Inbox).size_12())
                .into_any_element()
        }
    }

    fn loading(&self, _: &App) -> bool {
        self.loading
    }

    fn has_more(&self, _: &App) -> bool {
        self.has_more
    }

    fn load_more_threshold(&self) -> usize {
        self.load_more_threshold
    }

    fn load_more(&mut self, _: &mut Window, _: &mut Context<ListState<Self>>) {
        send_load_more_if_ready(
            self.has_more,
            self.loading,
            &mut self.load_more_sent,
            &self.cmd_tx,
            &self.on_load_more,
        );
    }
}

pub struct RowTableDelegate {
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    pub header_groups: Vec<Vec<ColumnGroup>>,
    /// Last Clojure row-id order; `ColumnSort::Default` restores this.
    source_order: Vec<String>,
    /// Table slot key; prefixes `render_td` element ids.
    path: String,
    cmd_tx: Option<mpsc::Sender<Cmd>>,
    loading: bool,
    has_more: bool,
    load_more_threshold: usize,
    empty: Option<String>,
    on_load_more: Option<String>,
    on_sort: Option<String>,
    load_more_sent: bool,
    /// True while a native-sort index remap is applying Kit selection.
    /// `SelectRow` / `SelectCell` must not become Clojure `:on-change`.
    suppress_select: bool,
    /// Logical Row/Cell/Column/None. Kit leaves the other selection
    /// slots populated when the active mode switches, and `SelectionMode`
    /// is private, so sort remap cannot trust `selected_cell().is_some()`.
    /// Interior mutability so a `TableEvent` subscriber can record the
    /// mode without a nested entity update.
    selection_mode: Cell<TableSelectionMode>,
}

impl RowTableDelegate {
    pub fn new(columns: Vec<Column>, rows: Vec<Row>) -> Self {
        let source_order: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        let mut this = Self {
            columns,
            rows,
            header_groups: Vec::new(),
            source_order,
            path: String::new(),
            cmd_tx: None,
            loading: false,
            has_more: false,
            load_more_threshold: 20,
            empty: None,
            on_load_more: None,
            on_sort: None,
            load_more_sent: false,
            suppress_select: false,
            selection_mode: Cell::new(TableSelectionMode::None),
        };
        apply_active_column_sort(&this.columns, &mut this.rows, &this.source_order);
        this
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

    pub fn sync_chrome(
        &mut self,
        loading: bool,
        has_more: bool,
        load_more_threshold: usize,
        empty: Option<String>,
        on_load_more: Option<String>,
        on_sort: Option<String>,
        collection_changed: bool,
    ) {
        let reset = load_more_latch_resets(
            collection_changed,
            has_more,
            loading,
            self.on_load_more.is_some(),
            on_load_more.is_some(),
        );
        self.loading = loading;
        self.has_more = has_more;
        self.load_more_threshold = load_more_threshold;
        self.empty = empty;
        self.on_load_more = on_load_more;
        self.on_sort = on_sort;
        if reset {
            self.load_more_sent = false;
        }
    }

    #[cfg(test)]
    pub fn load_more_sent(&self) -> bool {
        self.load_more_sent
    }

    #[cfg(test)]
    pub fn fire_load_more(&mut self) {
        send_load_more_if_ready(
            self.has_more,
            self.loading,
            &mut self.load_more_sent,
            &self.cmd_tx,
            &self.on_load_more,
        );
    }

    #[cfg(test)]
    pub fn source_ids(&self) -> &[String] {
        &self.source_order
    }

    pub fn suppress_select(&self) -> bool {
        self.suppress_select
    }

    pub fn selection_mode(&self) -> TableSelectionMode {
        self.selection_mode.get()
    }

    pub fn set_selection_mode(&self, mode: TableSelectionMode) {
        self.selection_mode.set(mode);
    }

    pub fn set_rows(&mut self, rows: Vec<Row>) {
        self.source_order = rows.iter().map(|row| row.id.clone()).collect();
        self.rows = rows;
        apply_active_column_sort(&self.columns, &mut self.rows, &self.source_order);
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

    /// Logical cell path: `table-key/td/row/{id|index}/…/col/{id|index}/…`.
    pub(crate) fn td_element_id(&self, row_ix: usize, col_ix: usize) -> String {
        let row = self
            .rows
            .get(row_ix)
            .map(|row| row.id.as_str())
            .map(|id| table_cell_axis(id, row_ix))
            .unwrap_or(TableCellAxis::Index(row_ix));
        let col = self
            .columns
            .get(col_ix)
            .map(|col| table_cell_axis(col.key.as_ref(), col_ix))
            .unwrap_or(TableCellAxis::Index(col_ix));
        table_cell_element_id(&self.path, &row, &col)
    }
}

/// Length-prefixed wire id so `/` in namespaced keywords (`user/ada`)
/// cannot split a cell path. `user/ada` encodes as `8:user/ada`.
pub(crate) fn encode_wire_id(id: &str) -> String {
    format!("{}:{id}", id.len())
}

/// Named wire id, or a positional fallback when the id is missing.
/// Index fallbacks are a separate namespace from real ids (`"#0"` is not
/// row 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TableCellAxis {
    Id(String),
    Index(usize),
}

fn table_cell_axis(id: &str, ix: usize) -> TableCellAxis {
    if id.is_empty() {
        TableCellAxis::Index(ix)
    } else {
        TableCellAxis::Id(id.to_string())
    }
}

fn encode_row_axis(axis: &TableCellAxis) -> String {
    match axis {
        TableCellAxis::Id(id) => format!("row/id/{}", encode_wire_id(id)),
        TableCellAxis::Index(ix) => format!("row/index/{ix}"),
    }
}

fn encode_col_axis(axis: &TableCellAxis) -> String {
    match axis {
        TableCellAxis::Id(id) => format!("col/id/{}", encode_wire_id(id)),
        TableCellAxis::Index(ix) => format!("col/index/{ix}"),
    }
}

/// GPUI element id for a table cell widget. Logical row/column ids, not
/// visible indices, so header-drag reorder keeps retained widget state
/// (Progress `transition`, HoverCard, …).
pub(crate) fn table_cell_element_id(
    table_key: &str,
    row: &TableCellAxis,
    col: &TableCellAxis,
) -> String {
    format!(
        "{table_key}/td/{}/{}",
        encode_row_axis(row),
        encode_col_axis(col)
    )
}

pub(crate) fn table_cell_row_id(row: &Item, row_ix: usize) -> TableCellAxis {
    table_cell_axis(&row.id_or_label(), row_ix)
}

pub(crate) fn table_cell_col_id(columns: &[Item], col_ix: usize) -> TableCellAxis {
    columns
        .get(col_ix)
        .map(|col| table_cell_axis(&col.id_or_label(), col_ix))
        .unwrap_or(TableCellAxis::Index(col_ix))
}

/// Fallback/declarative `options`/`items` table cell path. Same encoding
/// as DataTable `render_td`.
pub(crate) fn table_row_cell_path(
    table_key: &str,
    row: &Item,
    row_ix: usize,
    columns: &[Item],
    col_ix: usize,
) -> String {
    table_cell_element_id(
        table_key,
        &table_cell_row_id(row, row_ix),
        &table_cell_col_id(columns, col_ix),
    )
}

fn paint_td(inner: impl IntoElement) -> gpui::AnyElement {
    // Kit's cell is a column flex (`div().h_full()`). `h_full()` alone
    // shrink-wraps to the widget, so Progress sits on the top edge.
    // `flex_1` grows along that column; `h_flex` already `items_center`s.
    h_flex()
        .flex_1()
        .size_full()
        .child(inner)
        .into_any_element()
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
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let path = self.td_element_id(row_ix, col_ix);
        match self.rows.get(row_ix).and_then(|row| row.cells.get(col_ix)) {
            Some(TableCell::Node(node)) => paint_td(crate::overlay::paint_table_cell(
                node,
                &path,
                self.cmd_tx.as_ref(),
                Some(cx),
            )),
            Some(TableCell::Text(text)) => paint_td(div().child(SharedString::from(text.clone()))),
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

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        for (ix, col) in self.columns.iter_mut().enumerate() {
            if col.sort.is_some() {
                col.sort = Some(if ix == col_ix {
                    sort
                } else {
                    ColumnSort::Default
                });
            }
        }
        let old_ids: Vec<String> = self.rows.iter().map(|row| row.id.clone()).collect();
        sort_table_rows(&mut self.rows, col_ix, sort, &self.source_order);
        let new_ids: Vec<String> = self.rows.iter().map(|row| row.id.clone()).collect();
        if old_ids != new_ids {
            cx.defer_in(window, move |table, window, cx| {
                let action = table_sort_selection_sync(
                    &old_ids,
                    &new_ids,
                    table.delegate().selection_mode(),
                    table.selected_row(),
                    table.selected_cell(),
                );
                if matches!(action, TableSelectionSync::Keep) {
                    return;
                }
                table.delegate_mut().suppress_select = true;
                match action {
                    TableSelectionSync::SelectRow(ix) => {
                        table.delegate().set_selection_mode(TableSelectionMode::Row);
                        table.set_selected_row(ix, cx)
                    }
                    TableSelectionSync::SelectCell(row, col) => {
                        table
                            .delegate()
                            .set_selection_mode(TableSelectionMode::Cell);
                        table.set_selected_cell(row, col, cx)
                    }
                    TableSelectionSync::Clear => {
                        table
                            .delegate()
                            .set_selection_mode(TableSelectionMode::None);
                        table.clear_selection(cx)
                    }
                    TableSelectionSync::Keep => {}
                }
                cx.defer_in(window, |table, _, _| {
                    table.delegate_mut().suppress_select = false;
                });
            });
        }
        if let (Some(tx), Some(callback), Some(col)) =
            (&self.cmd_tx, self.on_sort.clone(), self.columns.get(col_ix))
        {
            let _ = tx.send(Cmd::Callback {
                id: callback,
                value: Some(json!({
                    "id": col.key.to_string(),
                    "sort": column_sort_name(sort)
                })),
                seq: None,
            });
        }
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let el = h_flex()
            .size_full()
            .justify_center()
            .text_color(cx.theme().muted_foreground.opacity(0.6));
        if let Some(text) = self.empty.as_deref().filter(|s| !s.is_empty()) {
            el.child(text.to_string()).into_any_element()
        } else {
            el.child(Icon::new(IconName::Inbox).size_12())
                .into_any_element()
        }
    }

    fn loading(&self, _: &App) -> bool {
        self.loading
    }

    fn has_more(&self, _: &App) -> bool {
        self.has_more
    }

    fn load_more_threshold(&self) -> usize {
        self.load_more_threshold
    }

    fn load_more(&mut self, _: &mut Window, _: &mut Context<TableState<Self>>) {
        send_load_more_if_ready(
            self.has_more,
            self.loading,
            &mut self.load_more_sent,
            &self.cmd_tx,
            &self.on_load_more,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::component::table::ColumnFixed;
    use serde_json::{Value, json};

    fn items(value: serde_json::Value) -> Vec<Item> {
        serde_json::from_value(value).unwrap()
    }

    fn id(s: &str) -> TableCellAxis {
        TableCellAxis::Id(s.to_string())
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
    fn table_cell_element_id_is_logical_and_unambiguous() {
        assert_eq!(
            table_cell_element_id("tbl", &id("ada"), &id("done")),
            "tbl/td/row/id/3:ada/col/id/4:done"
        );
        // Namespaced keywords keep `/` on the wire; length-prefixing
        // keeps (row, col) pairs distinct under naïve `/` splits.
        assert_eq!(
            table_cell_element_id("tbl", &id("user/ada"), &id("col/done")),
            "tbl/td/row/id/8:user/ada/col/id/8:col/done"
        );
        assert_ne!(
            table_cell_element_id("tbl", &id("user/ada"), &id("done")),
            table_cell_element_id("tbl", &id("user"), &id("ada/done"))
        );
        assert_ne!(
            table_cell_element_id("tbl", &id("user/ada"), &id("x")),
            table_cell_element_id("tbl", &id("user"), &id("ada/x"))
        );
    }

    #[test]
    fn render_td_id_follows_column_key_after_header_drag() {
        let (tx, _) = mpsc::channel();
        let cols = columns_from_items(&items(json!([
            {"id": "name", "label": "Name"},
            {"id": "done", "label": "Done"},
            {"id": "status", "label": "Status"}
        ])));
        let rows = rows_from_items(&items(json!([{
            "id": "ada",
            "cells": [
                "Ada",
                {"type": "progress", "value": 80},
                {"type": "tag", "text": "stable"}
            ]
        }])));
        let mut delegate = RowTableDelegate::new(cols, rows).with_cell_host("tbl", tx);
        let progress = delegate.td_element_id(0, 1);
        assert_eq!(progress, "tbl/td/row/id/3:ada/col/id/4:done");
        move_table_column(&mut delegate.columns, &mut delegate.rows, 1, 2);
        assert_eq!(delegate.td_element_id(0, 2), progress);
        assert_eq!(
            delegate.td_element_id(0, 1),
            "tbl/td/row/id/3:ada/col/id/6:status"
        );
        assert_ne!(delegate.td_element_id(0, 1), progress);
    }

    #[test]
    fn fallback_table_cell_paths_include_row_and_column_identity() {
        let rows = items(json!([
            {"id": "ada", "cells": [{"type": "progress", "value": 80}]},
            {"id": "grace", "cells": [{"type": "progress", "value": 45}]}
        ]));
        let cols = items(json!([{"id": "done", "label": "Done"}]));
        let ada = table_row_cell_path("table", &rows[0], 0, &cols, 0);
        let grace = table_row_cell_path("table", &rows[1], 1, &cols, 0);
        assert_eq!(ada, "table/td/row/id/3:ada/col/id/4:done");
        assert_eq!(grace, "table/td/row/id/5:grace/col/id/4:done");
        assert_ne!(ada, grace);
        let namespaced = items(json!([{"id": "user/ada", "cells": ["x"]}]));
        assert_eq!(
            table_row_cell_path("t", &namespaced[0], 0, &[], 0),
            "t/td/row/id/8:user/ada/col/index/0"
        );
    }

    #[test]
    fn fallback_index_ids_do_not_collide_with_wire_ids() {
        let missing_row = items(json!([{"cells": ["A"]}]));
        let hashed_row = items(json!([{"id": "#0", "cells": ["B"]}]));
        let col = items(json!([{"id": "done", "label": "Done"}]));
        assert_eq!(
            table_row_cell_path("t", &missing_row[0], 0, &col, 0),
            "t/td/row/index/0/col/id/4:done"
        );
        assert_eq!(
            table_row_cell_path("t", &hashed_row[0], 0, &col, 0),
            "t/td/row/id/2:#0/col/id/4:done"
        );
        assert_ne!(
            table_row_cell_path("t", &missing_row[0], 0, &col, 0),
            table_row_cell_path("t", &hashed_row[0], 0, &col, 0)
        );

        let row = items(json!([{"id": "ada", "cells": ["x", "y"]}]));
        let missing_col: Vec<Item> = Vec::new();
        let hashed_col = items(json!([{"id": "#0", "label": "Zero"}]));
        assert_eq!(
            table_row_cell_path("t", &row[0], 0, &missing_col, 0),
            "t/td/row/id/3:ada/col/index/0"
        );
        assert_eq!(
            table_row_cell_path("t", &row[0], 0, &hashed_col, 0),
            "t/td/row/id/3:ada/col/id/2:#0"
        );
        assert_ne!(
            table_row_cell_path("t", &row[0], 0, &missing_col, 0),
            table_row_cell_path("t", &row[0], 0, &hashed_col, 0)
        );

        let (tx, _) = mpsc::channel();
        let missing_key = columns_from_items(&items(json!([{}])));
        let hashed_key = columns_from_items(&items(json!([{"id": "#0", "label": "Zero"}])));
        let rows = rows_from_items(&items(json!([{"id": "ada", "cells": ["A"]}])));
        let missing =
            RowTableDelegate::new(missing_key, rows.clone()).with_cell_host("tbl", tx.clone());
        let hashed = RowTableDelegate::new(hashed_key, rows).with_cell_host("tbl", tx);
        assert_ne!(missing.td_element_id(0, 0), hashed.td_element_id(0, 0));
        assert!(missing.td_element_id(0, 0).contains("col/index/0"));
        assert!(hashed.td_element_id(0, 0).contains("col/id/2:#0"));

        let (tx, _) = mpsc::channel();
        let col = columns_from_items(&items(json!([{"id": "done"}])));
        let missing_row = rows_from_items(&items(json!([{"cells": ["A"]}])));
        let hashed_row = rows_from_items(&items(json!([{"id": "#0", "cells": ["B"]}])));
        let missing =
            RowTableDelegate::new(col.clone(), missing_row).with_cell_host("tbl", tx.clone());
        let hashed = RowTableDelegate::new(col, hashed_row).with_cell_host("tbl", tx);
        assert_ne!(missing.td_element_id(0, 0), hashed.td_element_id(0, 0));
        assert!(missing.td_element_id(0, 0).contains("row/index/0"));
        assert!(hashed.td_element_id(0, 0).contains("row/id/2:#0"));
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
        let sorted = items(json!([
            {"id": "name", "label": "Name", "width": 80, "align": "start", "sort": "asc"},
            {"id": "lang", "label": "Lang"}
        ]));
        assert_eq!(
            column_identity_fingerprint(&base),
            column_identity_fingerprint(&sorted)
        );
        assert_ne!(
            column_definition_fingerprint(&base),
            column_definition_fingerprint(&sorted)
        );
    }

    #[test]
    fn column_sort_fixed_and_row_reorder() {
        let columns = columns_from_items(&items(json!([
            {"id": "name", "label": "Name", "sort": "asc", "fixed": "left",
             "min-width": 40, "max-width": 200, "resizable": false, "movable": false},
            {"id": "n", "label": "N", "sort": "desc"}
        ])));
        assert_eq!(columns[0].sort, Some(ColumnSort::Ascending));
        assert!(matches!(columns[0].fixed, Some(ColumnFixed::Left)));
        assert!(!columns[0].resizable);
        assert!(!columns[0].movable);
        assert_eq!(columns[1].sort, Some(ColumnSort::Descending));

        let mut rows = rows_from_items(&items(json!([
            {"id": "b", "cells": ["B", "2"]},
            {"id": "a", "cells": ["A", "10"]}
        ])));
        let source: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        sort_table_rows(&mut rows, 0, ColumnSort::Ascending, &source);
        assert_eq!(rows[0].id, "a");
        sort_table_rows(&mut rows, 1, ColumnSort::Descending, &source);
        assert_eq!(rows[0].id, "a");
        sort_table_rows(&mut rows, 0, ColumnSort::Default, &source);
        assert_eq!(rows[0].id, "b");
        apply_active_column_sort(&columns, &mut rows, &source);
        assert_eq!(rows[0].id, "a");

        let default_cols = columns_from_items(&items(json!([
            {"id": "name", "label": "Name", "sort": "default"}
        ])));
        sort_table_rows(&mut rows, 0, ColumnSort::Ascending, &source);
        assert_eq!(rows[0].id, "a");
        apply_active_column_sort(&default_cols, &mut rows, &source);
        assert_eq!(rows[0].id, "b");
    }

    #[test]
    fn table_delegate_new_applies_initial_column_sort() {
        let columns = columns_from_items(&items(json!([
            {"id": "name", "label": "Name", "sort": "asc"}
        ])));
        let rows = rows_from_items(&items(json!([
            {"id": "z", "cells": ["Z"]},
            {"id": "a", "cells": ["A"]}
        ])));
        let delegate = RowTableDelegate::new(columns, rows);
        assert_eq!(delegate.id_at(0).as_deref(), Some("a"));
        assert_eq!(delegate.id_at(1).as_deref(), Some("z"));
        assert_eq!(delegate.source_ids(), &["z".to_string(), "a".to_string()]);

        let columns = columns_from_items(&items(json!([
            {"id": "name", "label": "Name", "sort": "desc"}
        ])));
        let rows = rows_from_items(&items(json!([
            {"id": "a", "cells": ["A"]},
            {"id": "z", "cells": ["Z"]}
        ])));
        let delegate = RowTableDelegate::new(columns, rows);
        assert_eq!(delegate.id_at(0).as_deref(), Some("z"));
        assert_eq!(delegate.id_at(1).as_deref(), Some("a"));
        assert_eq!(delegate.source_ids(), &["a".to_string(), "z".to_string()]);
    }

    fn sort_and_remap(
        rows: &mut Vec<Row>,
        col_ix: usize,
        sort: ColumnSort,
        source: &[String],
        selected_row: Option<usize>,
        selected_cell: Option<(usize, usize)>,
    ) -> (Option<usize>, Option<(usize, usize)>) {
        let old_ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        sort_table_rows(rows, col_ix, sort, source);
        let new_ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        remap_table_selection_after_sort(&old_ids, &new_ids, selected_row, selected_cell)
    }

    #[test]
    fn native_sort_remaps_selected_row_and_cell_by_id() {
        let mut rows = rows_from_items(&items(json!([
            {"id": "b", "cells": ["B", "x"]},
            {"id": "a", "cells": ["A", "y"]}
        ])));
        let source: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();

        // Row b at index 0 stays b after Asc (now index 1).
        let (row, cell) = sort_and_remap(
            &mut rows,
            0,
            ColumnSort::Ascending,
            &source,
            Some(0),
            Some((0, 1)),
        );
        assert_eq!(rows[0].id, "a");
        assert_eq!(rows[1].id, "b");
        assert_eq!(row, Some(1));
        assert_eq!(cell, Some((1, 1)));

        // Desc: a, b → b, a. Selected b (1) → 0; cell column stays 1.
        let (row, cell) = sort_and_remap(&mut rows, 0, ColumnSort::Descending, &source, row, cell);
        assert_eq!(rows[0].id, "b");
        assert_eq!(rows[1].id, "a");
        assert_eq!(row, Some(0));
        assert_eq!(cell, Some((0, 1)));

        // Default restores Clojure order b, a. Selection already matches.
        let (row, cell) = sort_and_remap(&mut rows, 0, ColumnSort::Default, &source, row, cell);
        assert_eq!(rows[0].id, "b");
        assert_eq!(rows[1].id, "a");
        assert_eq!(row, Some(0));
        assert_eq!(cell, Some((0, 1)));

        // Asc from source order with only a row selected (no cell).
        let (row, cell) =
            sort_and_remap(&mut rows, 0, ColumnSort::Ascending, &source, Some(0), None);
        assert_eq!(rows[0].id, "a");
        assert_eq!(row, Some(1));
        assert_eq!(cell, None);

        // Default from sorted order: selected a at 0 → 1 in Clojure order.
        let (row, cell) = sort_and_remap(&mut rows, 0, ColumnSort::Default, &source, Some(0), None);
        assert_eq!(rows[0].id, "b");
        assert_eq!(rows[1].id, "a");
        assert_eq!(row, Some(1));
        assert_eq!(cell, None);
    }

    #[test]
    fn native_sort_remap_does_not_emit_on_change() {
        let mut rows = rows_from_items(&items(json!([
            {"id": "b", "cells": ["B", "x"]},
            {"id": "a", "cells": ["A", "y"]}
        ])));
        let source: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        let old_ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        sort_table_rows(&mut rows, 0, ColumnSort::Ascending, &source);
        let new_ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        assert_eq!(new_ids[0], "a");
        assert_eq!(new_ids[1], "b");

        let row_action =
            table_sort_selection_sync(&old_ids, &new_ids, TableSelectionMode::Row, Some(0), None);
        assert_eq!(row_action, TableSelectionSync::SelectRow(1));
        assert!(table_sort_remap_suppresses_on_change(row_action));
        let mut coalesce = crate::protocol::TableClickCoalesce::default();
        assert!(!coalesce.on_select_row(1, table_sort_remap_suppresses_on_change(row_action)));
        assert!(coalesce.take_pending().is_none());

        let cell_action = table_sort_selection_sync(
            &old_ids,
            &new_ids,
            TableSelectionMode::Cell,
            Some(0),
            Some((0, 1)),
        );
        assert_eq!(cell_action, TableSelectionSync::SelectCell(1, 1));
        assert!(table_sort_remap_suppresses_on_change(cell_action));
        let mut coalesce = crate::protocol::TableClickCoalesce::default();
        assert!(!coalesce.on_select_cell(1, 1, table_sort_remap_suppresses_on_change(cell_action)));
        assert!(coalesce.take_pending().is_none());

        let keep =
            table_sort_selection_sync(&old_ids, &old_ids, TableSelectionMode::Row, Some(0), None);
        assert_eq!(keep, TableSelectionSync::Keep);
        assert!(!table_sort_remap_suppresses_on_change(keep));
    }

    /// Kit `set_selected_row` does not clear `selected_cell`. After
    /// select cell B then row A, sort must keep row A — not reactivate B.
    #[test]
    fn native_sort_remap_keeps_row_when_cell_slot_is_stale() {
        let mut rows = rows_from_items(&items(json!([
            {"id": "b", "cells": ["B", "x"]},
            {"id": "a", "cells": ["A", "y"]}
        ])));
        let source: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        let old_ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        sort_table_rows(&mut rows, 0, ColumnSort::Ascending, &source);
        let new_ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        assert_eq!(new_ids, vec!["a".to_string(), "b".to_string()]);

        // Active mode Row: selected_row 1 is a; selected_cell (0, 1) is stale b.
        let action = table_sort_selection_sync(
            &old_ids,
            &new_ids,
            TableSelectionMode::Row,
            Some(1),
            Some((0, 1)),
        );
        assert_eq!(action, TableSelectionSync::SelectRow(0));
        assert_ne!(action, TableSelectionSync::SelectCell(1, 1));
    }

    /// Kit `set_selected_cell` does not clear `selected_row`. After
    /// select row B then cell A, sort must keep the cell — not row B.
    #[test]
    fn native_sort_remap_keeps_cell_when_row_slot_is_stale() {
        let mut rows = rows_from_items(&items(json!([
            {"id": "b", "cells": ["B", "x"]},
            {"id": "a", "cells": ["A", "y"]}
        ])));
        let source: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        let old_ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        sort_table_rows(&mut rows, 0, ColumnSort::Ascending, &source);
        let new_ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        assert_eq!(new_ids, vec!["a".to_string(), "b".to_string()]);

        // Active mode Cell: selected_cell (1, 1) is a; selected_row 0 is stale b.
        let action = table_sort_selection_sync(
            &old_ids,
            &new_ids,
            TableSelectionMode::Cell,
            Some(0),
            Some((1, 1)),
        );
        assert_eq!(action, TableSelectionSync::SelectCell(0, 1));
        assert_ne!(action, TableSelectionSync::SelectRow(1));
    }

    #[test]
    fn native_sort_remap_keeps_column_and_empty_mode() {
        let old_ids = vec!["b".into(), "a".into()];
        let new_ids = vec!["a".into(), "b".into()];
        assert_eq!(
            table_sort_selection_sync(
                &old_ids,
                &new_ids,
                TableSelectionMode::Column,
                Some(1),
                Some((0, 1)),
            ),
            TableSelectionSync::Keep
        );
        assert_eq!(
            table_sort_selection_sync(
                &old_ids,
                &new_ids,
                TableSelectionMode::None,
                Some(1),
                Some((0, 1)),
            ),
            TableSelectionSync::Keep
        );
    }

    #[test]
    fn table_selection_mode_follows_kit_events() {
        assert_eq!(
            table_selection_mode_from_kit_event(&TableEvent::SelectRow(1)),
            Some(TableSelectionMode::Row)
        );
        assert_eq!(
            table_selection_mode_from_kit_event(&TableEvent::SelectCell(0, 1)),
            Some(TableSelectionMode::Cell)
        );
        assert_eq!(
            table_selection_mode_from_kit_event(&TableEvent::SelectColumn(2)),
            Some(TableSelectionMode::Column)
        );
        assert_eq!(
            table_selection_mode_from_kit_event(&TableEvent::ClearSelection),
            Some(TableSelectionMode::None)
        );
        assert_eq!(
            table_selection_mode_from_kit_event(&TableEvent::ColumnWidthsChanged(Vec::new())),
            None
        );
    }

    #[test]
    fn table_selection_sync_uses_logical_mode_not_stale_slots() {
        let row = |id: &str| match id {
            "a" => Some(1),
            "b" => Some(0),
            _ => None,
        };
        let col = |id: &str| match id {
            "lang" => Some(1),
            _ => None,
        };
        let exists = |_: &str| true;

        // Wanted row a (index 1) while Kit still holds stale cell b/col1.
        assert_eq!(
            table_selection_sync(
                &TableSelectionWanted::Row("a".into()),
                TableSelectionMode::Row,
                Some(1),
                Some((0, 1)),
                row,
                col,
                exists,
                exists,
            ),
            TableSelectionSync::Keep
        );

        // Wanted cell a/col1 while Kit still holds stale row b.
        assert_eq!(
            table_selection_sync(
                &TableSelectionWanted::Cell {
                    row: "a".into(),
                    col: "lang".into()
                },
                TableSelectionMode::Cell,
                Some(0),
                Some((1, 1)),
                row,
                col,
                exists,
                exists,
            ),
            TableSelectionSync::Keep
        );

        // Stale matching cell must not Keep a cell restore while Row is active.
        assert_eq!(
            table_selection_sync(
                &TableSelectionWanted::Cell {
                    row: "b".into(),
                    col: "lang".into()
                },
                TableSelectionMode::Row,
                Some(1),
                Some((0, 1)),
                row,
                col,
                exists,
                exists,
            ),
            TableSelectionSync::SelectCell(0, 1)
        );
    }

    #[test]
    fn load_more_latch_resets_on_collection_or_flags() {
        let mut delegate = RowListDelegate::new(rows_from_items(&items(json!([
            {"id": "alpha", "label": "Alpha"}
        ]))));
        delegate.sync_chrome(false, true, 20, None, Some("cb-more".into()), true);
        delegate.mark_load_more_sent();
        delegate.sync_chrome(false, true, 20, None, Some("cb-more".into()), false);
        assert!(delegate.load_more_sent());
        delegate.sync_chrome(false, true, 20, None, Some("cb-more".into()), true);
        assert!(!delegate.load_more_sent());
        delegate.mark_load_more_sent();
        delegate.sync_chrome(false, false, 20, None, Some("cb-more".into()), false);
        assert!(!delegate.load_more_sent());
        delegate.mark_load_more_sent();
        delegate.sync_chrome(true, true, 20, None, Some("cb-more".into()), false);
        assert!(!delegate.load_more_sent());
    }

    #[test]
    fn load_more_latch_resets_only_when_callback_appears() {
        assert!(!load_more_latch_resets(false, true, false, true, true));
        assert!(load_more_latch_resets(false, true, false, false, true));
        assert!(!load_more_latch_resets(false, true, false, true, false));
        assert!(!load_more_latch_resets(false, true, false, false, false));
        assert!(load_more_latch_resets(true, true, false, true, true));
        assert!(load_more_latch_resets(false, false, false, true, true));
        assert!(load_more_latch_resets(false, true, true, true, true));
    }

    #[test]
    fn load_more_latch_waits_for_callback() {
        let (tx, rx) = mpsc::channel();
        let mut list = RowListDelegate::new(rows_from_items(&items(json!([
            {"id": "alpha", "label": "Alpha"}
        ]))))
        .with_host(tx);
        list.sync_chrome(false, true, 20, None, None, true);
        list.fire_load_more();
        assert!(!list.load_more_sent());
        assert!(rx.try_recv().is_err());

        list.sync_chrome(false, true, 20, None, Some("cb-1".into()), false);
        assert!(!list.load_more_sent());
        list.fire_load_more();
        assert!(list.load_more_sent());
        match rx.try_recv() {
            Ok(Cmd::Callback { id, value, seq }) => {
                assert_eq!(id, "cb-1");
                assert_eq!(value, None);
                assert_eq!(seq, None);
            }
            other => panic!("expected load-more callback, got {other:?}"),
        }
        list.sync_chrome(false, true, 20, None, Some("cb-2".into()), false);
        assert!(list.load_more_sent());
        list.fire_load_more();
        assert!(rx.try_recv().is_err());
        list.sync_chrome(false, true, 20, None, None, false);
        assert!(list.load_more_sent());
        list.sync_chrome(false, true, 20, None, Some("cb-1".into()), false);
        assert!(!list.load_more_sent());

        let (tx, rx) = mpsc::channel();
        let cols = columns_from_items(&items(json!([{"id": "name", "label": "Name"}])));
        let rows = rows_from_items(&items(json!([{"id": "alpha", "cells": ["Alpha"]}])));
        let mut table = RowTableDelegate::new(cols, rows).with_cell_host("tbl", tx);
        table.sync_chrome(false, true, 20, None, None, None, true);
        table.fire_load_more();
        assert!(!table.load_more_sent());
        assert!(rx.try_recv().is_err());

        table.sync_chrome(false, true, 20, None, Some("cb-1".into()), None, false);
        assert!(!table.load_more_sent());
        table.fire_load_more();
        assert!(table.load_more_sent());
        match rx.try_recv() {
            Ok(Cmd::Callback { id, .. }) => assert_eq!(id, "cb-1"),
            other => panic!("expected table load-more callback, got {other:?}"),
        }
        table.sync_chrome(false, true, 20, None, Some("cb-2".into()), None, false);
        assert!(table.load_more_sent());
        table.fire_load_more();
        assert!(rx.try_recv().is_err());
        table.sync_chrome(false, true, 20, None, None, None, false);
        assert!(table.load_more_sent());
        table.sync_chrome(false, true, 20, None, Some("cb-1".into()), None, false);
        assert!(!table.load_more_sent());
        table.fire_load_more();
        match rx.try_recv() {
            Ok(Cmd::Callback { id, .. }) => assert_eq!(id, "cb-1"),
            other => panic!("expected table load-more after callback appeared, got {other:?}"),
        }
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
                TableSelectionMode::Row,
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
                TableSelectionMode::Cell,
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
