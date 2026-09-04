use crate::action_bridge;
use crate::catalog;
use crate::chat;
use crate::extra;
use crate::mapping;
use crate::overlay;
use crate::preview;
use crate::protocol::{self, Cmd, HostEvent, Item, Node};
use crate::rows::{self, RowListDelegate, RowTableDelegate, SelectionSync};
use gpui::{
    AnyElement, App, Axis, Bounds, ClickEvent, Context, DismissEvent, Element, ElementId, Entity,
    EntityId, Focusable, GlobalElementId, InspectorElementId, Keystroke, LayoutId,
    PathPromptOptions, Pixels, SharedString, Styled, Subscription, Window, canvas, div, prelude::*,
    px, size,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, FocusableExt as _, Icon, IconName, IndexPath, Root,
    Sizable as _, WindowExt as _,
    accordion::Accordion,
    alert::Alert,
    badge::Badge,
    breadcrumb::{Breadcrumb, BreadcrumbItem},
    button::{Button, DropdownButton, Toggle, ToggleVariants as _},
    checkbox::Checkbox,
    clipboard::Clipboard,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    combobox::{Combobox, ComboboxEvent, ComboboxState},
    command::{Command as KitCommand, CommandState},
    date_picker::{DatePicker, DatePickerEvent, DatePickerState},
    description_list::DescriptionList,
    dock::{DockArea, DockLayout, DockPlacement, DockSkin, panel_handle},
    group_box::{GroupBox, GroupBoxVariants as _},
    h_flex,
    hover_card::HoverCard,
    input::{
        Editor, EditorState, Input, InputEvent, InputState, NumberInput, NumberInputEvent,
        OtpEvent, OtpInput, OtpState, StepAction, Textarea, TextareaState,
    },
    kbd::Kbd,
    link::Link,
    list::{List, ListEvent, ListState},
    menu::{ContextMenuExt as _, DropdownMenu as _},
    message_scroller::{MessageScroller, MessageScrollerState},
    notification::Notification,
    pagination::Pagination,
    popover::Popover,
    progress::{Progress, ProgressCircle},
    radio::{Radio, RadioGroup},
    rating::Rating,
    resizable::{ResizableState, h_resizable, resizable_panel, v_resizable},
    scroll::ScrollableElement as _,
    searchable_list::{SearchableListDelegate, SearchableListItem},
    select::{SearchableVec, Select, SelectEvent, SelectGroup, SelectItem, SelectState},
    separator::Separator,
    shimmer::ShimmerText,
    sidebar::{Sidebar, SidebarMenu, SidebarMenuItem},
    skeleton::Skeleton,
    slider::{Slider, SliderEvent, SliderScale, SliderState, SliderValue},
    spinner::Spinner,
    status_bar::StatusBar,
    stepper::{Stepper, StepperItem},
    switch::Switch,
    tab::{Tab, TabBar},
    table::{
        DataTable, Table, TableBody, TableCaption, TableCell, TableEvent, TableFooter, TableHead,
        TableHeader, TableRow, TableState,
    },
    tag::Tag,
    theme::{Theme, ThemeConfig, ThemeMode},
    tooltip::Tooltip,
    tree::{TreeItem, TreeState, tree},
    v_flex,
};
use gpui_kit as gpui;
use gpui_kit::component as gpui_component;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

struct InputSlot {
    state: Entity<InputState>,
    on_change: Option<String>,
    on_submit: Option<String>,
    on_blur: Option<String>,
    on_escape: Option<String>,
    /// When set, ignore `Change` and wait for the tree that follows this submit.
    wait_for_seq: Option<u64>,
    /// Submitted string; a late `Change` echoing it must not restore the draft.
    submitted: Option<String>,
    /// NumberInput: emit JSON numbers and honor step/min/max.
    as_number: bool,
    number_min: Option<f32>,
    number_max: Option<f32>,
    number_step: Option<f32>,
    number_stepped: bool,
    /// Undo/redo groups and fast typing emit several Change events;
    /// flush once per callback-id generation. See
    /// `protocol::InputChangeCoalesce`.
    change: protocol::InputChangeCoalesce,
}

struct TextControlSlot<S> {
    state: Entity<S>,
    on_change: Option<String>,
    on_submit: Option<String>,
    on_blur: Option<String>,
    on_escape: Option<String>,
    wait_for_seq: Option<u64>,
    submitted: Option<String>,
    change: protocol::InputChangeCoalesce,
}

enum TextFlush {
    Input,
    Textarea,
    Editor,
}

struct SliderSlot {
    state: Entity<SliderState>,
    min: f32,
    max: f32,
    step: f32,
    scale: SliderScale,
    on_change: Option<String>,
    on_release: Option<String>,
    coalesce: protocol::SliderEventCoalesce,
    /// Last wrapper size. Crate fill/thumb use cached bar bounds; if the
    /// track width changes we must re-render or they disagree by a few px.
    bar_px: Option<(f32, f32)>,
    /// Extra RootView frames after the size looks stable, so fill/thumb
    /// rebuild against the crate canvas bounds from the previous prepaint.
    settle: u8,
}

#[derive(Clone)]
struct SelectOpt {
    id: SharedString,
    label: SharedString,
    disabled: bool,
    display: Option<SharedString>,
}

impl From<extra::SelectLeaf> for SelectOpt {
    fn from(leaf: extra::SelectLeaf) -> Self {
        Self {
            id: SharedString::from(leaf.id),
            label: SharedString::from(leaf.label),
            disabled: leaf.disabled,
            display: leaf.display.map(SharedString::from),
        }
    }
}

impl SelectItem for SelectOpt {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn display_title(&self) -> Option<AnyElement> {
        self.display.as_ref().map(|s| s.clone().into_any_element())
    }

    fn value(&self) -> &Self::Value {
        &self.id
    }

    fn disabled(&self) -> bool {
        self.disabled
    }
}

enum SelectStateHandle {
    Flat(Entity<SelectState<SearchableVec<SelectOpt>>>),
    Grouped(Entity<SelectState<SearchableVec<SelectGroup<SelectOpt>>>>),
}

struct SelectSlot {
    state: SelectStateHandle,
    searchable: bool,
    fingerprint: u64,
    selected: Option<SharedString>,
    on_change: Option<String>,
}

#[derive(Clone)]
struct ComboOpt {
    id: SharedString,
    label: SharedString,
    disabled: bool,
}

impl From<extra::SelectLeaf> for ComboOpt {
    fn from(leaf: extra::SelectLeaf) -> Self {
        Self {
            id: SharedString::from(leaf.id),
            label: SharedString::from(leaf.label),
            disabled: leaf.disabled,
        }
    }
}

impl SearchableListItem for ComboOpt {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.id
    }

    fn disabled(&self) -> bool {
        self.disabled
    }
}

#[derive(Clone)]
enum ComboboxStateHandle {
    Flat(Entity<ComboboxState<SearchableVec<ComboOpt>>>),
    Grouped(Entity<ComboboxState<SearchableVec<SelectGroup<ComboOpt>>>>),
}

struct ComboboxSlot {
    state: ComboboxStateHandle,
    searchable: bool,
    multiple: bool,
    fingerprint: u64,
    /// Last native or controlled selection. Updated from
    /// `ComboboxEvent::Change` / `Confirm` so a Clojure echo of those
    /// ids does not call `set_selected_values` (which clears the query).
    selected: Vec<SharedString>,
    on_change: Option<String>,
    on_confirm: Option<String>,
    coalesce: protocol::ComboboxActivationCoalesce,
}

struct CommandSlot {
    state: Entity<CommandState>,
    suppress_select: bool,
    pending_select: Option<Vec<String>>,
    /// Last query actually sent as `:on-query`, held until that callback's seq.
    /// Bound in `flush_callback_queue`, including a delayed flush.
    query_latch: Option<action_bridge::CommandEchoLatch<String>>,
    /// Last path actually sent as `:on-select`, held until that callback's seq.
    /// Bound in `flush_callback_queue`, including a delayed flush.
    selected_latch: Option<action_bridge::CommandEchoLatch<Vec<String>>>,
    programmatic_query: Option<String>,
}

struct MessageScrollerSlot {
    state: Entity<MessageScrollerState>,
    items: Rc<RefCell<Vec<Node>>>,
    last_ids: Vec<String>,
    last_fps: Vec<u64>,
    /// Last successfully applied `scroll_to_item` / `scroll_to_end`
    /// request. Unresolved items are not stored, so the same request can
    /// succeed after append/load. Omitted Clojure request leaves native
    /// scroll (jump button, user drag).
    last_scroll: Option<chat::ScrollerScrollToken>,
    _observe: Subscription,
}

struct ListSlot {
    state: Entity<ListState<RowListDelegate>>,
    fingerprint: u64,
    searchable: bool,
    on_change: Option<String>,
    on_confirm: Option<String>,
}

struct TableSlot {
    state: Entity<TableState<RowTableDelegate>>,
    fingerprint: u64,
    on_change: Option<String>,
    on_confirm: Option<String>,
    suppress_select: bool,
    /// SelectRow from this effect cycle, flushed by Defer unless
    /// DoubleClickedRow consumes it for a same-click activation batch.
    coalesce: protocol::TableClickCoalesce,
}

struct TreeSlot {
    state: Entity<TreeState>,
    items: Vec<TreeItem>,
    fingerprint: u64,
    on_change: Option<String>,
}

struct OtpSlot {
    state: Entity<OtpState>,
    length: usize,
    on_change: Option<String>,
    on_blur: Option<String>,
}

struct ColorSlot {
    state: Entity<ColorPickerState>,
    on_change: Option<String>,
}

struct DateSlot {
    state: Entity<DatePickerState>,
    range: bool,
    on_change: Option<String>,
}

struct DockSlot {
    area: Entity<DockArea>,
    fingerprint: String,
    panels: HashMap<String, Entity<extra::CljPanel>>,
}

struct NavStackSlot {
    state: Entity<gpui::base::NavStackState>,
    /// One `CljNavPage` entity per history entry, parallel to Kit's stack.
    /// Repeated catalog ids are distinct entities (Kit Presence identity is
    /// `("nav-stack", view.entity_id())`).
    entries: Vec<(String, Entity<extra::CljNavPage>)>,
    /// Parallel to Kit `History::forward_entries`: last is nearest (the
    /// entity `forward()` restores). Catalog id plus the retained
    /// `CljNavPage` so `forward_views()` can be expressed back to Clojure
    /// and restore cannot spawn a fresh Push.
    forward: Vec<(String, Entity<extra::CljNavPage>)>,
    /// Last invalid controlled trail we warned about, so a sticky typo
    /// does not reprint `[host] nav-stack: ignoring …` every frame.
    last_invalid: Option<Vec<String>>,
    /// Last duplicate catalog-id set we warned about.
    last_dup_catalog: Option<Vec<String>>,
    /// Last nearest-first forward id list sent on `:on-forward-change`.
    /// `None` means never notified (empty after first mount is skipped).
    last_forward_notified: Option<Vec<String>>,
    /// Last applied `:replace-generation` bound to the `CljNavPage`
    /// entity that was current when we observed it — not the catalog id.
    /// Repeated ids are distinct entities; a later bump replaces only
    /// that instance.
    last_replace: Option<(EntityId, String)>,
    /// Last ignored `:item` warning, so an unknown name or dropped fn
    /// does not reprint every frame.
    last_item_warn: Option<String>,
    _observe: Subscription,
}

struct NotificationSlot {
    entity: Entity<Notification>,
    fingerprint: String,
    on_click: Option<String>,
    on_close: Option<String>,
    suppress_close: bool,
}

struct CljNotification;

#[derive(Debug, PartialEq, Eq)]
enum ZenityPick {
    Picked(String),
    Cancelled,
    Unavailable,
    Failed(String),
}

fn zenity_from_output(
    status: std::process::ExitStatus,
    stdout: &[u8],
    stderr: &[u8],
) -> ZenityPick {
    if status.success() {
        let path = String::from_utf8_lossy(stdout).trim().to_string();
        if path.is_empty() {
            ZenityPick::Cancelled
        } else {
            ZenityPick::Picked(path)
        }
    } else if status.code() == Some(1) {
        // zenity: 1 means the user clicked Cancel.
        ZenityPick::Cancelled
    } else {
        let err = String::from_utf8_lossy(stderr).trim().to_string();
        let detail = if err.is_empty() {
            format!("zenity exited {status}")
        } else {
            err
        };
        ZenityPick::Failed(detail)
    }
}

fn zenity_pick_directory(title: &str) -> ZenityPick {
    match Command::new("zenity")
        .args(["--file-selection", "--directory", "--title", title])
        .output()
    {
        Ok(output) => zenity_from_output(output.status, &output.stdout, &output.stderr),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => ZenityPick::Unavailable,
        Err(err) => ZenityPick::Failed(err.to_string()),
    }
}

fn zenity_to_cmd(request_id: String, pick: ZenityPick, portal_err: &str) -> Cmd {
    match pick {
        ZenityPick::Picked(path) => Cmd::DirectoryPicked {
            request_id,
            path: Some(path),
            error: None,
            cancelled: false,
        },
        ZenityPick::Cancelled => Cmd::DirectoryPicked {
            request_id,
            path: None,
            error: None,
            cancelled: true,
        },
        ZenityPick::Unavailable => Cmd::DirectoryPicked {
            request_id,
            path: None,
            error: Some(format!(
                "native folder picker failed ({portal_err}). Install xdg-desktop-portal or zenity."
            )),
            cancelled: false,
        },
        ZenityPick::Failed(detail) => Cmd::DirectoryPicked {
            request_id,
            path: None,
            error: Some(format!("zenity failed: {detail}")),
            cancelled: false,
        },
    }
}

pub struct RootView {
    tree: Option<Node>,
    status: String,
    error: Option<String>,
    nrepl_port: u16,
    cmd_tx: mpsc::Sender<Cmd>,
    inputs: HashMap<String, InputSlot>,
    /// Kept for the window lifetime. Crate `SliderState.bounds` is private;
    /// dropping the entity on unmount remounts at size 0 (100% fill).
    sliders: HashMap<String, SliderSlot>,
    selects: HashMap<String, SelectSlot>,
    comboboxes: HashMap<String, ComboboxSlot>,
    commands: HashMap<String, CommandSlot>,
    lists: HashMap<String, ListSlot>,
    tables: HashMap<String, TableSlot>,
    trees: HashMap<String, TreeSlot>,
    otps: HashMap<String, OtpSlot>,
    colors: HashMap<String, ColorSlot>,
    dates: HashMap<String, DateSlot>,
    editors: HashMap<String, TextControlSlot<EditorState>>,
    textareas: HashMap<String, TextControlSlot<TextareaState>>,
    vlists: HashMap<String, Entity<extra::VirtualListView>>,
    scrollers: HashMap<String, MessageScrollerSlot>,
    docks: HashMap<String, DockSlot>,
    nav_stacks: HashMap<String, NavStackSlot>,
    resizables: HashMap<String, Entity<ResizableState>>,
    used_resizables: HashSet<String>,
    dialogs: Vec<overlay::DialogSpec>,
    dialog_live: Rc<RefCell<Vec<overlay::DialogSpec>>>,
    dialog_keys: Vec<String>,
    dialog_pending: bool,
    callback_queue: overlay::CallbackQueue,
    sheet: Option<overlay::SheetSpec>,
    sheet_live: Rc<RefCell<Option<overlay::SheetSpec>>>,
    sheet_key: Option<String>,
    sheet_pending: bool,
    notes: HashMap<String, NotificationSlot>,
    note_waiting: HashSet<String>,
    used_inputs: HashSet<String>,
    used_selects: HashSet<String>,
    used_comboboxes: HashSet<String>,
    used_commands: HashSet<String>,
    used_lists: HashSet<String>,
    used_tables: HashSet<String>,
    used_trees: HashSet<String>,
    used_otps: HashSet<String>,
    used_colors: HashSet<String>,
    used_dates: HashSet<String>,
    used_editors: HashSet<String>,
    used_textareas: HashSet<String>,
    used_vlists: HashSet<String>,
    used_scrollers: HashSet<String>,
    used_docks: HashSet<String>,
    used_nav_stacks: HashSet<String>,
    native_menu_open: HashMap<String, bool>,
    _appearance: Subscription,
    _window_bounds: Subscription,
    _keystrokes: Subscription,
    next_submit_seq: u64,
    tree_seq: Option<u64>,
    applied_title: String,
    applied_window_size: Option<(i32, i32)>,
    native_window_id: Option<u32>,
}

struct RenderPaint<'v, 'w, 'cx, 'a> {
    view: &'v mut RootView,
    window: &'w mut Window,
    cx: &'cx mut Context<'a, RootView>,
}

impl chat::NodePainter for RenderPaint<'_, '_, '_, '_> {
    fn paint_node(&mut self, node: &Node, path: &str) -> AnyElement {
        self.view.render_node(node, path, self.window, self.cx)
    }

    fn cmd_tx(&self) -> Option<mpsc::Sender<Cmd>> {
        Some(self.view.cmd_tx.clone())
    }
}

impl RootView {
    pub fn new(
        nrepl_port: u16,
        cmd_tx: mpsc::Sender<Cmd>,
        event_rx: async_channel::Receiver<HostEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            this.apply_theme(window, cx);
            // Always notify: nested nodes may use `:theme :system` even when
            // the root is pinned to light or dark.
            cx.notify();
        });
        let window_bounds = cx.observe_window_bounds(window, |this, _, cx| {
            for slot in this.sliders.values_mut() {
                slot.bar_px = None;
                slot.settle = 0;
            }
            cx.notify();
        });
        let keystrokes = cx.observe_keystrokes(|this, event, window, cx| {
            if event.keystroke.key != "escape" {
                return;
            }
            this.handle_escape(window, cx);
        });
        let _ = cmd_tx.send(Cmd::Render);
        cx.spawn(async move |this, cx| {
            while let Ok(event) = event_rx.recv().await {
                let _ = this.update(cx, |view, cx| {
                    match event {
                        HostEvent::Ready { nrepl_port, .. } => {
                            view.nrepl_port = nrepl_port;
                            view.status = format!("nREPL 127.0.0.1:{nrepl_port} · connected");
                        }
                        HostEvent::Tree(tree, seq, themes) => {
                            catalog::install_clojure_sets(themes);
                            overlay::acknowledge_dialog_tree(&mut view.dialog_keys, &tree);
                            view.tree = Some(tree);
                            view.tree_seq = seq;
                            view.callback_queue.tree_installed(seq);
                            view.flush_callback_queue();
                            view.error = None;
                            view.status = format!(
                                "nREPL 127.0.0.1:{} · live · hot reload on",
                                view.nrepl_port
                            );
                        }
                        HostEvent::Error(err) => {
                            view.callback_queue.clear();
                            for slot in view.inputs.values_mut() {
                                slot.wait_for_seq = None;
                                slot.submitted = None;
                                slot.change.clear();
                            }
                            for slot in view.textareas.values_mut() {
                                slot.wait_for_seq = None;
                                slot.submitted = None;
                                slot.change.clear();
                            }
                            for slot in view.editors.values_mut() {
                                slot.change.clear();
                            }
                            view.error = Some(err);
                        }
                        HostEvent::PickDirectory { request_id, title } => {
                            view.start_pick_directory(request_id, title, cx);
                        }
                        HostEvent::RevealPath { path } => {
                            cx.reveal_path(Path::new(&path));
                        }
                        HostEvent::OpenPath { path } => {
                            cx.open_with_system(Path::new(&path));
                        }
                        HostEvent::CapturePreview { request_id } => {
                            view.capture_preview(request_id, cx);
                        }
                    }
                    cx.notify();
                });
            }
        })
        .detach();

        Self {
            tree: None,
            status: format!("nREPL 127.0.0.1:{nrepl_port} · loading Clojure UI"),
            error: None,
            nrepl_port,
            cmd_tx,
            inputs: HashMap::new(),
            sliders: HashMap::new(),
            selects: HashMap::new(),
            comboboxes: HashMap::new(),
            commands: HashMap::new(),
            lists: HashMap::new(),
            tables: HashMap::new(),
            trees: HashMap::new(),
            otps: HashMap::new(),
            colors: HashMap::new(),
            dates: HashMap::new(),
            editors: HashMap::new(),
            textareas: HashMap::new(),
            vlists: HashMap::new(),
            scrollers: HashMap::new(),
            docks: HashMap::new(),
            nav_stacks: HashMap::new(),
            resizables: HashMap::new(),
            used_resizables: HashSet::new(),
            dialogs: Vec::new(),
            dialog_live: Rc::new(RefCell::new(Vec::new())),
            dialog_keys: Vec::new(),
            dialog_pending: false,
            callback_queue: overlay::CallbackQueue::default(),
            sheet: None,
            sheet_live: Rc::new(RefCell::new(None)),
            sheet_key: None,
            sheet_pending: false,
            notes: HashMap::new(),
            note_waiting: HashSet::new(),
            used_inputs: HashSet::new(),
            used_selects: HashSet::new(),
            used_comboboxes: HashSet::new(),
            used_commands: HashSet::new(),
            used_lists: HashSet::new(),
            used_tables: HashSet::new(),
            used_trees: HashSet::new(),
            used_otps: HashSet::new(),
            used_colors: HashSet::new(),
            used_dates: HashSet::new(),
            used_editors: HashSet::new(),
            used_textareas: HashSet::new(),
            used_vlists: HashSet::new(),
            used_scrollers: HashSet::new(),
            used_docks: HashSet::new(),
            used_nav_stacks: HashSet::new(),
            native_menu_open: HashMap::new(),
            _appearance: appearance,
            _window_bounds: window_bounds,
            _keystrokes: keystrokes,
            next_submit_seq: 0,
            tree_seq: None,
            applied_title: String::new(),
            applied_window_size: None,
            native_window_id: preview::native_window_id(window),
        }
    }

    fn requested_theme(&self) -> &str {
        self.tree
            .as_ref()
            .and_then(|node| node.theme.as_deref())
            .filter(|theme| !theme.is_empty())
            .unwrap_or("system")
    }

    fn apply_theme(&self, window: &mut Window, cx: &mut Context<Self>) {
        apply_theme_pref(self.requested_theme(), window, cx);
    }

    fn requested_chrome(&self) -> &str {
        self.tree
            .as_ref()
            .and_then(|node| node.chrome.as_deref())
            .filter(|chrome| !chrome.is_empty())
            .unwrap_or("dev")
    }

    fn show_dev_chrome(&self) -> bool {
        self.requested_chrome() != "app"
    }

    fn apply_chrome(&mut self, window: &mut Window) {
        let (title, width, height) = {
            let Some(tree) = self.tree.as_ref() else {
                return;
            };
            (
                tree.title
                    .as_deref()
                    .filter(|title| !title.is_empty())
                    .unwrap_or("clj-gpui")
                    .to_string(),
                tree.window_width.or(tree.width),
                tree.window_height.or(tree.height),
            )
        };
        if self.applied_title != title {
            window.set_window_title(&title);
            self.applied_title = title;
        }
        if let (Some(width), Some(height)) = (width, height) {
            let requested = (width.round() as i32, height.round() as i32);
            if self.applied_window_size != Some(requested) {
                window.resize(size(px(width), px(height)));
                self.applied_window_size = Some(requested);
            }
        }
    }

    fn handle_escape(&self, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self
            .inputs
            .values()
            .filter_map(|slot| {
                slot.on_escape
                    .clone()
                    .map(|id| (id, slot.state.read(cx).focus_handle(cx)))
            })
            .chain(self.editors.values().filter_map(|slot| {
                slot.on_escape
                    .clone()
                    .map(|id| (id, slot.state.read(cx).focus_handle(cx)))
            }))
            .chain(self.textareas.values().filter_map(|slot| {
                slot.on_escape
                    .clone()
                    .map(|id| (id, slot.state.read(cx).focus_handle(cx)))
            }))
            .find(|(_, handle)| handle.is_focused(window));
        if let Some((id, _)) = focused {
            let _ = self.cmd_tx.send(Cmd::Callback {
                id,
                value: None,
                seq: None,
            });
        }
    }

    fn start_pick_directory(
        &self,
        request_id: String,
        title: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let prompt = title.unwrap_or_else(|| "Choose a folder".to_string());
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(prompt.clone())),
        });
        let cmd_tx = self.cmd_tx.clone();
        cx.spawn(async move |_this, cx| {
            let cmd = match receiver.await {
                Ok(Ok(Some(paths))) => {
                    let path = paths
                        .into_iter()
                        .next()
                        .map(|p| p.to_string_lossy().into_owned());
                    Cmd::DirectoryPicked {
                        request_id,
                        cancelled: path.is_none(),
                        path,
                        error: None,
                    }
                }
                Ok(Ok(None)) => Cmd::DirectoryPicked {
                    request_id,
                    path: None,
                    error: None,
                    cancelled: true,
                },
                Ok(Err(err)) => {
                    // zenity `.output()` waits for the user. Do that on the
                    // background executor so the GPUI foreground runtime
                    // can keep painting while the dialog is open.
                    let prompt = prompt.clone();
                    let pick = cx
                        .background_executor()
                        .spawn(async move { zenity_pick_directory(&prompt) })
                        .await;
                    zenity_to_cmd(request_id, pick, &err.to_string())
                }
                Err(_) => Cmd::DirectoryPicked {
                    request_id,
                    path: None,
                    error: Some("folder picker was cancelled internally".into()),
                    cancelled: false,
                },
            };
            let _ = cmd_tx.send(cmd);
        })
        .detach();
    }

    fn preview_title(&self) -> String {
        self.tree
            .as_ref()
            .and_then(|node| node.title.clone())
            .filter(|title| !title.is_empty())
            .or_else(|| (!self.applied_title.is_empty()).then(|| self.applied_title.clone()))
            .unwrap_or_else(|| "clj-gpui".into())
    }

    fn capture_preview(&self, request_id: String, cx: &mut Context<Self>) {
        if self.tree.is_none() {
            let _ = self.cmd_tx.send(Cmd::PreviewCaptured {
                request_id,
                png: None,
            });
            return;
        }
        // GPUI 0.2.2 stops the macOS display link while occluded (zed#63217).
        // Enable that override only now, on the first capture-preview, so
        // ordinary apps keep GPUI's occlusion power-saving until Preview is
        // used. Dirty this view so the next tick presents (unfocused windows
        // otherwise skip present), then capture off the UI thread.
        preview::restart_occluded_display_link();
        cx.notify();
        let title = self.preview_title();
        let window_id = self.native_window_id;
        let cmd_tx = self.cmd_tx.clone();
        cx.background_executor()
            .spawn(async move {
                let png = preview::capture_host_window(&title, window_id);
                let _ = cmd_tx.send(Cmd::PreviewCaptured { request_id, png });
            })
            .detach();
    }

    fn click(&self, callback_id: String) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let cmd_tx = self.cmd_tx.clone();
        move |_, _, _| {
            let _ = cmd_tx.send(Cmd::Callback {
                id: callback_id.clone(),
                value: None,
                seq: None,
            });
        }
    }

    fn emit_value(&self, callback_id: String, value: Value) {
        let _ = self.cmd_tx.send(Cmd::Callback {
            id: callback_id,
            value: Some(value),
            seq: None,
        });
    }

    fn action_emitter(cx: &Context<Self>) -> overlay::ActionEmitter {
        let entity = cx.weak_entity();
        Rc::new(move |action, cx| {
            let _ = entity.update(cx, |this, _| {
                this.callback_queue.push(action);
                this.flush_callback_queue();
            });
        })
    }

    fn handle_clj_action(&mut self, action: &action_bridge::CljAction) {
        let key = action.slot.clone();
        let item_path = action.item_path.clone();
        let is_command = self.commands.contains_key(&key);
        if is_command {
            if let Some(slot) = self.commands.get_mut(&key) {
                slot.pending_select = Some(item_path.clone());
            }
        }
        self.callback_queue
            .push(overlay::QueuedAction::CljSelect { key, item_path });
        // Command confirm is Kit Action then deferred on_confirm. Leave
        // CljSelect queued so the confirm callback can share one batch.
        if !is_command {
            self.flush_callback_queue();
        }
    }

    fn flush_callback_queue(&mut self) -> Option<u64> {
        let outbound = {
            let tree = self.tree.as_ref()?;
            self.callback_queue.next_outbound(tree)?
        };
        // Share the existing sequence allocator with input-submit responses.
        // The matching Tree is the barrier, not a timer or an arbitrary paint.
        self.next_submit_seq = self.next_submit_seq.saturating_add(1);
        let seq = self.next_submit_seq;
        self.callback_queue.sent(seq);
        protocol::send_callbacks_seq(&self.cmd_tx, outbound.calls, Some(seq));
        // Command echo latches belong to the seq that actually left the
        // queue. A later HostEvent::Tree flush must bind them too, or a
        // queued CommandQuery/CommandSelect would render without a latch.
        self.install_command_echo_latch(seq, outbound.command_echo);
        Some(seq)
    }

    fn install_command_echo_latch(&mut self, seq: u64, echo: Option<overlay::CommandEcho>) {
        match echo {
            Some(overlay::CommandEcho::Select { key, item_path }) => {
                if let Some(slot) = self.commands.get_mut(&key) {
                    slot.selected_latch = Some(action_bridge::CommandEchoLatch {
                        seq,
                        value: item_path,
                    });
                }
            }
            Some(overlay::CommandEcho::Query { key, query }) => {
                if let Some(slot) = self.commands.get_mut(&key) {
                    slot.query_latch = Some(action_bridge::CommandEchoLatch { seq, value: query });
                }
            }
            None => {}
        }
    }

    fn schedule_input_change_flush(
        key: String,
        kind: TextFlush,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        cx.defer_in(window, move |this, _, _cx| {
            this.flush_input_change(&key, kind);
        });
    }

    fn schedule_slider_event_flush(key: String, window: &Window, cx: &mut Context<Self>) {
        cx.defer_in(window, move |this, _, _cx| {
            this.flush_slider_events(&key);
        });
    }

    fn flush_slider_events(&mut self, key: &str) {
        let Some(slot) = self.sliders.get_mut(key) else {
            return;
        };
        let calls = slot
            .coalesce
            .take_outbound(slot.on_change.clone(), slot.on_release.clone());
        if calls.is_empty() {
            return;
        }
        protocol::send_callbacks(&self.cmd_tx, calls);
    }

    fn flush_text_slot<S>(
        slot: &mut TextControlSlot<S>,
        as_number: bool,
    ) -> Option<(String, Value)> {
        if slot.wait_for_seq.is_some() {
            slot.change.clear();
            return None;
        }
        let value = slot.change.take_pending()?;
        if slot.submitted.as_ref() == Some(&value) {
            slot.change.clear();
            return None;
        }
        slot.submitted = None;
        let id = slot.on_change.clone()?;
        let payload = extra::input_change_payload(as_number, &value)?;
        Some((id, payload))
    }

    fn flush_input_change(&mut self, key: &str, kind: TextFlush) {
        let flushed = match kind {
            TextFlush::Input => {
                let Some(slot) = self.inputs.get_mut(key) else {
                    return;
                };
                if slot.wait_for_seq.is_some() {
                    slot.change.clear();
                    return;
                }
                let Some(value) = slot.change.take_pending() else {
                    return;
                };
                if slot.submitted.as_ref() == Some(&value) {
                    slot.change.clear();
                    return;
                }
                slot.submitted = None;
                let Some(id) = slot.on_change.clone() else {
                    slot.change.clear();
                    return;
                };
                extra::input_change_payload(slot.as_number, &value).map(|payload| (id, payload))
            }
            TextFlush::Textarea => self
                .textareas
                .get_mut(key)
                .and_then(|slot| Self::flush_text_slot(slot, false)),
            TextFlush::Editor => self
                .editors
                .get_mut(key)
                .and_then(|slot| Self::flush_text_slot(slot, false)),
        };
        if let Some((id, payload)) = flushed {
            self.emit_value(id, payload);
        }
    }

    fn input_slot(
        &mut self,
        key: &str,
        node: &Node,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        self.used_inputs.insert(key.to_string());

        if let Some(slot) = self.inputs.get_mut(key) {
            let id_changed = slot.on_change != node.on_change;
            slot.on_change = node.on_change.clone();
            slot.on_submit = node.on_submit.clone();
            slot.on_blur = node.on_blur.clone();
            slot.on_escape = node.on_escape.clone();
            // Text-field owns this slot this frame. number_slot sets
            // as_number back when the node is a number-input. Keep
            // number_stepped so a later number-input does not double-subscribe.
            slot.as_number = false;
            slot.number_min = None;
            slot.number_max = None;
            slot.number_step = None;
            let refresh = id_changed && slot.change.on_ids_refreshed();
            let state = slot.state.clone();
            let force = matches!(
                (slot.wait_for_seq, self.tree_seq),
                (Some(wait), Some(seq)) if wait == seq
            );
            let focused = state.read(cx).focus_handle(cx).is_focused(window);
            let desired = node.text.clone().unwrap_or_default();
            let current = state.read(cx).value().to_string();
            if current != desired && (force || (!focused && slot.wait_for_seq.is_none())) {
                let desired = desired.clone();
                state.update(cx, |input, cx| {
                    input.set_value(desired, window, cx);
                });
            }
            if force {
                slot.wait_for_seq = None;
            }
            if let Some(placeholder) = node.placeholder.clone() {
                state.update(cx, |input, cx| {
                    input.set_placeholder(placeholder, window, cx);
                });
            }
            if node.focus && !state.read(cx).focus_handle(cx).is_focused(window) {
                state.read(cx).focus_handle(cx).focus(window, cx);
            }
            if refresh {
                Self::schedule_input_change_flush(key.to_string(), TextFlush::Input, window, cx);
            }
            return state;
        }

        let placeholder = node.placeholder.clone().unwrap_or_default();
        let default = node.text.clone().unwrap_or_default();
        let want_focus = node.focus;
        let state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .default_value(default)
        });
        self.inputs.insert(
            key.to_string(),
            InputSlot {
                state: state.clone(),
                on_change: node.on_change.clone(),
                on_submit: node.on_submit.clone(),
                on_blur: node.on_blur.clone(),
                on_escape: node.on_escape.clone(),
                wait_for_seq: None,
                submitted: None,
                as_number: false,
                number_min: None,
                number_max: None,
                number_step: None,
                number_stepped: false,
                change: protocol::InputChangeCoalesce::default(),
            },
        );

        let key_owned = key.to_string();
        cx.subscribe_in(
            &state,
            window,
            move |this, input, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let Some(slot) = this.inputs.get_mut(&key_owned) else {
                        return;
                    };
                    if slot.wait_for_seq.is_some() {
                        return;
                    }
                    let value = input.read(cx).value().to_string();
                    if slot.submitted.as_ref() == Some(&value) {
                        return;
                    }
                    slot.submitted = None;
                    if slot.change.on_change(value) {
                        Self::schedule_input_change_flush(
                            key_owned.clone(),
                            TextFlush::Input,
                            window,
                            cx,
                        );
                    }
                }
                InputEvent::PressEnter { .. } => {
                    this.next_submit_seq = this.next_submit_seq.saturating_add(1);
                    let seq = this.next_submit_seq;
                    let (on_submit, value, state, clear, as_number) = {
                        let Some(slot) = this.inputs.get_mut(&key_owned) else {
                            return;
                        };
                        let value = input.read(cx).value().to_string();
                        slot.wait_for_seq = Some(seq);
                        slot.submitted = Some(value.clone());
                        let clear =
                            !slot.as_number && slot.on_blur.is_none() && slot.on_escape.is_none();
                        (
                            slot.on_submit.clone(),
                            value,
                            slot.state.clone(),
                            clear,
                            slot.as_number,
                        )
                    };
                    if let Some(id) = on_submit {
                        let payload = if as_number {
                            extra::number_from_input(&value)
                                .map(|n| json!(n))
                                .unwrap_or_else(|| json!(value.clone()))
                        } else {
                            json!(value.clone())
                        };
                        let _ = this.cmd_tx.send(Cmd::Callback {
                            id,
                            value: Some(payload),
                            seq: Some(seq),
                        });
                    }
                    // Compose fields (no blur/escape handlers) clear immediately so a
                    // stale render cannot put the text back before Clojure's tree arrives.
                    if clear {
                        state.update(cx, |input, cx| {
                            input.set_value("", window, cx);
                        });
                    }
                }
                InputEvent::Blur => {
                    let Some(slot) = this.inputs.get(&key_owned) else {
                        return;
                    };
                    if slot.wait_for_seq.is_some() {
                        return;
                    }
                    let Some(id) = slot.on_blur.clone() else {
                        return;
                    };
                    let as_number = slot.as_number;
                    let value = input.read(cx).value().to_string();
                    let payload = if as_number {
                        extra::number_from_input(&value)
                            .map(|n| json!(n))
                            .unwrap_or_else(|| json!(value))
                    } else {
                        json!(value)
                    };
                    let _ = this.cmd_tx.send(Cmd::Callback {
                        id,
                        value: Some(payload),
                        seq: None,
                    });
                }
                _ => {}
            },
        )
        .detach();

        if want_focus {
            state.read(cx).focus_handle(cx).focus(window, cx);
        }

        state
    }

    fn slider_slot(
        &mut self,
        key: &str,
        node: &Node,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SliderState> {
        // gpui-component paints fill/thumb from cached `bounds`. Size 0 is a
        // 100% bar; a stale width leaves the fill a few px off the knob until
        // the mouse moves. Keep the entity across tab switches; a canvas on
        // the wrapper re-renders when the laid-out size changes.
        let (lo, hi) = slider_range(node.min, node.max);
        let step = slider_step(node.step);
        let scale = slider_effective_scale(node, lo, hi);
        let value = slider_wanted_value(node, lo, hi);

        if let Some(slot) = self.sliders.get_mut(key) {
            if (slot.min - lo).abs() <= f32::EPSILON
                && (slot.max - hi).abs() <= f32::EPSILON
                && (slot.step - step).abs() <= f32::EPSILON
                && slot.scale == scale
            {
                let id_changed =
                    slot.on_change != node.on_change || slot.on_release != node.on_release;
                slot.on_change = node.on_change.clone();
                slot.on_release = node.on_release.clone();
                let refresh = id_changed && slot.coalesce.on_ids_refreshed();
                let current = slot.state.read(cx).value();
                // `set_value` notifies without emitting Change or Release, so
                // applying Clojure's current value cannot loop. Step is drag
                // granularity only; a 40→42 update with step 5 must still land
                // on 42.
                if slider_value_changed(current, value) {
                    slot.state.update(cx, |s, cx| {
                        s.set_value(value, window, cx);
                    });
                }
                if refresh {
                    Self::schedule_slider_event_flush(key.to_string(), window, cx);
                }
                return slot.state.clone();
            }
        }

        let mut builder = SliderState::new().min(lo).max(hi).step(step);
        if scale.is_logarithmic() {
            builder = builder.scale(SliderScale::Logarithmic);
        }
        let state = cx.new(|_cx| builder.default_value(value));
        if slider_log_scale_fallback(node, lo, hi) {
            eprintln!(
                "[host] slider {}: logarithmic scale requires min > 0 and min < max; using linear",
                node.id.as_deref().unwrap_or("?")
            );
        }
        let key_owned = key.to_string();
        self.sliders.insert(
            key.to_string(),
            SliderSlot {
                state: state.clone(),
                min: lo,
                max: hi,
                step,
                scale,
                on_change: node.on_change.clone(),
                on_release: node.on_release.clone(),
                coalesce: protocol::SliderEventCoalesce::default(),
                bar_px: None,
                settle: 0,
            },
        );
        cx.subscribe_in(
            &state,
            window,
            move |this, _, event: &SliderEvent, window, cx| {
                let schedule = {
                    let Some(slot) = this.sliders.get_mut(&key_owned) else {
                        return;
                    };
                    match event {
                        SliderEvent::Change(changed) => {
                            slot.coalesce.on_change(slider_event_payload(*changed))
                        }
                        SliderEvent::Release(changed) => {
                            slot.coalesce.on_release(slider_event_payload(*changed))
                        }
                    }
                };
                if schedule {
                    Self::schedule_slider_event_flush(key_owned.clone(), window, cx);
                }
            },
        )
        .detach();
        state
    }

    fn select_slot(&mut self, key: &str, node: &Node, window: &mut Window, cx: &mut Context<Self>) {
        self.used_selects.insert(key.to_string());
        let grouped = extra::select_is_grouped(node.collection());
        let fingerprint = extra::select_fingerprint(node.collection());
        let selected = node.string_value().map(SharedString::from);

        if let Some(slot) = self.selects.get_mut(key) {
            let same_kind = match &slot.state {
                SelectStateHandle::Grouped(_) => grouped,
                SelectStateHandle::Flat(_) => !grouped,
            };
            if same_kind && slot.searchable == node.searchable {
                slot.on_change = node.on_change.clone();
                match extra::select_live_sync(
                    slot.fingerprint,
                    fingerprint,
                    slot.selected.as_deref(),
                    selected.as_deref(),
                ) {
                    extra::SelectLiveSync::Leave => return,
                    extra::SelectLiveSync::SetValue => {
                        slot.selected = selected.clone();
                        match &slot.state {
                            SelectStateHandle::Flat(state) => {
                                let state = state.clone();
                                state.update(cx, |state, cx| {
                                    apply_select_controlled_value(
                                        state,
                                        selected.as_ref(),
                                        window,
                                        cx,
                                    );
                                });
                            }
                            SelectStateHandle::Grouped(state) => {
                                let state = state.clone();
                                state.update(cx, |state, cx| {
                                    apply_select_controlled_value(
                                        state,
                                        selected.as_ref(),
                                        window,
                                        cx,
                                    );
                                });
                            }
                        }
                        return;
                    }
                    extra::SelectLiveSync::Rebuild => {
                        // Fall through and recreate so query text cannot
                        // stay attached to a fresh unfiltered SearchableVec.
                    }
                }
            }
        }

        let selected_index = extra::select_index(node.collection(), node.string_value().as_deref());
        let searchable = node.searchable;
        let key_owned = key.to_string();
        let handle = if grouped {
            let state = cx.new(|cx| {
                SelectState::new(select_group_vec(node), selected_index, window, cx)
                    .searchable(searchable)
            });
            cx.subscribe(
                &state,
                move |this, _, event: &SelectEvent<SearchableVec<SelectGroup<SelectOpt>>>, _cx| {
                    let SelectEvent::Confirm(value) = event;
                    emit_select_confirm(this, &key_owned, value.as_ref());
                },
            )
            .detach();
            SelectStateHandle::Grouped(state)
        } else {
            let state = cx.new(|cx| {
                SelectState::new(select_flat_vec(node), selected_index, window, cx)
                    .searchable(searchable)
            });
            cx.subscribe(
                &state,
                move |this, _, event: &SelectEvent<SearchableVec<SelectOpt>>, _cx| {
                    let SelectEvent::Confirm(value) = event;
                    emit_select_confirm(this, &key_owned, value.as_ref());
                },
            )
            .detach();
            SelectStateHandle::Flat(state)
        };
        self.selects.insert(
            key.to_string(),
            SelectSlot {
                state: handle,
                searchable,
                fingerprint,
                selected,
                on_change: node.on_change.clone(),
            },
        );
    }

    fn render_select(
        &mut self,
        node: &Node,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.select_slot(key, node, window, cx);
        let Some(slot) = self.selects.get(key) else {
            return div().into_any_element();
        };
        match &slot.state {
            SelectStateHandle::Flat(state) => finish_select(Select::new(state), node, cx),
            SelectStateHandle::Grouped(state) => finish_select(Select::new(state), node, cx),
        }
    }

    fn render_node(
        &mut self,
        node: &Node,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = widget_key(node, path);
        let scope = resolve_theme(node, window, cx);
        let prev = scope.as_ref().map(|_| Theme::global(cx).clone());
        if let Some(applied) = scope.as_ref() {
            activate_theme(applied, cx);
        }

        let element = match node.kind.as_str() {
            "window" => {
                let mut layout = node.clone();
                layout.width = None;
                layout.height = None;
                apply_style(v_flex().id(eid(&key)).size_full(), &layout, cx)
                    .children(self.render_children(node, path, window, cx))
                    .into_any_element()
            }
            "label" => {
                let mut el = apply_style(div().id(eid(&key)), node, cx)
                    .child(node.text.clone().unwrap_or_default());
                let on_click = node.on_click.clone();
                let on_double = node.on_double_click.clone();
                if on_click.is_some() || on_double.is_some() {
                    let cmd_tx = self.cmd_tx.clone();
                    el = el.cursor_pointer().on_click(move |event, _, _| {
                        if event.click_count() >= 2 {
                            if let Some(id) = on_double.clone() {
                                let _ = cmd_tx.send(Cmd::Callback {
                                    id,
                                    value: None,
                                    seq: None,
                                });
                                return;
                            }
                        }
                        if let Some(id) = on_click.clone() {
                            let _ = cmd_tx.send(Cmd::Callback {
                                id,
                                value: None,
                                seq: None,
                            });
                        }
                    });
                }
                el.into_any_element()
            }
            "button" => {
                let label = node.text.clone().unwrap_or_default();
                let mut button =
                    overlay::apply_button_chrome(Button::new(eid(&key)).label(label), node);
                if node.on_click.is_some() {
                    let emit = Self::action_emitter(cx);
                    let key = key.clone();
                    button = button.on_click(move |_, _, cx| {
                        emit(overlay::QueuedAction::ButtonClick { key: key.clone() }, cx);
                    });
                }
                apply_style(button, node, cx).into_any_element()
            }
            "vstack" => {
                let mut el = apply_style(v_flex().id(eid(&key)), node, cx)
                    .children(self.render_children(node, path, window, cx));
                if let Some(callback_id) = node.on_click.clone() {
                    el = el.cursor_pointer().on_click(self.click(callback_id));
                }
                el.into_any_element()
            }
            "hstack" => {
                let mut el = apply_style(h_flex().id(eid(&key)), node, cx)
                    .children(self.render_children(node, path, window, cx));
                if let Some(callback_id) = node.on_click.clone() {
                    el = el.cursor_pointer().on_click(self.click(callback_id));
                }
                el.into_any_element()
            }
            "spacer" => {
                let el = apply_style(div().id(eid(&key)), node, cx);
                if node.size.is_some() || node.flex.is_some() {
                    el.into_any_element()
                } else {
                    el.flex_1().into_any_element()
                }
            }
            "checkbox" => {
                if node.shape.as_deref() == Some("circle") {
                    self.render_circle_checkbox(node, &key, cx)
                } else {
                    let checked = node.checked.unwrap_or(false);
                    let mut checkbox = Checkbox::new(eid(&key)).checked(checked);
                    if let Some(text) = node.text.clone() {
                        checkbox = checkbox.label(text);
                    }
                    if let Some(callback_id) = node.on_click.clone() {
                        let cmd_tx = self.cmd_tx.clone();
                        checkbox = checkbox.on_click(move |_, _, _| {
                            let _ = cmd_tx.send(Cmd::Callback {
                                id: callback_id.clone(),
                                value: None,
                                seq: None,
                            });
                        });
                    }
                    apply_style(checkbox, node, cx).into_any_element()
                }
            }
            "scroll" => self.render_scroll(node, path, &key, window, cx),
            "input" => {
                let state = self.input_slot(&key, node, window, cx);
                apply_style(Input::new(&state), node, cx).into_any_element()
            }
            "textarea" => {
                let state = self.textarea_slot(&key, node, window, cx);
                apply_style(Textarea::new(&state), node, cx).into_any_element()
            }
            "switch" => self.render_switch(node, &key, cx),
            "toggle" => self.render_toggle(node, &key, cx),
            "radio-group" => self.render_radio_group(node, &key, cx),
            "slider" => self.render_slider(node, &key, window, cx),
            "rating" => self.render_rating(node, &key, cx),
            "stepper" => self.render_stepper(node, &key, cx),
            "pagination" => self.render_pagination(node, &key, cx),
            "progress" => self.render_progress(node, cx),
            "progress-circle" => self.render_progress_circle(node, path, &key, window, cx),
            "separator" => self.render_separator(node, cx),
            "spinner" => self.render_spinner(node, cx),
            "tag" => self.render_tag(node, cx),
            "alert" => self.render_alert(node, &key, cx),
            "skeleton" => self.render_skeleton(node, cx),
            "shimmer" => self.render_shimmer(node, cx),
            "kbd" => self.render_kbd(node, cx),
            "link" => self.render_link(node, &key, cx),
            "group-box" => self.render_group_box(node, path, &key, window, cx),
            "badge" => self.render_badge(node, path, window, cx),
            "tabs" => self.render_tabs(node, &key, cx),
            "select" => self.render_select(node, &key, window, cx),
            "combobox" => self.render_combobox(node, &key, window, cx),
            "icon" => self.render_icon(node, cx),
            "clipboard" => self.render_clipboard(node, &key, cx),
            "breadcrumb" => self.render_breadcrumb(node, &key, cx),
            "avatar" => self.render_avatar(node, cx),
            "avatar-group" => self.render_avatar_group(node, cx),
            "accordion" => self.render_accordion(node, path, &key, window, cx),
            "description-list" => self.render_description_list(node, cx),
            "dialog" | "alert-dialog" => div().into_any_element(),
            "popover" => self.render_popover(node, &key, cx),
            "hover-card" => self.render_hover_card(node, path, &key, window, cx),
            "dropdown-menu" => self.render_dropdown_menu(node, &key, window, cx),
            "dropdown-button" => self.render_dropdown_button(node, path, &key, window, cx),
            "context-menu" => self.render_context_menu(node, path, &key, window, cx),
            "native-menu" => div().into_any_element(),
            "command" => self.render_command(node, &key, window, cx),
            "status-bar" => self.render_status_bar(node, path, &key, window, cx),
            "list" => self.render_list(node, &key, window, cx),
            "data-table" => self.render_data_table(node, &key, window, cx),
            "table" => self.render_table(node, path, window, cx),
            "table-header" => self
                .paint_table_header(node, path, window, cx)
                .into_any_element(),
            "table-body" => self
                .paint_table_body(node, path, window, cx)
                .into_any_element(),
            "table-footer" => self
                .paint_table_footer(node, path, window, cx)
                .into_any_element(),
            "table-row" => self
                .paint_table_row_node(node, path, window, cx)
                .into_any_element(),
            "table-head" => self
                .paint_table_head(node, path, window, cx)
                .into_any_element(),
            "table-cell" => self
                .paint_table_cell(node, path, window, cx)
                .into_any_element(),
            "table-caption" => self
                .paint_table_caption(node, path, window, cx)
                .into_any_element(),
            "tree" => self.render_tree(node, &key, window, cx),
            "sheet" => div().into_any_element(),
            "notification" => div().into_any_element(),
            "number-input" => self.render_number_input(node, &key, window, cx),
            "otp-input" => self.render_otp_input(node, &key, window, cx),
            "color-picker" => self.render_color_picker(node, &key, window, cx),
            "date-picker" => self.render_date_picker(node, &key, window, cx),
            "editor" => self.render_editor(node, &key, window, cx),
            "virtual-list" => self.render_virtual_list(node, &key, window, cx),
            "chart" => {
                let default_h = extra::chart_viewport(node).1;
                viewport_sized(extra::paint_chart(node, &key, cx), node, default_h, cx)
            }
            "markdown" | "html" => apply_style(v_flex().id(eid(&key)), node, cx)
                .child(extra::paint_markdown(node, &key))
                .into_any_element(),
            "sidebar" => self.render_sidebar(node, &key, cx),
            "settings" => viewport_sized(
                extra::build_settings(node, &key, &self.cmd_tx),
                node,
                360.0,
                cx,
            ),
            "dock" => self.render_dock(node, &key, window, cx),
            "nav-stack" => self.render_nav_stack(node, &key, window, cx),
            "nav-page" => overlay::paint_scroller_tree(node, path, &self.cmd_tx),
            "resizable" => self.render_resizable(node, path, &key, window, cx),
            "message-scroller" => self.render_message_scroller(node, path, &key, window, cx),
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
            | "attachment-media-overlay"
            | "attachment-content"
            | "attachment-title"
            | "attachment-description"
            | "attachment-actions"
            | "attachment-group"
            | "marker"
            | "marker-icon"
            | "marker-content" => chat::render_any(
                &mut RenderPaint {
                    view: self,
                    window,
                    cx,
                },
                node,
                path,
            ),
            other => div()
                .id(eid(&key))
                .text_color(cx.theme().danger)
                .child(format!("Unknown GPUI node: {other}"))
                .into_any_element(),
        };

        let element = with_tooltip(element, node, &key);

        if let Some(prev) = prev {
            *Theme::global_mut(cx) = prev;
        }
        match scope {
            Some(applied) => ThemeScope::new(applied, element).into_any_element(),
            None => element,
        }
    }

    fn render_switch(&self, node: &Node, key: &str, cx: &App) -> AnyElement {
        let checked = node.checked.unwrap_or(false);
        let mut el = Switch::new(eid(key)).checked(checked);
        if let Some(text) = node.text.clone() {
            el = el.label(text);
        }
        if node.disabled {
            el = el.disabled(true);
        }
        el = el.with_size(mapping::parse_scale(node.control_size.as_deref()));
        if let Some(callback_id) = node.on_change.clone().or(node.on_click.clone()) {
            let cmd_tx = self.cmd_tx.clone();
            el = el.on_click(move |value, _, _| {
                let _ = cmd_tx.send(Cmd::Callback {
                    id: callback_id.clone(),
                    value: Some(json!(*value)),
                    seq: None,
                });
            });
        }
        apply_style(el, node, cx).into_any_element()
    }

    fn render_toggle(&self, node: &Node, key: &str, cx: &App) -> AnyElement {
        let checked = node.checked.unwrap_or(false);
        let mut el = Toggle::new(eid(key))
            .checked(checked)
            .with_variant(mapping::parse_toggle_variant(node.variant.as_deref()));
        if let Some(text) = node.text.clone() {
            el = el.label(text);
        }
        if node.disabled {
            el = el.disabled(true);
        }
        el = el.with_size(mapping::parse_scale(node.control_size.as_deref()));
        if let Some(callback_id) = node.on_change.clone().or(node.on_click.clone()) {
            let cmd_tx = self.cmd_tx.clone();
            el = el.on_click(move |value, _, _| {
                let _ = cmd_tx.send(Cmd::Callback {
                    id: callback_id.clone(),
                    value: Some(json!(*value)),
                    seq: None,
                });
            });
        }
        apply_style(el, node, cx).into_any_element()
    }

    fn render_radio_group(&self, node: &Node, key: &str, cx: &App) -> AnyElement {
        let selected = node.string_value();
        let items = node.collection();
        let selected_index = selected
            .as_ref()
            .and_then(|id| items.iter().position(|item| &item.id_or_label() == id));
        let mut group = if mapping::parse_axis(node.orientation.as_deref()) == Axis::Horizontal {
            RadioGroup::horizontal(eid(key))
        } else {
            RadioGroup::vertical(eid(key))
        };
        group = group
            .selected_index(selected_index)
            .disabled(node.disabled)
            .children(items.iter().map(|item| {
                Radio::new(SharedString::from(item.id_or_label())).label(item.label_or_id())
            }));
        if let Some(callback_id) = node.on_change.clone() {
            let ids: Vec<String> = items.iter().map(Item::id_or_label).collect();
            let cmd_tx = self.cmd_tx.clone();
            group = group.on_click(move |ix, _, _| {
                if let Some(id) = ids.get(*ix) {
                    let _ = cmd_tx.send(Cmd::Callback {
                        id: callback_id.clone(),
                        value: Some(json!(id)),
                        seq: None,
                    });
                }
            });
        }
        apply_style(group, node, cx).into_any_element()
    }

    fn render_slider(
        &mut self,
        node: &Node,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.slider_slot(key, node, window, cx);
        let mut slider = Slider::new(&state);
        slider = if mapping::parse_axis(node.orientation.as_deref()) == Axis::Vertical {
            slider.vertical()
        } else {
            slider.horizontal()
        };
        if node.disabled {
            slider = slider.disabled(true);
        }
        if node.reverse {
            slider = slider.reverse();
        }
        let mut inner = node.clone();
        inner.flex = None;
        inner.width = None;
        inner.height = None;
        inner.size = None;
        let slider = apply_style(slider, &inner, cx);
        let view = cx.weak_entity();
        let key = key.to_string();
        copy_outer_layout(div().relative().id(eid(&format!("{key}-track"))), node)
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let size = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                        let mut refresh = false;
                        let _ = view.update(cx, |this, _cx| {
                            let Some(slot) = this.sliders.get_mut(&key) else {
                                return;
                            };
                            let changed = match slot.bar_px {
                                None => true,
                                Some((w, h)) => {
                                    (w - size.0).abs() > 0.5 || (h - size.1).abs() > 0.5
                                }
                            };
                            if changed {
                                slot.bar_px = Some(size);
                                slot.settle = 0;
                                refresh = true;
                            } else if slot.settle < 4 {
                                slot.settle += 1;
                                refresh = true;
                            }
                        });
                        if refresh {
                            let view = view.clone();
                            window.on_next_frame(move |_, cx| {
                                let _ = view.update(cx, |_, cx| cx.notify());
                            });
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(slider)
            .into_any_element()
    }

    fn render_progress(&self, node: &Node, cx: &App) -> AnyElement {
        let value = node.number_value().unwrap_or(0.0).clamp(0.0, 100.0);
        apply_style(
            Progress::new(node.id.clone().unwrap_or_else(|| "progress".into())).value(value),
            node,
            cx,
        )
        .into_any_element()
    }

    fn render_progress_circle(
        &mut self,
        node: &Node,
        path: &str,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Kit `transition((id, "value"), …)` keys off this id. Use
        // `widget_key` (`:id` or path) so reorder/insert does not steal
        // another circle's animation. `path` still names children.
        let mut circle = ProgressCircle::new(eid(key))
            .value(extra::progress_circle_value(node))
            .loading(node.loading)
            .with_size(mapping::parse_scale(node.control_size.as_deref()));
        if let Some(color) = node.color.as_deref().and_then(extra::parse_hex_color) {
            circle = circle.color(color);
        }
        if let Some(label) = node
            .accessibility_label
            .as_deref()
            .filter(|s| !s.is_empty())
        {
            circle = circle.accessibility_label(label.to_string());
        }
        apply_style(
            circle.children(self.render_children(node, path, window, cx)),
            node,
            cx,
        )
        .into_any_element()
    }

    fn render_pagination(&self, node: &Node, key: &str, cx: &App) -> AnyElement {
        let mut pagination = Pagination::new(eid(key))
            .current_page(extra::pagination_current_page(node))
            .total_pages(extra::pagination_total_pages(node))
            .with_size(mapping::parse_scale(node.control_size.as_deref()))
            .disabled(node.disabled);
        if node.compact {
            pagination = pagination.compact();
        }
        if let Some(visible) = extra::pagination_visible_pages(node) {
            pagination = pagination.visible_pages(visible);
        }
        if let Some(id) = node.on_change.clone().or(node.on_click.clone()) {
            let cmd_tx = self.cmd_tx.clone();
            pagination = pagination.on_click(move |page, _, _| {
                let _ = cmd_tx.send(Cmd::Callback {
                    id: id.clone(),
                    value: Some(json!(*page)),
                    seq: None,
                });
            });
        }
        apply_style(pagination, node, cx).into_any_element()
    }

    fn render_shimmer(&self, node: &Node, cx: &App) -> AnyElement {
        let text = node.text.clone().unwrap_or_default();
        let mut shimmer = ShimmerText::new(text);
        if let Some(id) = node.id.clone().filter(|s| !s.is_empty()) {
            shimmer = shimmer.id(id);
        }
        if let Some(secs) = extra::shimmer_duration_secs(node) {
            shimmer = shimmer.duration(Duration::from_secs_f32(secs));
        }
        match extra::shimmer_spread(node) {
            Some(extra::ShimmerSpreadSpec::Relative(fraction)) => {
                shimmer = shimmer.spread(fraction);
            }
            Some(extra::ShimmerSpreadSpec::Absolute(pixels)) => {
                shimmer = shimmer.spread(px(pixels));
            }
            None => {}
        }
        if node.reverse {
            shimmer = shimmer.reverse(true);
        }
        if node.once {
            shimmer = shimmer.once(true);
        }
        if let Some(color) = node
            .highlight_color
            .as_deref()
            .and_then(extra::parse_hex_color)
        {
            shimmer = shimmer.highlight_color(color);
        }
        apply_style(shimmer, node, cx).into_any_element()
    }

    fn render_separator(&self, node: &Node, cx: &App) -> AnyElement {
        let mut separator = if mapping::parse_axis(node.orientation.as_deref()) == Axis::Vertical {
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
        apply_style(separator, node, cx).into_any_element()
    }

    fn render_spinner(&self, node: &Node, cx: &App) -> AnyElement {
        let mut spinner =
            Spinner::new().with_size(mapping::parse_scale(node.control_size.as_deref()));
        if let Some(icon) = node.icon.as_deref().and_then(mapping::parse_icon) {
            spinner = spinner.icon(icon);
        }
        // Spinner is not `Styled`; a host div owns Clojure layout/visual keys.
        style_host(spinner, node, cx)
    }

    fn render_tag(&self, node: &Node, cx: &App) -> AnyElement {
        let mut tag = Tag::new().with_variant(mapping::parse_tag_variant(node.variant.as_deref()));
        if node.outline {
            tag = tag.outline();
        }
        tag = tag.with_size(mapping::parse_scale(node.control_size.as_deref()));
        let label = node.text.clone().unwrap_or_default();
        apply_style(tag.child(label), node, cx).into_any_element()
    }

    fn render_alert(&self, node: &Node, key: &str, cx: &App) -> AnyElement {
        let message = node
            .message
            .clone()
            .or_else(|| node.text.clone())
            .unwrap_or_default();
        let variant = node.variant.as_deref().map(catalog::normalize);
        let mut alert = match variant.as_deref() {
            Some("info") => Alert::info(eid(key), message),
            Some("success") => Alert::success(eid(key), message),
            Some("warning") => Alert::warning(eid(key), message),
            Some("error") | Some("danger") => Alert::error(eid(key), message),
            _ => Alert::new(eid(key), message),
        };
        if let Some(title) = node.title.clone() {
            alert = alert.title(title);
        }
        alert = alert.with_size(mapping::parse_scale(node.control_size.as_deref()));
        if let Some(callback_id) = node.on_close.clone() {
            alert = alert.on_close(self.click(callback_id));
        }
        apply_style(alert, node, cx).into_any_element()
    }

    fn render_skeleton(&self, node: &Node, cx: &App) -> AnyElement {
        apply_style(Skeleton::new(), node, cx).into_any_element()
    }

    fn render_kbd(&self, node: &Node, cx: &App) -> AnyElement {
        let text = node.text.clone().unwrap_or_default();
        match Keystroke::parse(&text) {
            Ok(stroke) => apply_style(Kbd::new(stroke), node, cx).into_any_element(),
            Err(_) => apply_style(div().child(text), node, cx).into_any_element(),
        }
    }

    fn render_link(&self, node: &Node, key: &str, cx: &App) -> AnyElement {
        let mut link = Link::new(eid(key));
        if let Some(href) = node.href.clone().filter(|s| !s.is_empty()) {
            link = link.href(href);
        }
        if node.disabled {
            link = link.disabled(true);
        }
        if let Some(callback_id) = node.on_click.clone() {
            link = link.on_click(self.click(callback_id));
        }
        let label = node
            .text
            .clone()
            .unwrap_or_else(|| node.href.clone().unwrap_or_default());
        apply_style(link.child(label), node, cx).into_any_element()
    }

    fn render_group_box(
        &mut self,
        node: &Node,
        path: &str,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut box_ = GroupBox::new()
            .id(eid(key))
            .with_variant(mapping::parse_group_variant(node.variant.as_deref()));
        if let Some(title) = node.title.clone() {
            box_ = box_.title(title);
        }
        apply_style(box_, node, cx)
            .children(self.render_children(node, path, window, cx))
            .into_any_element()
    }

    fn render_badge(
        &mut self,
        node: &Node,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut badge = Badge::new();
        if node.dot {
            badge = badge.dot();
        } else if let Some(count) = node.count {
            badge = badge.count(count as usize);
        } else if let Some(n) = node.number_value() {
            badge = badge.count(n.max(0.0) as usize);
        }
        badge = badge.with_size(mapping::parse_scale(node.control_size.as_deref()));
        // Badge is not `Styled`; wrapper owns :width/:height/:size/:flex.
        style_host(
            badge.children(self.render_children(node, path, window, cx)),
            node,
            cx,
        )
    }

    fn render_tabs(&self, node: &Node, key: &str, cx: &App) -> AnyElement {
        let items = node.collection();
        let selected = node.string_value();
        let selected_index = selected
            .as_ref()
            .and_then(|id| items.iter().position(|item| &item.id_or_label() == id))
            .unwrap_or(0);
        let mut bar = TabBar::new(eid(key))
            .with_variant(mapping::parse_tab_variant(node.variant.as_deref()))
            .with_size(mapping::parse_scale(node.control_size.as_deref()))
            .selected_index(selected_index)
            .children(items.iter().map(|item| Tab::from(item.label_or_id())));
        if let Some(callback_id) = node.on_change.clone() {
            let ids: Vec<String> = items.iter().map(Item::id_or_label).collect();
            let cmd_tx = self.cmd_tx.clone();
            bar = bar.on_click(move |ix, _, _| {
                if let Some(id) = ids.get(*ix) {
                    let _ = cmd_tx.send(Cmd::Callback {
                        id: callback_id.clone(),
                        value: Some(json!(id)),
                        seq: None,
                    });
                }
            });
        }
        apply_style(bar, node, cx).into_any_element()
    }

    fn render_combobox(
        &mut self,
        node: &Node,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.combobox_slot(key, node, window, cx);
        let Some(slot) = self.comboboxes.get(key) else {
            return div().into_any_element();
        };
        match &slot.state {
            ComboboxStateHandle::Flat(state) => finish_combobox(Combobox::new(state), node, cx),
            ComboboxStateHandle::Grouped(state) => finish_combobox(Combobox::new(state), node, cx),
        }
    }

    fn combobox_slot(
        &mut self,
        key: &str,
        node: &Node,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.used_comboboxes.insert(key.to_string());
        let grouped = extra::select_is_grouped(node.collection());
        let fingerprint = extra::combobox_fingerprint(node.collection());
        let selected: Vec<SharedString> = node
            .string_values()
            .into_iter()
            .map(SharedString::from)
            .collect();
        if let Some(slot) = self.comboboxes.get_mut(key) {
            let same_kind = match &slot.state {
                ComboboxStateHandle::Grouped(_) => grouped,
                ComboboxStateHandle::Flat(_) => !grouped,
            };
            if same_kind && slot.searchable == node.searchable && slot.multiple == node.multiple {
                slot.on_change = node.on_change.clone();
                slot.on_confirm = node.on_confirm.clone();
                let query = node.query.as_deref();
                if grouped {
                    match extra::combobox_live_sync(
                        slot.fingerprint,
                        fingerprint,
                        &slot.selected,
                        &selected,
                    ) {
                        extra::ComboboxLiveSync::Leave => {
                            let handle = slot.state.clone();
                            sync_combobox_handle(&handle, None, query, window, cx);
                            return;
                        }
                        extra::ComboboxLiveSync::SetValues => {
                            slot.selected = selected.clone();
                            let handle = slot.state.clone();
                            sync_combobox_handle(&handle, Some(&selected), query, window, cx);
                            return;
                        }
                        extra::ComboboxLiveSync::Rebuild => {
                            // Fall through and recreate so query text cannot
                            // stay attached to a fresh unfiltered SearchableVec.
                        }
                    }
                } else {
                    let sync = extra::combobox_slot_sync(
                        slot.fingerprint,
                        fingerprint,
                        &slot.selected,
                        &selected,
                    );
                    if sync.set_items {
                        slot.fingerprint = fingerprint;
                    }
                    if sync.set_selected {
                        slot.selected = selected.clone();
                    }
                    if let ComboboxStateHandle::Flat(state) = &slot.state {
                        let state = state.clone();
                        if sync.set_items || sync.set_selected {
                            state.update(cx, |state, cx| {
                                if sync.set_items {
                                    state.set_items(combo_flat_vec(node), window, cx);
                                }
                                if sync.set_selected {
                                    // Rebuilds Kit's cloned selection (labels /
                                    // dropped ids). Also clears the search query.
                                    state.set_selected_values(&selected, window, cx);
                                }
                            });
                        }
                        apply_combobox_query(&state, query, window, cx);
                    }
                    return;
                }
            }
        }
        let searchable = node.searchable;
        let multiple = node.multiple;
        let key_owned = key.to_string();
        let handle = if grouped {
            let state = cx.new(|cx| {
                ComboboxState::new(combo_group_vec(node), Vec::new(), window, cx)
                    .searchable(searchable)
                    .multiple(multiple)
            });
            state.update(cx, |state, cx| {
                state.set_selected_values(&selected, window, cx);
            });
            subscribe_combobox(&state, key_owned, window, cx);
            ComboboxStateHandle::Grouped(state)
        } else {
            let state = cx.new(|cx| {
                ComboboxState::new(combo_flat_vec(node), Vec::new(), window, cx)
                    .searchable(searchable)
                    .multiple(multiple)
            });
            state.update(cx, |state, cx| {
                state.set_selected_values(&selected, window, cx);
            });
            subscribe_combobox(&state, key_owned, window, cx);
            ComboboxStateHandle::Flat(state)
        };
        sync_combobox_handle(&handle, None, node.query.as_deref(), window, cx);
        self.comboboxes.insert(
            key.to_string(),
            ComboboxSlot {
                state: handle,
                searchable,
                multiple,
                fingerprint,
                selected,
                on_change: node.on_change.clone(),
                on_confirm: node.on_confirm.clone(),
                coalesce: protocol::ComboboxActivationCoalesce::default(),
            },
        );
    }

    fn flush_pending_combobox_change(&mut self, key: &str) {
        let Some(slot) = self.comboboxes.get_mut(key) else {
            return;
        };
        let Some(payload) = slot.coalesce.take_pending_change() else {
            return;
        };
        let Some(id) = slot.on_change.clone() else {
            return;
        };
        protocol::send_callbacks(
            &self.cmd_tx,
            protocol::combobox_activation_calls(Some(id), None, payload),
        );
    }

    fn render_table(
        &mut self,
        node: &Node,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let size = mapping::parse_scale(node.control_size.as_deref());
        let mut table = Table::new().with_size(size);
        if let Some(label) = extra::table_accessibility_label(node) {
            table = table.accessibility_label(label);
        }
        if table_has_primitive_children(node) {
            table = self.paint_table_sections(table, node, path, window, cx);
        } else {
            table = paint_table_from_items(table, node);
        }
        apply_style(table, node, cx).into_any_element()
    }

    fn paint_table_sections(
        &mut self,
        mut table: Table,
        node: &Node,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Table {
        for (index, child) in node.children.iter().enumerate() {
            let child_path = format!("{path}-{index}");
            match child.kind.as_str() {
                "table-header" => {
                    table = table.child(self.paint_table_header(child, &child_path, window, cx));
                }
                "table-body" => {
                    table = table.child(self.paint_table_body(child, &child_path, window, cx));
                }
                "table-footer" => {
                    table = table.child(self.paint_table_footer(child, &child_path, window, cx));
                }
                "table-caption" => {
                    table = table.child(self.paint_table_caption(child, &child_path, window, cx));
                }
                _ => {}
            }
        }
        table
    }

    fn paint_table_header(
        &mut self,
        node: &Node,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TableHeader {
        let mut header = apply_style(TableHeader::new(), node, cx);
        for (index, child) in node.children.iter().enumerate() {
            let child_path = format!("{path}-{index}");
            header = header.child(self.paint_table_row_node(child, &child_path, window, cx));
        }
        header
    }

    fn paint_table_body(
        &mut self,
        node: &Node,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TableBody {
        let mut body = apply_style(TableBody::new(), node, cx);
        for (index, child) in node.children.iter().enumerate() {
            let child_path = format!("{path}-{index}");
            body = body.child(self.paint_table_row_node(child, &child_path, window, cx));
        }
        body
    }

    fn paint_table_footer(
        &mut self,
        node: &Node,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TableFooter {
        let mut footer = apply_style(TableFooter::new(), node, cx);
        for (index, child) in node.children.iter().enumerate() {
            let child_path = format!("{path}-{index}");
            footer = footer.child(self.paint_table_row_node(child, &child_path, window, cx));
        }
        footer
    }

    fn paint_table_row_node(
        &mut self,
        node: &Node,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TableRow {
        let mut row = apply_style(TableRow::new(), node, cx);
        for (index, child) in node.children.iter().enumerate() {
            let child_path = format!("{path}-{index}");
            match child.kind.as_str() {
                "table-head" => {
                    row = row.child(self.paint_table_head(child, &child_path, window, cx));
                }
                "table-cell" => {
                    row = row.child(self.paint_table_cell(child, &child_path, window, cx));
                }
                _ => {
                    let mut cell = TableCell::new();
                    cell = cell.child(self.render_node(child, &child_path, window, cx));
                    row = row.child(cell);
                }
            }
        }
        row
    }

    fn paint_table_head(
        &mut self,
        node: &Node,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TableHead {
        let mut el = style_table_head_node(TableHead::new(), node);
        el = apply_style(el, node, cx);
        self.fill_table_cell_children(el, node, path, window, cx)
    }

    fn paint_table_cell(
        &mut self,
        node: &Node,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TableCell {
        let mut el = style_table_cell_node(TableCell::new(), node);
        el = apply_style(el, node, cx);
        self.fill_table_cell_children(el, node, path, window, cx)
    }

    fn paint_table_caption(
        &mut self,
        node: &Node,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TableCaption {
        let mut caption = apply_style(TableCaption::new(), node, cx);
        caption = self.fill_table_cell_children(caption, node, path, window, cx);
        caption
    }

    fn fill_table_cell_children<E>(
        &mut self,
        mut el: E,
        node: &Node,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> E
    where
        E: ParentElement,
    {
        if node.children.is_empty() {
            if let Some(text) = node.text.clone().filter(|s| !s.is_empty()) {
                el = el.child(text);
            }
        } else {
            for child in self.render_children(node, path, window, cx) {
                el = el.child(child);
            }
        }
        el
    }

    fn render_rating(&self, node: &Node, key: &str, cx: &App) -> AnyElement {
        let (max, value) = extra::rating_max_then_value(node);
        let mut rating = Rating::new(eid(key))
            .max(max)
            .value(value)
            .with_size(mapping::parse_scale(node.control_size.as_deref()))
            .disabled(node.disabled);
        if let Some(color) = node.color.as_deref().and_then(extra::parse_hex_color) {
            rating = rating.color(color);
        }
        if let Some(id) = node.on_change.clone().or(node.on_click.clone()) {
            let cmd_tx = self.cmd_tx.clone();
            rating = rating.on_click(move |value, _, _| {
                let _ = cmd_tx.send(Cmd::Callback {
                    id: id.clone(),
                    value: Some(json!(*value)),
                    seq: None,
                });
            });
        }
        apply_style(rating, node, cx).into_any_element()
    }

    fn render_stepper(&self, node: &Node, key: &str, cx: &App) -> AnyElement {
        let items = node.collection();
        let selected = extra::stepper_selected_index(items, node.string_value().as_deref());
        let mut stepper = Stepper::new(eid(key))
            .selected_index(selected)
            .layout(mapping::parse_axis(node.orientation.as_deref()))
            .with_size(mapping::parse_scale(node.control_size.as_deref()))
            .disabled(node.disabled);
        for item in items {
            let mut step = StepperItem::new().child(item.label_or_id());
            if item.disabled {
                step = step.disabled(true);
            }
            if let Some(icon) = item.icon.as_deref().and_then(mapping::parse_icon) {
                step = step.icon(icon);
            }
            stepper = stepper.item(step);
        }
        if let Some(callback_id) = node.on_change.clone().or(node.on_click.clone()) {
            let ids: Vec<String> = items.iter().map(Item::id_or_label).collect();
            let cmd_tx = self.cmd_tx.clone();
            stepper = stepper.on_click(move |ix, _, _| {
                if let Some(id) = ids.get(*ix) {
                    let _ = cmd_tx.send(Cmd::Callback {
                        id: callback_id.clone(),
                        value: Some(json!(id)),
                        seq: None,
                    });
                }
            });
        }
        apply_style(stepper, node, cx).into_any_element()
    }

    fn render_icon(&self, node: &Node, cx: &App) -> AnyElement {
        let name = node
            .icon
            .as_deref()
            .or(node.text.as_deref())
            .unwrap_or("check");
        let icon = mapping::parse_icon(name).unwrap_or(IconName::Asterisk);
        apply_style(
            Icon::new(icon).with_size(mapping::parse_scale(node.control_size.as_deref())),
            node,
            cx,
        )
        .into_any_element()
    }

    fn render_clipboard(&self, node: &Node, key: &str, cx: &App) -> AnyElement {
        let mut clip = Clipboard::new(eid(key)).value(node.text.clone().unwrap_or_default());
        if let Some(callback_id) = node.on_copied.clone() {
            let cmd_tx = self.cmd_tx.clone();
            clip = clip.on_copied(move |value, _, _| {
                let _ = cmd_tx.send(Cmd::Callback {
                    id: callback_id.clone(),
                    value: Some(json!(value.to_string())),
                    seq: None,
                });
            });
        }
        // Clipboard is not `Styled`; wrapper owns Clojure layout keys.
        style_host(clip, node, cx)
    }

    fn render_breadcrumb(&self, node: &Node, _key: &str, cx: &App) -> AnyElement {
        let items = node.collection();
        let last = items.len().saturating_sub(1);
        let mut crumb = Breadcrumb::new();
        for (ix, item) in items.iter().enumerate() {
            let mut entry = BreadcrumbItem::new(item.label_or_id()).disabled(item.disabled);
            if ix != last {
                if let Some(callback_id) = item.on_click.clone().or_else(|| node.on_change.clone())
                {
                    let id = item.id_or_label();
                    let cmd_tx = self.cmd_tx.clone();
                    if node.on_change.is_some() && item.on_click.is_none() {
                        entry = entry.on_click(move |_, _, _| {
                            let _ = cmd_tx.send(Cmd::Callback {
                                id: callback_id.clone(),
                                value: Some(json!(id.clone())),
                                seq: None,
                            });
                        });
                    } else {
                        entry = entry.on_click(self.click(callback_id));
                    }
                }
            }
            crumb = crumb.child(entry);
        }
        apply_style(crumb, node, cx).into_any_element()
    }

    fn render_avatar(&self, node: &Node, cx: &App) -> AnyElement {
        apply_style(overlay::kit_avatar(node), node, cx).into_any_element()
    }

    fn render_avatar_group(&self, node: &Node, cx: &App) -> AnyElement {
        // Kit `AvatarGroup: Styled` keys (`:gap`, padding, colors, …) refine
        // the real group. The clip wrap only takes workaround geometry plus
        // Clojure box keys (`:width` / `:height` / `:size` / `:flex`).
        let group = apply_kit_visual_style(overlay::kit_avatar_group(node), node, cx);
        apply_outer_box_style(overlay::avatar_group_element(group, node), node).into_any_element()
    }

    fn render_hover_card(
        &mut self,
        node: &Node,
        path: &str,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut card = HoverCard::new(eid(key));
        if let Some(anchor) = mapping::parse_anchor(node.placement.as_deref()) {
            card = card.anchor(anchor);
        }
        if let Some(secs) = extra::hover_card_delay_secs(node.open_delay) {
            card = card.open_delay(Duration::from_secs_f32(secs));
        }
        if let Some(secs) = extra::hover_card_delay_secs(node.close_delay) {
            card = card.close_delay(Duration::from_secs_f32(secs));
        }
        if let Some(appearance) = node.appearance {
            card = card.appearance(appearance);
        }
        if let Some(trigger) = node.trigger.as_deref() {
            card = card.trigger(self.render_node(trigger, &format!("{path}-trigger"), window, cx));
        }
        card = card.children(self.render_children(node, path, window, cx));
        if let Some(callback_id) = node.on_open_change.clone() {
            let cmd_tx = self.cmd_tx.clone();
            card = card.on_open_change(move |open, _, _| {
                let _ = cmd_tx.send(Cmd::Callback {
                    id: callback_id.clone(),
                    value: Some(json!(*open)),
                    seq: None,
                });
            });
        }
        apply_style(card, node, cx).into_any_element()
    }

    fn render_accordion(
        &mut self,
        node: &Node,
        path: &str,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let items = node.collection();
        let open_ids = node.string_values();
        let mut accordion = Accordion::new(eid(key)).multiple(node.multiple);
        if node.disabled {
            accordion = accordion.disabled(true);
        }
        accordion = accordion.with_size(mapping::parse_scale(node.control_size.as_deref()));
        for (ix, item) in items.iter().enumerate() {
            let id = item.id_or_label();
            let title = item.label_or_id();
            let is_open = open_ids.iter().any(|open| open == &id);
            let content = if let Some(child) = item.content.as_ref() {
                self.render_node(child, &format!("{path}-acc-{ix}"), window, cx)
            } else {
                div().into_any_element()
            };
            accordion = accordion.item(move |acc| acc.title(title).open(is_open).child(content));
        }
        if let Some(callback_id) = node.on_change.clone() {
            let ids: Vec<String> = items.iter().map(Item::id_or_label).collect();
            let multiple = node.multiple;
            let cmd_tx = self.cmd_tx.clone();
            accordion = accordion.on_toggle_click(move |open_ixs, _, _| {
                let _ = cmd_tx.send(Cmd::Callback {
                    id: callback_id.clone(),
                    value: Some(accordion_callback_value(&ids, open_ixs, multiple)),
                    seq: None,
                });
            });
        }
        // Accordion::render uses size_full(); as a flex child that steals the
        // leftover viewport and squeezes every sibling below it. Outer wrapper
        // owns :width/:height/:size/:flex; inner stays content-sized.
        content_sized(accordion, node, cx)
    }

    fn render_description_list(&self, node: &Node, cx: &App) -> AnyElement {
        let mut list =
            if mapping::parse_description_axis(node.orientation.as_deref()) == Axis::Vertical {
                DescriptionList::vertical()
            } else {
                DescriptionList::horizontal()
            };
        list = list.with_size(mapping::parse_scale(node.control_size.as_deref()));
        list = list.columns(mapping::parse_columns(node.columns));
        for item in node.collection() {
            let label = item
                .label
                .clone()
                .or_else(|| item.id.clone())
                .unwrap_or_default();
            let value = item.text.clone().unwrap_or_default();
            list = list.item(label, value, mapping::parse_span(item.span));
        }
        content_sized(list, node, cx)
    }

    fn render_popover(&self, node: &Node, key: &str, cx: &Context<Self>) -> AnyElement {
        let open = node.open.unwrap_or(false);
        let content = node.children.clone();
        let emit = Self::action_emitter(cx);
        let content_path = format!("{key}/content");
        let mut popover = Popover::new(eid(key))
            .open(open)
            .trigger(overlay::trigger_button(node.trigger.as_deref(), key))
            .content({
                let emit = emit.clone();
                move |_, _, _| overlay::paint_static(&content, emit.clone(), &content_path)
            });
        if node.on_open_change.is_some() {
            let key = key.to_string();
            popover = popover.on_open_change(move |open, _, cx| {
                emit(
                    overlay::QueuedAction::PopoverOpen {
                        key: key.clone(),
                        open: *open,
                    },
                    cx,
                );
            });
        } else {
            let _ = emit;
        }
        apply_style(popover, node, cx).into_any_element()
    }

    fn render_dropdown_menu(
        &self,
        node: &Node,
        key: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let items = node.items.clone();
        let emit = Self::action_emitter(cx);
        let key = key.to_string();
        let button = apply_style(
            overlay::trigger_button(node.trigger.as_deref(), &key),
            node,
            cx,
        );
        button
            .dropdown_menu(move |menu, window, cx| {
                overlay::fill_popup_menu(menu, &items, &key, &[], emit.clone(), window, cx)
            })
            .into_any_element()
    }

    fn render_dropdown_button(
        &self,
        node: &Node,
        path: &str,
        key: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let items = node.items.clone();
        let emit = Self::action_emitter(cx);
        let key = key.to_string();
        let mut dropdown =
            overlay::apply_dropdown_button_chrome(DropdownButton::new(eid(&key)), node);
        if let Some(trigger) = node.trigger.as_deref() {
            let mut button = overlay::trigger_button(Some(trigger), &key);
            if trigger.compact || node.compact {
                button = button.compact();
            }
            if trigger.loading || node.loading {
                button = button.loading(true);
            }
            if let Some(text) = trigger.tooltip.clone().filter(|s| !s.is_empty()) {
                button = button.tooltip(text);
            }
            if trigger.on_click.is_some() {
                let emit = emit.clone();
                let click_key = overlay::node_key(trigger, &format!("{path}-trigger"));
                button = button.on_click(move |_, _, cx| {
                    emit(
                        overlay::QueuedAction::ButtonClick {
                            key: click_key.clone(),
                        },
                        cx,
                    );
                });
            }
            dropdown = dropdown.button(button);
        }
        let emit_menu = emit.clone();
        let menu_key = key.clone();
        dropdown = if let Some(anchor) = mapping::parse_anchor(node.placement.as_deref()) {
            dropdown.dropdown_menu_with_anchor(anchor, move |menu, window, cx| {
                overlay::fill_popup_menu(
                    menu,
                    &items,
                    &menu_key,
                    &[],
                    emit_menu.clone(),
                    window,
                    cx,
                )
            })
        } else {
            dropdown.dropdown_menu(move |menu, window, cx| {
                overlay::fill_popup_menu(
                    menu,
                    &items,
                    &menu_key,
                    &[],
                    emit_menu.clone(),
                    window,
                    cx,
                )
            })
        };
        apply_style(dropdown, node, cx).into_any_element()
    }

    fn render_context_menu(
        &mut self,
        node: &Node,
        path: &str,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let items = node.items.clone();
        let emit = Self::action_emitter(cx);
        let key = key.to_string();
        // Flex column, not a block `div`. A `:flex 1` list/table/tree skips
        // default viewport height and uses crate `size_full()`; inside a
        // non-flex host that `flex_1` is ignored and the listing collapses.
        let mut el = apply_style(v_flex().id(eid(&key)).min_h_0(), node, cx);
        // Inherit leftover height from a flex-fill child only when `:flex`
        // was omitted. An explicit wrapper value (`0`, `0.5`, `1`) is kept.
        if node.flex.is_none() && context_menu_flex_fill(node) {
            el = el.flex_1().min_w_0().min_h_0();
        }
        el.context_menu(move |menu, window, cx| {
            overlay::fill_popup_menu(menu, &items, &key, &[], emit.clone(), window, cx)
        })
        .children(self.render_children(node, path, window, cx))
        .into_any_element()
    }

    fn command_slot(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<CommandState> {
        self.used_commands.insert(key.to_string());
        if let Some(slot) = self.commands.get(key) {
            return slot.state.clone();
        }
        let state = cx.new(|cx| CommandState::new(window, cx));
        self.commands.insert(
            key.to_string(),
            CommandSlot {
                state: state.clone(),
                suppress_select: false,
                pending_select: None,
                query_latch: None,
                selected_latch: None,
                programmatic_query: None,
            },
        );
        state
    }

    fn sync_command_state(
        &mut self,
        key: &str,
        node: &Node,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.commands.get(key).map(|slot| slot.state.clone()) else {
            return;
        };
        let mut suppress = false;
        let tree_seq = self.tree_seq;
        if let Some(desired) = node.query.as_ref() {
            let current = state.read(cx).query(cx).to_string();
            if current != *desired {
                let latch = self
                    .commands
                    .get(key)
                    .and_then(|slot| slot.query_latch.as_ref());
                if action_bridge::should_apply_command_echo(latch, Some(&current), tree_seq) {
                    if let Some(slot) = self.commands.get_mut(key) {
                        slot.programmatic_query = Some(desired.clone());
                        slot.suppress_select = true;
                    }
                    suppress = true;
                    let desired = desired.clone();
                    state.update(cx, |cmd, cx| {
                        cmd.set_query(desired, window, cx);
                    });
                }
            }
        }
        match action_bridge::command_value_path(node.value.as_ref()) {
            None => {}
            Some(path) => {
                let desired_ix = if path.is_empty() {
                    None
                } else {
                    action_bridge::command_index_path(&node.items, &path)
                };
                let current_ix = state.read(cx).selected_index();
                if current_ix != desired_ix {
                    let current_path =
                        current_ix.and_then(|ix| action_bridge::command_item_path(&node.items, ix));
                    let latch = self
                        .commands
                        .get(key)
                        .and_then(|slot| slot.selected_latch.as_ref());
                    if action_bridge::should_apply_command_echo(
                        latch,
                        current_path.as_ref(),
                        tree_seq,
                    ) {
                        if let Some(slot) = self.commands.get_mut(key) {
                            slot.suppress_select = true;
                        }
                        suppress = true;
                        state.update(cx, |cmd, cx| {
                            cmd.set_selected_index(desired_ix, window, cx);
                        });
                    }
                }
            }
        }
        // The matching callback-seq tree consumes the latch even when
        // Clojure omits `:query` / `:selected` or returns a different
        // value. Unrelated trees leave it in place.
        if let Some(slot) = self.commands.get_mut(key) {
            if slot
                .query_latch
                .as_ref()
                .is_some_and(|latch| tree_seq == Some(latch.seq))
            {
                slot.query_latch = None;
            }
            if slot
                .selected_latch
                .as_ref()
                .is_some_and(|latch| tree_seq == Some(latch.seq))
            {
                slot.selected_latch = None;
            }
        }
        let loading = node.loading;
        if state.read(cx).is_loading() != loading {
            state.update(cx, |cmd, cx| {
                cmd.set_loading(loading, window, cx);
            });
        }
        if node.focus {
            state.update(cx, |cmd, cx| {
                cmd.focus(window, cx);
            });
        }
        if suppress {
            let key = key.to_string();
            let entity = cx.entity();
            cx.defer(move |app| {
                let _ = entity.update(app, |this, _cx| {
                    if let Some(slot) = this.commands.get_mut(&key) {
                        slot.suppress_select = false;
                    }
                });
            });
        }
    }

    fn render_command(
        &mut self,
        node: &Node,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.command_slot(key, window, cx);
        let emit = Self::action_emitter(cx);
        let entity = cx.weak_entity();
        let mut command =
            action_bridge::apply_command_entries(KitCommand::new(&state), &node.items, key);
        command = command.searchable(node.searchable);
        command = command.filterable(node.filterable.unwrap_or(true));
        if let Some(bordered) = node.bordered {
            command = command.bordered(bordered);
        }
        if let Some(placeholder) = node.placeholder.clone().filter(|s| !s.is_empty()) {
            command = command.placeholder(placeholder);
        }
        if let Some(empty) = node.empty.clone().filter(|s| !s.is_empty()) {
            command = command.empty(move |_, _, _| empty.clone());
        }
        if let Some(h) = node.menu_max_h.filter(|h| *h > 0.0) {
            command = command.max_h(px(h));
        }
        if node.on_select.is_some() {
            let entity = entity.clone();
            let key = key.to_string();
            let items = node.items.clone();
            command = command.on_select(move |index_path, _, cx| {
                let _ = entity.update(cx, |this, _| {
                    if this
                        .commands
                        .get(&key)
                        .is_some_and(|slot| slot.suppress_select)
                    {
                        return;
                    }
                    if this.callback_queue.has_clj_select(&key) {
                        return;
                    }
                    let Some(item_path) = action_bridge::command_item_path(&items, index_path)
                    else {
                        return;
                    };
                    this.callback_queue
                        .push(overlay::QueuedAction::CommandSelect {
                            key: key.clone(),
                            item_path,
                        });
                    this.flush_callback_queue();
                });
            });
        }
        {
            let entity = entity.clone();
            let key = key.to_string();
            let items = node.items.clone();
            command = command.on_confirm(move |index_path, _, cx| {
                let _ = entity.update(cx, |this, _| {
                    let item_path = this
                        .commands
                        .get_mut(&key)
                        .and_then(|slot| slot.pending_select.take())
                        .or_else(|| action_bridge::command_item_path(&items, index_path))
                        .unwrap_or_default();
                    this.callback_queue
                        .push(overlay::QueuedAction::CommandConfirm {
                            key: key.clone(),
                            item_path,
                        });
                    this.flush_callback_queue();
                });
            });
        }
        if node.on_query.is_some() {
            let entity = entity.clone();
            let key = key.to_string();
            command = command.on_query(move |query, _, cx| {
                let query = query.to_string();
                let _ = entity.update(cx, |this, _| {
                    if this
                        .commands
                        .get(&key)
                        .and_then(|slot| slot.programmatic_query.as_ref())
                        == Some(&query)
                    {
                        if let Some(slot) = this.commands.get_mut(&key) {
                            slot.programmatic_query = None;
                        }
                        return;
                    }
                    this.callback_queue
                        .push(overlay::QueuedAction::CommandQuery {
                            key: key.clone(),
                            query,
                        });
                    this.flush_callback_queue();
                });
            });
        }
        if node.on_cancel.is_some() {
            let emit = emit.clone();
            let key = key.to_string();
            command = command.on_cancel(move |_, cx| {
                emit(
                    overlay::QueuedAction::CommandCancel { key: key.clone() },
                    cx,
                );
            });
        }
        let _ = emit;
        let command = apply_style(command, node, cx);
        // Command::render calls install_model. Controlled query/selection
        // resolve against that matched list, so they must run after this.
        // RenderOnce returns `impl IntoElement`, which in edition 2024
        // captures the `&mut Window` / `&mut App` lifetimes; convert to
        // an owned AnyElement so the later sync can borrow them again.
        let element = command.render(window, cx).into_any_element();
        self.sync_command_state(key, node, window, cx);
        element
    }

    fn render_status_bar(
        &mut self,
        node: &Node,
        path: &str,
        _key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut bar = StatusBar::new();
        for (index, child) in node.left.iter().enumerate() {
            bar = bar.left(self.render_node(child, &format!("{path}-left-{index}"), window, cx));
        }
        for (index, child) in node.right.iter().enumerate() {
            bar = bar.right(self.render_node(child, &format!("{path}-right-{index}"), window, cx));
        }
        apply_style(bar, node, cx)
            .children(self.render_children(node, path, window, cx))
            .into_any_element()
    }

    fn render_list(
        &mut self,
        node: &Node,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.list_slot(key, node, window, cx);
        viewport_sized(List::new(&state), node, 200.0, cx)
    }

    fn list_slot(
        &mut self,
        key: &str,
        node: &Node,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ListState<RowListDelegate>> {
        self.used_lists.insert(key.to_string());
        let items = rows::rows_from_items(node.collection());
        let fingerprint = rows::rows_fingerprint(node.collection());
        let searchable = node.searchable;
        let selected = node.string_value();

        if let Some(slot) = self.lists.get_mut(key) {
            slot.on_change = node.on_change.clone();
            slot.on_confirm = node.on_confirm.clone();
            let state = slot.state.clone();
            if slot.fingerprint != fingerprint || slot.searchable != searchable {
                slot.fingerprint = fingerprint;
                slot.searchable = searchable;
                let items = items.clone();
                state.update(cx, |list, cx| {
                    list.delegate_mut().set_items(items);
                    list.set_searchable(searchable, cx);
                    cx.notify();
                });
            }
            sync_list_selection(&state, selected.as_deref(), window, cx);
            return state;
        }

        let delegate = RowListDelegate::new(items);
        let state = cx.new(|cx| ListState::new(delegate, window, cx).searchable(searchable));
        sync_list_selection(&state, selected.as_deref(), window, cx);
        let key_owned = key.to_string();
        cx.subscribe(&state, move |this, _, event: &ListEvent, cx| match event {
            ListEvent::Select(ix) => {
                emit_list_change(this, &key_owned, *ix, cx);
            }
            ListEvent::Confirm(ix) => {
                // 0.5.1: arrows emit Select only; mouse click and Enter emit
                // Confirm only. Treat confirm as selection + activation in
                // one batch so :on-change cannot rewire :on-confirm's id.
                emit_list_activation(this, &key_owned, *ix, cx);
            }
            ListEvent::Cancel => {
                if let Some(callback) = this.lists.get(&key_owned).and_then(|s| s.on_change.clone())
                {
                    this.emit_value(callback, Value::Null);
                }
            }
        })
        .detach();
        self.lists.insert(
            key.to_string(),
            ListSlot {
                state: state.clone(),
                fingerprint,
                searchable,
                on_change: node.on_change.clone(),
                on_confirm: node.on_confirm.clone(),
            },
        );
        state
    }

    fn render_data_table(
        &mut self,
        node: &Node,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.table_slot(key, node, window, cx);
        viewport_sized(DataTable::new(&state), node, 220.0, cx)
    }

    fn table_slot(
        &mut self,
        key: &str,
        node: &Node,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TableState<RowTableDelegate>> {
        self.used_tables.insert(key.to_string());
        let columns = rows::columns_from_items(&node.options);
        let rows = rows::rows_from_items(&node.items);
        let fingerprint = rows::table_fingerprint(&node.options, &node.items);
        let selected = node.string_value();

        if let Some(slot) = self.tables.get_mut(key) {
            slot.on_change = node.on_change.clone();
            slot.on_confirm = node.on_confirm.clone().or(node.on_double_click.clone());
            let state = slot.state.clone();
            if slot.fingerprint != fingerprint {
                slot.fingerprint = fingerprint;
                let columns = columns.clone();
                let rows = rows.clone();
                state.update(cx, |table, cx| {
                    table.delegate_mut().columns = columns;
                    table.delegate_mut().rows = rows;
                    table.refresh(cx);
                });
            }
            self.sync_table_selection(key, &state, selected.as_deref(), cx);
            return state;
        }

        let delegate = RowTableDelegate::new(columns, rows);
        let state = cx.new(|cx| TableState::new(delegate, window, cx));
        // Subscribe after the first programmatic select so SelectRow from
        // `set_selected_row` has no listener yet. Reuse uses suppress_select.
        self.sync_table_selection(key, &state, selected.as_deref(), cx);
        let key_owned = key.to_string();
        cx.subscribe_in(
            &state,
            window,
            move |this, _, event: &TableEvent, window, cx| match event {
                TableEvent::SelectRow(ix) => {
                    let suppress = this
                        .tables
                        .get(&key_owned)
                        .is_some_and(|s| s.suppress_select);
                    let schedule = this
                        .tables
                        .get_mut(&key_owned)
                        .is_some_and(|slot| slot.coalesce.on_select_row(*ix, suppress));
                    if !schedule {
                        return;
                    }
                    // End of this effect cycle: after already-queued
                    // DoubleClickedRow, or as the lone count-1 :on-change.
                    let key = key_owned.clone();
                    cx.defer_in(window, move |this, _, cx| {
                        this.flush_pending_table_select(&key, cx);
                    });
                }
                TableEvent::DoubleClickedRow(ix) => {
                    let include_change = this
                        .tables
                        .get_mut(&key_owned)
                        .is_some_and(|s| s.coalesce.on_double_clicked_row(*ix));
                    emit_table_activation(this, &key_owned, *ix, include_change, cx);
                }
                _ => {}
            },
        )
        .detach();
        self.tables.insert(
            key.to_string(),
            TableSlot {
                state: state.clone(),
                fingerprint,
                on_change: node.on_change.clone(),
                on_confirm: node.on_confirm.clone().or(node.on_double_click.clone()),
                suppress_select: false,
                coalesce: protocol::TableClickCoalesce::default(),
            },
        );
        state
    }

    fn sync_table_selection(
        &mut self,
        key: &str,
        state: &Entity<TableState<RowTableDelegate>>,
        selected: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let decision = {
            let table = state.read(cx);
            rows::selection_sync(
                selected,
                table.selected_row(),
                |id| table.delegate().index_of(id),
                |id| table.delegate().contains_id(id),
            )
        };
        if matches!(decision, SelectionSync::Keep) {
            return;
        }
        // `set_selected_row` only queues `Effect::Emit`. Clearing suppress
        // here would let the subscriber treat programmatic select as a
        // user click. Drop the flag at the end of this effect cycle.
        if let Some(slot) = self.tables.get_mut(key) {
            slot.suppress_select = true;
        }
        state.update(cx, |table, cx| match decision {
            SelectionSync::Select(ix) => table.set_selected_row(ix, cx),
            SelectionSync::Clear => table.clear_selection(cx),
            SelectionSync::Keep => {}
        });
        let key = key.to_string();
        let entity = cx.entity();
        cx.defer(move |app| {
            let _ = entity.update(app, |this, _cx| {
                if let Some(slot) = this.tables.get_mut(&key) {
                    slot.suppress_select = false;
                }
            });
        });
    }

    /// Lone `:on-change` after `SelectRow` when `DoubleClickedRow` does
    /// not follow from the same `on_row_left_click`. Always consume the
    /// pending row so a change-less table cannot leave a stuck flush.
    fn flush_pending_table_select(&mut self, key: &str, cx: &App) {
        let Some(slot) = self.tables.get_mut(key) else {
            return;
        };
        let Some(ix) = slot.coalesce.take_pending_select() else {
            return;
        };
        let callback = slot.on_change.clone();
        let state = slot.state.clone();
        let Some(callback) = callback else {
            return;
        };
        let Some(row_id) = state.read(cx).delegate().id_at(ix) else {
            return;
        };
        protocol::send_callbacks(
            &self.cmd_tx,
            protocol::table_activation_calls(Some(callback), None, row_id),
        );
    }

    fn render_tree(
        &mut self,
        node: &Node,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.tree_slot(key, node, window, cx);
        let on_change = node.on_change.clone();
        let cmd_tx = self.cmd_tx.clone();
        let view = tree(&state, move |ix, entry, selected, _, _| {
            let id = entry.item().id.to_string();
            let label = entry.item().label.clone();
            let cmd_tx = cmd_tx.clone();
            let on_change = on_change.clone();
            gpui_component::list::ListItem::new(ix)
                .pl(px(16. * entry.depth() as f32))
                .selected(selected)
                .child(label)
                .on_click(move |_, _, _| {
                    if let Some(callback_id) = on_change.clone() {
                        let _ = cmd_tx.send(Cmd::Callback {
                            id: callback_id,
                            value: Some(json!(id.clone())),
                            seq: None,
                        });
                    }
                })
        });
        viewport_sized(view, node, 200.0, cx)
    }

    fn tree_slot(
        &mut self,
        key: &str,
        node: &Node,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TreeState> {
        self.used_trees.insert(key.to_string());
        let fingerprint = rows::rows_fingerprint(node.collection());
        let items = rows::tree_items_from_protocol(node.collection());
        let selected = node.string_value();

        if self.trees.contains_key(key) {
            let (state, live, refresh) = {
                let slot = self.trees.get_mut(key).unwrap();
                slot.on_change = node.on_change.clone();
                let refresh = slot.fingerprint != fingerprint;
                if refresh {
                    slot.fingerprint = fingerprint;
                    slot.items = items;
                }
                (slot.state.clone(), slot.items.clone(), refresh)
            };
            if refresh {
                let next = live.clone();
                state.update(cx, |tree, cx| {
                    tree.set_items(next, cx);
                });
            }
            sync_tree_selection(&state, &live, selected.as_deref(), cx);
            return state;
        }

        let state = cx.new(|cx| TreeState::new(cx).items(items.clone()));
        sync_tree_selection(&state, &items, selected.as_deref(), cx);
        self.trees.insert(
            key.to_string(),
            TreeSlot {
                state: state.clone(),
                items,
                fingerprint,
                on_change: node.on_change.clone(),
            },
        );
        state
    }

    fn sync_native_menus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let menus = self
            .tree
            .as_ref()
            .map(overlay::collect_native_menus)
            .unwrap_or_default();
        let mut to_show = Vec::new();
        let mut live_keys = HashSet::new();
        for (key, node) in menus {
            let open = node.open.unwrap_or(false);
            let was = self.native_menu_open.get(&key).copied().unwrap_or(false);
            if action_bridge::native_menu_should_show(was, open) {
                to_show.push((key.clone(), node));
            }
            live_keys.insert(key.clone());
            self.native_menu_open.insert(key, open);
        }
        self.native_menu_open
            .retain(|key, _| live_keys.contains(key));
        if to_show.is_empty() {
            return;
        }
        let emit = Self::action_emitter(cx);
        window.on_next_frame(move |window, cx| {
            for (key, node) in to_show {
                let position = action_bridge::native_menu_position(&node, window);
                action_bridge::fill_native_menu(&node.items, &key).show(position, window, cx);
                emit(overlay::QueuedAction::PopoverOpen { key, open: false }, cx);
            }
        });
    }

    fn sync_dialogs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted = self
            .tree
            .as_ref()
            .map(overlay::collect_open_dialogs)
            .unwrap_or_default();
        let wanted_keys = overlay::dialog_keys(&wanted);
        let crate_open = window.has_active_dialog(cx);
        let keys_changed = wanted_keys != self.dialog_keys;
        let waiting =
            overlay::crate_dismiss_waiting_for_clojure(&wanted_keys, &self.dialog_keys, crate_open);
        let should_open = !wanted_keys.is_empty() && !crate_open && !waiting;
        let should_close = wanted_keys.is_empty() && crate_open;
        // Always refresh the live cell so an already-open dialog builder
        // (`render_dialog_layer` each paint) sees the latest callback ids,
        // title, and body. Do not re-enter RootView from that builder.
        self.dialogs = wanted.clone();
        *self.dialog_live.borrow_mut() = wanted;
        if self.dialog_pending {
            return;
        }
        if !(keys_changed || should_open || should_close) {
            return;
        }
        self.dialog_pending = true;
        let entity = cx.entity();
        let emit = Self::action_emitter(cx);
        window.on_next_frame(move |window, cx| {
            window.close_all_dialogs(cx);
            let (keys, live) = entity.update(cx, |this, _| {
                this.dialog_pending = false;
                let keys = overlay::dialog_keys(&this.dialogs);
                this.dialog_keys = keys.clone();
                (keys, this.dialog_live.clone())
            });
            for key in keys {
                let live = live.clone();
                let emit = emit.clone();
                let close = Rc::new(RefCell::new(overlay::DialogClose::default()));
                let is_alert = live
                    .borrow()
                    .iter()
                    .any(|spec| spec.key == key && spec.node.kind == "alert-dialog");
                if is_alert {
                    window.open_alert_dialog(cx, move |alert, _, _cx| {
                        let Some(spec) = overlay::latest_dialog_spec(&live, &key) else {
                            return alert;
                        };
                        let children = vec![overlay::paint_static(
                            &spec.node.children,
                            emit.clone(),
                            &format!("{}/content", spec.key),
                        )];
                        let alert = overlay::configure_alert_dialog(alert, &spec.node, children);
                        overlay::bind_alert_dialog_callbacks(
                            alert,
                            key.clone(),
                            emit.clone(),
                            close.clone(),
                        )
                    });
                } else {
                    window.open_dialog(cx, move |dialog, _, _cx| {
                        let Some(spec) = overlay::latest_dialog_spec(&live, &key) else {
                            return dialog;
                        };
                        let children = vec![overlay::paint_static(
                            &spec.node.children,
                            emit.clone(),
                            &format!("{}/content", spec.key),
                        )];
                        let dialog = overlay::configure_dialog(dialog, &spec.node, children);
                        overlay::bind_dialog_callbacks(
                            dialog,
                            key.clone(),
                            emit.clone(),
                            close.clone(),
                        )
                    });
                }
            }
        });
    }

    fn sync_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted = self.tree.as_ref().and_then(overlay::collect_open_sheet);
        let wanted_key = wanted.as_ref().map(|spec| spec.key.clone());
        let crate_open = window.has_active_sheet(cx);
        let keys_changed = wanted_key != self.sheet_key;
        let wanted_keys: Vec<String> = wanted_key.iter().cloned().collect();
        let current_keys: Vec<String> = self.sheet_key.iter().cloned().collect();
        let waiting =
            overlay::crate_dismiss_waiting_for_clojure(&wanted_keys, &current_keys, crate_open);
        let should_open = wanted_key.is_some() && !crate_open && !waiting;
        let should_close = wanted_key.is_none() && crate_open;
        self.sheet = wanted.clone();
        *self.sheet_live.borrow_mut() = wanted;
        if self.sheet_pending {
            return;
        }
        if !(keys_changed || should_open || should_close) {
            return;
        }
        self.sheet_pending = true;
        let entity = cx.entity();
        let emit = Self::action_emitter(cx);
        window.on_next_frame(move |window, cx| {
            window.close_sheet(cx);
            let (key, live, cmd_tx, placement) = entity.update(cx, |this, _| {
                this.sheet_pending = false;
                let key = this.sheet.as_ref().map(|spec| spec.key.clone());
                this.sheet_key = key.clone();
                let placement = this
                    .sheet
                    .as_ref()
                    .map(|spec| extra::parse_sheet_placement(&spec.node))
                    .unwrap_or(gpui_component::Placement::Right);
                (key, this.sheet_live.clone(), this.cmd_tx.clone(), placement)
            });
            let Some(key) = key else {
                return;
            };
            window.open_sheet_at(placement, cx, move |sheet, _, _| {
                let Some(spec) = overlay::latest_sheet_spec(&live, &key) else {
                    return sheet;
                };
                let children = vec![overlay::paint_static(
                    &spec.node.children,
                    emit.clone(),
                    &format!("{}/content", spec.key),
                )];
                let footer = spec.node.footer.as_ref().map(|node| {
                    overlay::paint_static(
                        std::slice::from_ref(node.as_ref()),
                        emit.clone(),
                        &format!("{}/footer", spec.key),
                    )
                });
                let sheet = overlay::configure_sheet(sheet, &spec.node, children, footer);
                overlay::bind_sheet_callbacks(sheet, &spec.node, cmd_tx.clone())
            });
        });
    }

    fn sync_notifications(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted = self
            .tree
            .as_ref()
            .map(overlay::collect_notifications)
            .unwrap_or_default();
        let wanted_keys: HashSet<String> = wanted.iter().map(|spec| spec.key.clone()).collect();
        self.note_waiting.retain(|key| wanted_keys.contains(key));

        let stale: Vec<String> = self
            .notes
            .keys()
            .filter(|key| !wanted_keys.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(mut slot) = self.notes.remove(&key) {
                slot.suppress_close = true;
                slot.entity.update(cx, |note, cx| {
                    note.dismiss(window, cx);
                });
            }
        }

        for spec in wanted {
            if self.note_waiting.contains(&spec.key) {
                continue;
            }
            let fingerprint = overlay::notification_fingerprint(&spec.node);
            if let Some(slot) = self.notes.get_mut(&spec.key) {
                slot.on_click = spec.node.on_click.clone();
                slot.on_close = spec.node.on_close.clone();
                if slot.fingerprint == fingerprint {
                    continue;
                }
                slot.fingerprint = fingerprint.clone();
                slot.suppress_close = true;
                slot.entity.update(cx, |note, cx| note.dismiss(window, cx));
                self.notes.remove(&spec.key);
            }
            self.push_notification(spec, fingerprint, window, cx);
        }
    }

    fn push_notification(
        &mut self,
        spec: overlay::NotificationSpec,
        fingerprint: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = spec.key.clone();
        let mut note = match spec
            .node
            .variant
            .as_deref()
            .map(crate::catalog::normalize)
            .as_deref()
        {
            Some("success") => Notification::success(
                spec.node
                    .message
                    .clone()
                    .or(spec.node.text.clone())
                    .unwrap_or_default(),
            ),
            Some("warning") => Notification::warning(
                spec.node
                    .message
                    .clone()
                    .or(spec.node.text.clone())
                    .unwrap_or_default(),
            ),
            Some("error") | Some("danger") => Notification::error(
                spec.node
                    .message
                    .clone()
                    .or(spec.node.text.clone())
                    .unwrap_or_default(),
            ),
            _ => Notification::info(
                spec.node
                    .message
                    .clone()
                    .or(spec.node.text.clone())
                    .unwrap_or_default(),
            ),
        };
        if let Some(title) = spec.node.title.clone() {
            note = note.title(title);
        }
        note = note
            .id1::<CljNotification>(SharedString::from(key.clone()))
            .autohide(overlay::notification_autohide(&spec.node));
        let on_click = spec.node.on_click.clone();
        if on_click.is_some() {
            let cmd_key = key.clone();
            let weak = cx.weak_entity();
            note = note.on_click(move |_, _, app| {
                let _ = weak.update(app, |this, _cx| {
                    let ids: HashMap<String, Option<String>> = this
                        .notes
                        .iter()
                        .map(|(k, slot)| (k.clone(), slot.on_click.clone()))
                        .collect();
                    if let Some(id) = overlay::live_notification_click(&ids, &cmd_key) {
                        let _ = this.cmd_tx.send(Cmd::Callback {
                            id,
                            value: None,
                            seq: None,
                        });
                    }
                });
            });
        }
        window.push_notification(note, cx);
        let entity = window
            .notifications(cx)
            .last()
            .cloned()
            .expect("notification just pushed");
        let key_owned = key.clone();
        cx.subscribe(&entity, move |this, _, _: &DismissEvent, _cx| {
            let Some(slot) = this.notes.remove(&key_owned) else {
                return;
            };
            this.note_waiting.insert(key_owned.clone());
            if slot.suppress_close {
                return;
            }
            if let Some(id) = slot.on_close {
                let _ = this.cmd_tx.send(Cmd::Callback {
                    id,
                    value: None,
                    seq: None,
                });
            }
        })
        .detach();
        self.notes.insert(
            key,
            NotificationSlot {
                entity,
                fingerprint,
                on_click,
                on_close: spec.node.on_close.clone(),
                suppress_close: false,
            },
        );
    }

    fn render_number_input(
        &mut self,
        node: &Node,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.number_slot(key, node, window, cx);
        let mut input = NumberInput::new(&state);
        if let Some(placeholder) = node.placeholder.clone() {
            input = input.placeholder(placeholder);
        }
        if node.disabled {
            input = input.disabled(true);
        }
        apply_style(
            input.with_size(mapping::parse_scale(node.control_size.as_deref())),
            node,
            cx,
        )
        .into_any_element()
    }

    fn number_slot(
        &mut self,
        key: &str,
        node: &Node,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let wanted = node
            .number_value()
            .map(|n| n.to_string())
            .or_else(|| node.string_value())
            .or_else(|| node.text.clone())
            .unwrap_or_default();
        let mut sync_node = node.clone();
        sync_node.text = Some(wanted);
        let state = self.input_slot(key, &sync_node, window, cx);
        let need_step = if let Some(slot) = self.inputs.get_mut(key) {
            slot.as_number = true;
            slot.number_min = node.min;
            slot.number_max = node.max;
            slot.number_step = node.step;
            let need = !slot.number_stepped;
            if need {
                slot.number_stepped = true;
            }
            need
        } else {
            false
        };
        if need_step {
            let key_owned = key.to_string();
            cx.subscribe_in(
                &state,
                window,
                move |this, input, event: &NumberInputEvent, window, cx| {
                    let NumberInputEvent::Step(action) = event;
                    let Some(slot) = this.inputs.get(&key_owned) else {
                        return;
                    };
                    if !slot.as_number {
                        return;
                    }
                    let mut bounds = Node::default();
                    bounds.min = slot.number_min;
                    bounds.max = slot.number_max;
                    bounds.step = slot.number_step;
                    let current = extra::number_from_input(&input.read(cx).value()).unwrap_or(0.0);
                    let next = extra::apply_number_step(
                        current,
                        matches!(action, StepAction::Increment),
                        &bounds,
                    );
                    input.update(cx, |state, cx| {
                        state.set_value(next.to_string(), window, cx);
                    });
                },
            )
            .detach();
        }
        state
    }

    fn render_otp_input(
        &mut self,
        node: &Node,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.otp_slot(key, node, window, cx);
        let mut input =
            OtpInput::new(&state).with_size(mapping::parse_scale(node.control_size.as_deref()));
        if node.disabled {
            input = input.disabled(true);
        }
        style_host(input, node, cx)
    }

    fn otp_slot(
        &mut self,
        key: &str,
        node: &Node,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<OtpState> {
        self.used_otps.insert(key.to_string());
        let length = extra::otp_length(node);
        let wanted = node
            .string_value()
            .or_else(|| node.text.clone())
            .unwrap_or_default();
        if let Some(slot) = self.otps.get_mut(key) {
            slot.on_change = node.on_change.clone();
            slot.on_blur = node.on_blur.clone();
            let state = slot.state.clone();
            if slot.length == length {
                let focused = state.read(cx).focus_handle(cx).is_focused(window);
                let current = state.read(cx).value().to_string();
                if current != wanted && !focused {
                    state.update(cx, |otp, cx| otp.set_value(wanted, window, cx));
                }
                state.update(cx, |otp, cx| otp.set_masked(node.masked, window, cx));
                return state;
            }
        }
        let masked = node.masked;
        let default = wanted.clone();
        let state = cx.new(|cx| {
            OtpState::new(length, window, cx)
                .default_value(default)
                .masked(masked)
        });
        let key_owned = key.to_string();
        cx.subscribe(&state, move |this, otp, event: &OtpEvent, cx| match event {
            OtpEvent::Complete => {
                if let Some(id) = this.otps.get(&key_owned).and_then(|s| s.on_change.clone()) {
                    let value = otp.read(cx).value().to_string();
                    this.emit_value(id, json!(value));
                }
            }
            OtpEvent::Blur => {
                if let Some(id) = this.otps.get(&key_owned).and_then(|s| s.on_blur.clone()) {
                    let value = otp.read(cx).value().to_string();
                    this.emit_value(id, json!(value));
                }
            }
            _ => {}
        })
        .detach();
        self.otps.insert(
            key.to_string(),
            OtpSlot {
                state: state.clone(),
                length,
                on_change: node.on_change.clone(),
                on_blur: node.on_blur.clone(),
            },
        );
        state
    }

    fn render_color_picker(
        &mut self,
        node: &Node,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.color_slot(key, node, window, cx);
        let mut picker = ColorPicker::new(&state);
        if let Some(label) = node.text.clone().or(node.title.clone()) {
            picker = picker.label(label);
        }
        apply_style(
            picker.with_size(mapping::parse_scale(node.control_size.as_deref())),
            node,
            cx,
        )
        .into_any_element()
    }

    fn color_slot(
        &mut self,
        key: &str,
        node: &Node,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ColorPickerState> {
        self.used_colors.insert(key.to_string());
        let wanted = extra::color_from_node(node);
        let recreate = self.colors.get(key).is_some_and(|slot| {
            extra::color_sync(wanted, slot.state.read(cx).value())
                == extra::ColorSync::RecreateClear
        });
        if recreate {
            self.colors.remove(key);
        } else if let Some(slot) = self.colors.get_mut(key) {
            slot.on_change = node.on_change.clone();
            let state = slot.state.clone();
            if extra::color_sync(wanted, state.read(cx).value()) == extra::ColorSync::Set {
                if let Some(color) = wanted {
                    state.update(cx, |picker, cx| picker.set_value(color, window, cx));
                }
            }
            return state;
        }
        let state = cx.new(|cx| {
            let mut picker = ColorPickerState::new(window, cx);
            if let Some(color) = wanted {
                picker = picker.default_value(color);
            }
            picker
        });
        let key_owned = key.to_string();
        cx.subscribe(&state, move |this, _, event: &ColorPickerEvent, _cx| {
            let ColorPickerEvent::Change(color) = event;
            if let Some(id) = this
                .colors
                .get(&key_owned)
                .and_then(|s| s.on_change.clone())
            {
                let value = extra::color_event_payload(*color);
                this.emit_value(id, value);
            }
        })
        .detach();
        self.colors.insert(
            key.to_string(),
            ColorSlot {
                state: state.clone(),
                on_change: node.on_change.clone(),
            },
        );
        state
    }

    fn render_date_picker(
        &mut self,
        node: &Node,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.date_slot(key, node, window, cx);
        let mut picker = DatePicker::new(&state).cleanable(true);
        if let Some(placeholder) = node.placeholder.clone() {
            picker = picker.placeholder(placeholder);
        }
        if node.disabled {
            picker = picker.disabled(true);
        }
        apply_style(
            picker.with_size(mapping::parse_scale(node.control_size.as_deref())),
            node,
            cx,
        )
        .into_any_element()
    }

    fn date_slot(
        &mut self,
        key: &str,
        node: &Node,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<DatePickerState> {
        self.used_dates.insert(key.to_string());
        let range = node.range || node.multiple;
        let wanted = extra::date_from_value(&node.value, range);
        if let Some(slot) = self.dates.get_mut(key) {
            slot.on_change = node.on_change.clone();
            let state = slot.state.clone();
            if slot.range == range {
                let current = state.read(cx).date();
                if current != wanted {
                    state.update(cx, |picker, cx| picker.set_date(wanted, window, cx));
                }
                return state;
            }
        }
        let state = cx.new(|cx| {
            let mut picker = if range {
                DatePickerState::range(window, cx)
            } else {
                DatePickerState::new(window, cx)
            };
            picker = picker.date_format("%Y-%m-%d");
            picker
        });
        state.update(cx, |picker, cx| picker.set_date(wanted, window, cx));
        let key_owned = key.to_string();
        cx.subscribe(&state, move |this, _, event: &DatePickerEvent, _cx| {
            let DatePickerEvent::Change(date) = event;
            if let Some(id) = this.dates.get(&key_owned).and_then(|s| s.on_change.clone()) {
                this.emit_value(id, extra::date_to_value(*date));
            }
        })
        .detach();
        self.dates.insert(
            key.to_string(),
            DateSlot {
                state: state.clone(),
                range,
                on_change: node.on_change.clone(),
            },
        );
        state
    }

    fn render_editor(
        &mut self,
        node: &Node,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.editor_slot(key, node, window, cx);
        // Code editor Input is multi-line but `h_auto` without an explicit
        // height collapses to a single row. Fill the viewport wrapper.
        viewport_sized(Editor::new(&state).h_full(), node, 200.0, cx)
    }

    fn editor_slot(
        &mut self,
        key: &str,
        node: &Node,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<EditorState> {
        self.used_editors.insert(key.to_string());
        let language = extra::editor_language(node);
        let wanted = node.text.clone().unwrap_or_default();
        if let Some(slot) = self.editors.get_mut(key) {
            let id_changed = slot.on_change != node.on_change;
            slot.on_change = node.on_change.clone();
            slot.on_submit = node.on_submit.clone();
            slot.on_blur = node.on_blur.clone();
            slot.on_escape = node.on_escape.clone();
            let refresh = id_changed && slot.change.on_ids_refreshed();
            let state = slot.state.clone();
            let focused = state.read(cx).focus_handle(cx).is_focused(window);
            let current = state.read(cx).value().to_string();
            if current != wanted && !focused {
                state.update(cx, |input, cx| input.set_value(wanted, window, cx));
            }
            let lang = language.clone();
            state.update(cx, |input, cx| input.set_highlighter(lang, cx));
            if refresh {
                Self::schedule_input_change_flush(key.to_string(), TextFlush::Editor, window, cx);
            }
            return state;
        }
        let placeholder = node.placeholder.clone().unwrap_or_default();
        let state = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(language)
                .placeholder(placeholder)
                .default_value(wanted)
        });
        self.editors.insert(
            key.to_string(),
            TextControlSlot {
                state: state.clone(),
                on_change: node.on_change.clone(),
                on_submit: node.on_submit.clone(),
                on_blur: node.on_blur.clone(),
                on_escape: node.on_escape.clone(),
                wait_for_seq: None,
                submitted: None,
                change: protocol::InputChangeCoalesce::default(),
            },
        );
        let key_owned = key.to_string();
        cx.subscribe_in(
            &state,
            window,
            move |this, input, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let Some(slot) = this.editors.get_mut(&key_owned) else {
                        return;
                    };
                    let value = input.read(cx).value().to_string();
                    if slot.change.on_change(value) {
                        Self::schedule_input_change_flush(
                            key_owned.clone(),
                            TextFlush::Editor,
                            window,
                            cx,
                        );
                    }
                }
                InputEvent::Blur => {
                    if let Some(id) = this.editors.get(&key_owned).and_then(|s| s.on_blur.clone()) {
                        let value = input.read(cx).value().to_string();
                        this.emit_value(id, json!(value));
                    }
                }
                _ => {}
            },
        )
        .detach();
        state
    }

    fn textarea_slot(
        &mut self,
        key: &str,
        node: &Node,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextareaState> {
        self.used_textareas.insert(key.to_string());
        let wanted = node.text.clone().unwrap_or_default();
        let rows = node.rows.unwrap_or(3).max(1) as usize;
        let submit_on_enter = extra::textarea_submit_on_enter(node.on_submit.as_deref());
        if let Some(slot) = self.textareas.get_mut(key) {
            let id_changed = slot.on_change != node.on_change;
            slot.on_change = node.on_change.clone();
            slot.on_submit = node.on_submit.clone();
            slot.on_blur = node.on_blur.clone();
            slot.on_escape = node.on_escape.clone();
            let refresh = id_changed && slot.change.on_ids_refreshed();
            let state = slot.state.clone();
            let force = matches!(
                (slot.wait_for_seq, self.tree_seq),
                (Some(wait), Some(seq)) if wait == seq
            );
            let focused = state.read(cx).focus_handle(cx).is_focused(window);
            let current = state.read(cx).value().to_string();
            if current != wanted && (force || (!focused && slot.wait_for_seq.is_none())) {
                let wanted = wanted.clone();
                state.update(cx, |input, cx| input.set_value(wanted, window, cx));
            }
            if force {
                slot.wait_for_seq = None;
            }
            if let Some(placeholder) = node.placeholder.clone() {
                state.update(cx, |input, cx| {
                    input.set_placeholder(placeholder, window, cx);
                });
            }
            state.update(cx, |input, cx| {
                input.set_rows(rows, cx);
                input.set_submit_on_enter(submit_on_enter, cx);
            });
            if node.focus && !state.read(cx).focus_handle(cx).is_focused(window) {
                state.read(cx).focus_handle(cx).focus(window, cx);
            }
            if refresh {
                Self::schedule_input_change_flush(key.to_string(), TextFlush::Textarea, window, cx);
            }
            return state;
        }
        let placeholder = node.placeholder.clone().unwrap_or_default();
        let want_focus = node.focus;
        let state = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(placeholder)
                .default_value(wanted)
                .rows(rows)
                .submit_on_enter(submit_on_enter)
        });
        if want_focus {
            state.read(cx).focus_handle(cx).focus(window, cx);
        }
        self.textareas.insert(
            key.to_string(),
            TextControlSlot {
                state: state.clone(),
                on_change: node.on_change.clone(),
                on_submit: node.on_submit.clone(),
                on_blur: node.on_blur.clone(),
                on_escape: node.on_escape.clone(),
                wait_for_seq: None,
                submitted: None,
                change: protocol::InputChangeCoalesce::default(),
            },
        );
        let key_owned = key.to_string();
        cx.subscribe_in(
            &state,
            window,
            move |this, input, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let Some(slot) = this.textareas.get_mut(&key_owned) else {
                        return;
                    };
                    if slot.wait_for_seq.is_some() {
                        return;
                    }
                    let value = input.read(cx).value().to_string();
                    if slot.submitted.as_ref() == Some(&value) {
                        return;
                    }
                    slot.submitted = None;
                    if slot.change.on_change(value) {
                        Self::schedule_input_change_flush(
                            key_owned.clone(),
                            TextFlush::Textarea,
                            window,
                            cx,
                        );
                    }
                }
                InputEvent::Blur => {
                    if let Some(id) = this
                        .textareas
                        .get(&key_owned)
                        .and_then(|s| s.on_blur.clone())
                    {
                        let value = input.read(cx).value().to_string();
                        this.emit_value(id, json!(value));
                    }
                }
                InputEvent::PressEnter {
                    secondary: false,
                    shift: false,
                } => {
                    let (on_submit, value, seq) = {
                        let Some(slot) = this.textareas.get_mut(&key_owned) else {
                            return;
                        };
                        let Some(id) = slot.on_submit.clone() else {
                            return;
                        };
                        this.next_submit_seq = this.next_submit_seq.saturating_add(1);
                        let seq = this.next_submit_seq;
                        let value = input.read(cx).value().to_string();
                        slot.wait_for_seq = Some(seq);
                        slot.submitted = Some(value.clone());
                        (id, value, seq)
                    };
                    let _ = this.cmd_tx.send(Cmd::Callback {
                        id: on_submit,
                        value: Some(json!(value)),
                        seq: Some(seq),
                    });
                }
                _ => {}
            },
        )
        .detach();
        state
    }

    fn apply_message_scroller_scroll(
        slot: &mut MessageScrollerSlot,
        node: &Node,
        ids: &[String],
        cx: &mut Context<Self>,
    ) {
        let request =
            chat::scroller_scroll_request(node.scroll_to_end, node.scroll_to_item.as_ref());
        let generation = chat::scroller_scroll_generation(node.scroll_generation.as_ref());
        let Some((apply, token)) = chat::scroller_scroll_plan(
            slot.last_scroll.as_ref(),
            request.as_ref(),
            generation.as_deref(),
            ids,
        ) else {
            return;
        };
        let applied = match apply {
            chat::ScrollerScrollApply::End => {
                slot.state.update(cx, |state, cx| state.scroll_to_end(cx));
                true
            }
            chat::ScrollerScrollApply::Item(index) => slot
                .state
                .update(cx, |state, cx| state.scroll_to_item(index, cx)),
        };
        if applied {
            slot.last_scroll = Some(token);
        }
    }

    fn render_message_scroller(
        &mut self,
        node: &Node,
        path: &str,
        key: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.used_scrollers.insert(key.to_string());
        let children = node.children.clone();
        let ids: Vec<String> = children
            .iter()
            .enumerate()
            .map(|(index, child)| chat::scroller_item_id(child, index))
            .collect();
        let fps: Vec<u64> = children.iter().map(chat::node_fingerprint).collect();
        if !self.scrollers.contains_key(key) {
            let item_count = children.len();
            let state = cx.new(|cx| MessageScrollerState::new(item_count, cx));
            let observe = cx.observe(&state, |_, _, cx| cx.notify());
            self.scrollers.insert(
                key.to_string(),
                MessageScrollerSlot {
                    state,
                    items: Rc::new(RefCell::new(children)),
                    last_ids: ids.clone(),
                    last_fps: fps,
                    last_scroll: None,
                    _observe: observe,
                },
            );
        } else if let Some(slot) = self.scrollers.get_mut(key) {
            match chat::scroller_edit(&slot.last_ids, &ids) {
                chat::ScrollerEdit::Leave => {
                    if chat::scroller_survivors_changed(
                        &chat::ScrollerEdit::Leave,
                        &slot.last_fps,
                        &fps,
                    ) {
                        slot.state.update(cx, |state, cx| state.remeasure(cx));
                    }
                }
                chat::ScrollerEdit::Reset { count } => {
                    slot.state.update(cx, |state, cx| state.reset(count, cx));
                }
                chat::ScrollerEdit::Append(n) => {
                    let survivor_changed = chat::scroller_survivors_changed(
                        &chat::ScrollerEdit::Append(n),
                        &slot.last_fps,
                        &fps,
                    );
                    slot.state.update(cx, |state, cx| {
                        let _ = state.append(n, cx);
                        if survivor_changed {
                            state.remeasure(cx);
                        }
                    });
                }
                chat::ScrollerEdit::Prepend(n) => {
                    let survivor_changed = chat::scroller_survivors_changed(
                        &chat::ScrollerEdit::Prepend(n),
                        &slot.last_fps,
                        &fps,
                    );
                    slot.state.update(cx, |state, cx| {
                        let _ = state.prepend(n, cx);
                        if survivor_changed {
                            state.remeasure(cx);
                        }
                    });
                }
            }
            *slot.items.borrow_mut() = children;
            slot.last_ids = ids.clone();
            slot.last_fps = fps;
        }
        if let Some(slot) = self.scrollers.get_mut(key) {
            Self::apply_message_scroller_scroll(slot, node, &ids, cx);
        }
        let slot = self.scrollers.get(key).expect("scroller slot");
        let items = slot.items.clone();
        let cmd_tx = self.cmd_tx.clone();
        let row_path = path.to_string();
        let mut scroller = MessageScroller::new(
            SharedString::from(key.to_string()),
            slot.state.clone(),
            move |index, _, _| {
                let tree = items.borrow();
                match tree.get(index) {
                    Some(row) => {
                        overlay::paint_scroller_tree(row, &format!("{row_path}.{index}"), &cmd_tx)
                    }
                    None => div().into_any_element(),
                }
            },
        );
        if node.scrollbar == Some(false) {
            scroller = scroller.scrollbar(false);
        }
        if node.jump_button == Some(false) {
            scroller = scroller.jump_button(false);
        }
        if let Some(label) = node.jump_button_label.clone() {
            scroller = scroller.with_jump_button_label(label);
        }
        if let Some(secs) = node
            .jump_button_transition
            .filter(|n| n.is_finite() && *n >= 0.0)
        {
            scroller = scroller.with_jump_button_transition(Duration::from_secs_f32(secs));
        }
        if let Some(fade) = node.bottom_fade.as_deref().and_then(extra::parse_hex_color) {
            scroller = scroller.with_bottom_fade(fade);
        }
        if let Some(style) = mapping::style_refinement(node.content_style.as_deref()) {
            scroller = scroller.with_content_style(style);
        }
        if let Some(style) = mapping::style_refinement(node.list_style.as_deref()) {
            scroller = scroller.with_list_style(style);
        }
        if let Some(style) = mapping::style_refinement(node.row_style.as_deref()) {
            scroller = scroller.with_row_style(style);
        }
        if let Some(style) = mapping::style_refinement(node.jump_button_style.as_deref()) {
            scroller = scroller.with_jump_button_style(style);
        }
        if let Some(chrome) = node.jump_button_renderer.clone() {
            scroller = scroller.with_jump_button_renderer(move |button| {
                mapping::apply_jump_button_renderer(button, &chrome)
            });
        }
        // Kit's root Styled (padding, gap, font/color, bg, border, shadow,
        // align/justify) lives on MessageScroller. The host wrapper only
        // supplies viewport/box geometry so content/list/row slots stay
        // distinct from the scroller root.
        let scroller = apply_kit_visual_style(scroller, node, cx);
        viewport_box_sized(scroller, node, 400.0)
    }

    fn render_virtual_list(
        &mut self,
        node: &Node,
        key: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.used_vlists.insert(key.to_string());
        if let Some(entity) = self.vlists.get(key) {
            entity.update(cx, |list, cx| {
                list.sync_from_node(node, self.cmd_tx.clone());
                cx.notify();
            });
            return viewport_sized(self.vlists[key].clone(), node, 200.0, cx);
        }
        let view = extra::VirtualListView::from_node(node, self.cmd_tx.clone());
        let entity = cx.new(|_| view);
        self.vlists.insert(key.to_string(), entity.clone());
        viewport_sized(entity, node, 200.0, cx)
    }

    fn render_sidebar(&self, node: &Node, key: &str, cx: &App) -> AnyElement {
        let collapsed = node.collapsed;
        let selected = node.string_value();
        let cmd_tx = self.cmd_tx.clone();
        let on_change = node.on_change.clone();
        let items: Vec<SidebarMenuItem> = node
            .collection()
            .iter()
            .map(|item| {
                let id = item.id_or_label();
                let mut row = SidebarMenuItem::new(item.label_or_id())
                    .active(selected.as_deref() == Some(id.as_str()))
                    .collapsed(collapsed);
                if let Some(icon) = item.icon.as_deref().and_then(mapping::parse_icon) {
                    row = row.icon(icon);
                }
                let cmd_tx = cmd_tx.clone();
                let on_change = on_change.clone();
                if item.disabled {
                    row
                } else {
                    row.on_click(move |_, _, _| {
                        if let Some(callback) = on_change.clone() {
                            protocol::send_callbacks(
                                &cmd_tx,
                                vec![protocol::CallbackCall::with_value(
                                    callback,
                                    json!(id.clone()),
                                )],
                            );
                        }
                    })
                }
            })
            .collect();
        let mut sidebar = Sidebar::new(key.to_string()).side(extra::parse_sidebar_side(node));
        sidebar = sidebar
            .collapsed(collapsed)
            .child(SidebarMenu::new().children(items));
        if let Some(title) = sidebar_header_title(node) {
            sidebar = sidebar.header(div().px_2().py_1().child(title));
        }
        viewport_sized(sidebar, node, 280.0, cx)
    }

    fn render_dock(
        &mut self,
        node: &Node,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.used_docks.insert(key.to_string());
        let fingerprint = node
            .collection()
            .iter()
            .map(|item| format!("{}:{}", extra::dock_side(item), item.id_or_label()))
            .collect::<Vec<_>>()
            .join("|");
        if let Some(slot) = self.docks.get_mut(key) {
            if slot.fingerprint == fingerprint {
                for item in node.collection() {
                    let id = item.id_or_label();
                    if let Some(panel) = slot.panels.get(&id) {
                        let content =
                            item.content
                                .as_ref()
                                .map(|n| *n.clone())
                                .unwrap_or_else(|| Node {
                                    kind: "label".into(),
                                    text: item.label.clone(),
                                    ..Node::default()
                                });
                        panel.update(cx, |p, cx| {
                            p.title = item.label_or_id().into();
                            *p.live.borrow_mut() = content;
                            cx.notify();
                        });
                    }
                }
                return viewport_sized(slot.area.clone(), node, 360.0, cx);
            }
        }
        let (area, _skin) =
            DockSkin::dock_area(SharedString::from(key.to_string()), None, window, cx);
        let mut panels: HashMap<String, Entity<extra::CljPanel>> = HashMap::new();
        let mut by_side: HashMap<&str, Vec<std::sync::Arc<dyn gpui::base::dock::PanelView>>> =
            HashMap::new();
        for (ix, item) in node.collection().iter().enumerate() {
            let id = item.id_or_label();
            let content = item
                .content
                .as_ref()
                .map(|n| *n.clone())
                .unwrap_or_else(|| Node {
                    kind: "label".into(),
                    text: item.label.clone(),
                    ..Node::default()
                });
            let live = Rc::new(RefCell::new(content));
            let path = format!("{key}/panel/{ix}");
            let title = item.label_or_id();
            let emit = Self::action_emitter(cx);
            let panel = cx
                .new(|cx| extra::CljPanel::new(title.clone(), live, path, emit, cx.focus_handle()));
            let side = extra::dock_side(item);
            by_side
                .entry(side)
                .or_default()
                .push(panel_handle(panel.clone()));
            panels.insert(id, panel);
        }
        area.update(cx, |dock, cx| {
            if let Some(center) = by_side.remove("center") {
                if !center.is_empty() {
                    dock.set_center(dock_tabs(center, cx), window, cx);
                }
            }
            if let Some(left) = by_side.remove("left") {
                if !left.is_empty() {
                    dock.set_dock(DockPlacement::Left, dock_tabs(left, cx), window, cx);
                    let size = node.width.unwrap_or(240.0);
                    dock.set_dock_size(DockPlacement::Left, px(size), window, cx);
                }
            }
            if let Some(right) = by_side.remove("right") {
                if !right.is_empty() {
                    dock.set_dock(DockPlacement::Right, dock_tabs(right, cx), window, cx);
                    dock.set_dock_size(DockPlacement::Right, px(240.), window, cx);
                }
            }
            if let Some(bottom) = by_side.remove("bottom") {
                if !bottom.is_empty() {
                    let bottom_h = node
                        .height
                        .map(|h| (h * 0.34).clamp(64.0, 140.0))
                        .unwrap_or(96.0);
                    dock.set_dock(DockPlacement::Bottom, dock_tabs(bottom, cx), window, cx);
                    dock.set_dock_size(DockPlacement::Bottom, px(bottom_h), window, cx);
                }
            }
        });
        self.docks.insert(
            key.to_string(),
            DockSlot {
                area: area.clone(),
                fingerprint,
                panels,
            },
        );
        viewport_sized(area, node, 360.0, cx)
    }

    fn render_nav_stack(
        &mut self,
        node: &Node,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.used_nav_stacks.insert(key.to_string());
        let cmd_tx = self.cmd_tx.clone();
        if !self.nav_stacks.contains_key(key) {
            let state = cx.new(|_| gpui::base::NavStackState::new());
            let observe = cx.observe(&state, |_, _, cx| cx.notify());
            self.nav_stacks.insert(
                key.to_string(),
                NavStackSlot {
                    state,
                    entries: Vec::new(),
                    forward: Vec::new(),
                    last_invalid: None,
                    last_dup_catalog: None,
                    last_forward_notified: None,
                    last_replace: None,
                    last_item_warn: None,
                    _observe: observe,
                },
            );
        }
        let catalog = extra::nav_catalog(node);
        let catalog_ids: Vec<String> = catalog.iter().map(|(id, _)| id.clone()).collect();
        let catalog_by_id: HashMap<String, Node> = catalog.into_iter().collect();
        let motion = extra::nav_motion(node.duration, node.motion.as_deref());
        if let Some(slot) = self.nav_stacks.get_mut(key) {
            let dups = extra::nav_duplicate_catalog_ids(catalog_ids.iter().map(String::as_str));
            if dups.is_empty() {
                slot.last_dup_catalog = None;
            } else if slot.last_dup_catalog.as_ref() != Some(&dups) {
                eprintln!(
                    "[host] nav-stack: duplicate catalog page id(s) {dups:?}; lookup uses the last template"
                );
                slot.last_dup_catalog = Some(dups);
            }
            let token = extra::nav_replace_token(node.replace_generation.as_ref());
            let mut replacing = None;
            let steps = match extra::nav_desired(node, &catalog_ids) {
                extra::NavDesired::Invalid { trail, unknown } => {
                    if slot.last_invalid.as_ref() != Some(&trail) {
                        eprintln!(
                            "[host] nav-stack: ignoring controlled trail {trail:?}; unknown page id(s) {unknown:?}"
                        );
                        slot.last_invalid = Some(trail);
                    }
                    Vec::new()
                }
                desired => {
                    slot.last_invalid = None;
                    let resolved = desired.ids(&catalog_ids).expect("valid nav trail");
                    let current: Vec<String> =
                        slot.entries.iter().map(|(id, _)| id.clone()).collect();
                    let forward: Vec<String> =
                        slot.forward.iter().map(|(id, _)| id.clone()).collect();
                    let before = slot.entries.last().map(|(_, page)| page.entity_id());
                    let mut steps = extra::nav_trail_sync(
                        &current,
                        &resolved,
                        &forward,
                        extra::nav_reuse_forward(node.reuse_forward),
                    );
                    let replace = extra::nav_same_id_replace(
                        &current,
                        &resolved,
                        before,
                        slot.last_replace.as_ref(),
                        token.as_deref(),
                    );
                    if replace {
                        if let Some(id) = resolved.last() {
                            steps = vec![extra::NavTrailStep::Replace(id.clone())];
                        }
                    }
                    replacing = Some(replace);
                    steps
                }
            };
            let skip_current_live = steps
                .iter()
                .any(|step| matches!(step, extra::NavTrailStep::Replace(_)));
            let current_last = slot.entries.len().saturating_sub(1);
            for (i, (id, entity)) in slot.entries.iter().enumerate() {
                if skip_current_live && i == current_last {
                    continue;
                }
                if let Some(page_node) = catalog_by_id.get(id) {
                    entity.update(cx, |page, cx| {
                        page.replace_live(page_node.clone(), cx);
                    });
                }
            }
            for (id, entity) in slot.forward.iter() {
                if let Some(page_node) = catalog_by_id.get(id) {
                    entity.update(cx, |page, cx| {
                        page.replace_live(page_node.clone(), cx);
                    });
                }
            }
            apply_nav_trail_plan(slot, steps, motion, &catalog_by_id, key, &cmd_tx, cx);
            if let Some(replaced) = replacing {
                slot.last_replace = extra::nav_commit_replace_binding(
                    replaced,
                    slot.entries.last().map(|(_, page)| page.entity_id()),
                    token.as_deref(),
                    slot.last_replace.clone(),
                );
            }
            notify_nav_forward_change(slot, node.on_forward_change.clone(), &cmd_tx, window, cx);
            if let Some(reason) = extra::nav_item_reject_reason(node.item.as_ref()) {
                if slot.last_item_warn.as_ref() != Some(&reason) {
                    eprintln!("[host] nav-stack: ignoring item; {reason}");
                    slot.last_item_warn = Some(reason);
                }
            } else {
                slot.last_item_warn = None;
            }
        }
        let slot = self.nav_stacks.get(key).expect("nav-stack slot");
        let mut nav = gpui::base::NavStack::new(&slot.state).w_full().h_full();
        if extra::nav_clip(node.overflow.as_deref(), node.overflow_hidden) {
            nav = nav.overflow_hidden();
        }
        if let Some(secs) = extra::nav_transition_secs(node.duration) {
            nav = nav.transition(gpui::base::motion::Transition::new(
                Duration::from_secs_f32(secs),
            ));
        }
        if let Some(spec) = extra::nav_item_spec(node) {
            nav = nav.item(move |page, _, _| extra::nav_stack_item(page, &spec));
        }
        let nav = apply_kit_visual_style(nav, node, cx);
        viewport_box_sized(nav, node, 200.0)
    }

    fn render_resizable(
        &mut self,
        node: &Node,
        path: &str,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.used_resizables.insert(key.to_string());
        let vertical = mapping::parse_axis(node.orientation.as_deref()) == Axis::Vertical;
        let state = self
            .resizables
            .entry(key.to_string())
            .or_insert_with(|| cx.new(|_| ResizableState::default()))
            .clone();
        if let Some(id) = node.on_change.clone() {
            let cmd_tx = self.cmd_tx.clone();
            let _ = id;
            let _ = cmd_tx;
        }
        let mut group = if vertical {
            v_resizable(eid(key))
        } else {
            h_resizable(eid(key))
        };
        group = group.with_state(&state);
        if let Some(on_change) = node.on_change.clone() {
            let cmd_tx = self.cmd_tx.clone();
            group = group.on_resize(move |state, _, cx| {
                let sizes: Vec<f32> = state
                    .read(cx)
                    .sizes()
                    .iter()
                    .map(|p| f32::from(*p))
                    .collect();
                protocol::send_callbacks(
                    &cmd_tx,
                    vec![protocol::CallbackCall::with_value(
                        on_change.clone(),
                        json!(sizes),
                    )],
                );
            });
        }
        for (index, child) in node.children.iter().enumerate() {
            let painted = self.render_node(child, &format!("{path}-{index}"), window, cx);
            let mut panel = resizable_panel().child(painted);
            if let Some(size) = child.width.or(child.height).or(child.size) {
                panel = panel.size(px(size));
            }
            group = group.child(panel);
        }
        viewport_sized(group, node, 240.0, cx)
    }

    fn render_scroll(
        &mut self,
        node: &Node,
        path: &str,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // gpui-component's overflow_y_scrollbar wraps in size_full() (height
        // 100%). In a column with siblings that makes the scroller as tall as
        // the parent instead of the leftover space, so the window grows and
        // the list never scrolls. Bound the wrapper; the inner size_full then
        // fills that viewport.
        //
        // Viewport size (`:width` / `:height` / `:size` / leftover flex) lives
        // on the wrapper. Visual styles stay on the inner body so they are not
        // applied twice and so an explicit width is not swallowed by w_full().
        let viewport = scroll_viewport(node);
        let mut wrap = v_flex().id(eid(key)).min_h_0().overflow_hidden();
        wrap = match viewport.width {
            ScrollExtent::Px(width) => wrap.w(px(width)),
            ScrollExtent::Fill => wrap.w_full(),
        };
        wrap = match viewport.height {
            ScrollExtent::Px(height) => wrap.h(px(height)),
            ScrollExtent::Fill => wrap.flex_1(),
        };
        let mut inner = node.clone();
        inner.height = None;
        inner.width = None;
        inner.size = None;
        inner.flex = None;
        wrap.child(
            apply_style(v_flex().id(eid(&format!("{key}-body"))), &inner, cx)
                .overflow_y_scrollbar()
                .children(self.render_children(node, path, window, cx)),
        )
        .into_any_element()
    }

    fn render_children(
        &mut self,
        node: &Node,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        node.children
            .clone()
            .into_iter()
            .enumerate()
            .filter_map(|(index, child)| {
                if child.kind == "dialog"
                    || child.kind == "sheet"
                    || child.kind == "notification"
                    || child.kind == "native-menu"
                {
                    None
                } else {
                    Some(self.render_node(&child, &format!("{path}-{index}"), window, cx))
                }
            })
            .collect()
    }

    fn render_circle_checkbox(&self, node: &Node, key: &str, cx: &App) -> AnyElement {
        let checked = node.checked.unwrap_or(false);
        let diameter = node.size.unwrap_or(30.0);
        let theme = cx.theme();
        let ring = if checked { theme.primary } else { theme.border };
        let mut mark = div()
            .id(eid(&format!("{key}-mark")))
            .flex()
            .items_center()
            .justify_center()
            .size(px(diameter))
            .rounded_full()
            .border_1()
            .border_color(ring)
            .cursor_pointer();
        if checked {
            mark = mark.child(
                div()
                    .text_size(px(16.))
                    .text_color(theme.primary)
                    .child("✓"),
            );
        }
        if let Some(callback_id) = node.on_click.clone() {
            mark = mark.on_click(self.click(callback_id));
        }
        let mut row = h_flex().id(eid(key)).items_center().gap(px(12.));
        row = row.child(mark);
        if let Some(text) = node.text.clone() {
            row = row.child(div().child(text));
        }
        let mut style = node.clone();
        style.size = None;
        apply_style(row, &style, cx).into_any_element()
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_theme(window, cx);
        self.apply_chrome(window);
        self.native_window_id = preview::native_window_id(window);
        self.used_inputs.clear();
        self.used_selects.clear();
        self.used_comboboxes.clear();
        self.used_commands.clear();
        self.used_lists.clear();
        self.used_tables.clear();
        self.used_trees.clear();
        self.used_otps.clear();
        self.used_colors.clear();
        self.used_dates.clear();
        self.used_editors.clear();
        self.used_textareas.clear();
        self.used_vlists.clear();
        self.used_scrollers.clear();
        self.used_docks.clear();
        self.used_nav_stacks.clear();
        self.used_resizables.clear();
        let tree = self.tree.clone();
        let error = self.error.clone();

        let body = if let Some(error) = error {
            v_flex()
                .gap_2()
                .child(div().text_color(cx.theme().danger).child("Clojure error"))
                .child(div().text_color(cx.theme().foreground).child(error))
                .into_any_element()
        } else if let Some(tree) = tree.as_ref() {
            self.render_node(tree, "root", window, cx)
        } else {
            div()
                .text_color(cx.theme().muted_foreground)
                .child("Waiting for Clojure to render…")
                .into_any_element()
        };

        let used = std::mem::take(&mut self.used_inputs);
        self.inputs.retain(|key, _| used.contains(key));
        // SliderState stores the last laid-out bar size. Dropping it on tab
        // switch recreates an entity whose bounds are 0, so the fill paints
        // at 100% until the mouse moves. Bounds are crate-private, so slots
        // stay for the window lifetime (see docs/gpui-component.md).
        let used_selects = std::mem::take(&mut self.used_selects);
        self.selects.retain(|key, _| used_selects.contains(key));
        let used_comboboxes = std::mem::take(&mut self.used_comboboxes);
        self.comboboxes
            .retain(|key, _| used_comboboxes.contains(key));
        let used_commands = std::mem::take(&mut self.used_commands);
        self.commands.retain(|key, _| used_commands.contains(key));
        let used_lists = std::mem::take(&mut self.used_lists);
        self.lists.retain(|key, _| used_lists.contains(key));
        let used_tables = std::mem::take(&mut self.used_tables);
        self.tables.retain(|key, _| used_tables.contains(key));
        let used_trees = std::mem::take(&mut self.used_trees);
        self.trees.retain(|key, _| used_trees.contains(key));
        let used_otps = std::mem::take(&mut self.used_otps);
        self.otps.retain(|key, _| used_otps.contains(key));
        let used_colors = std::mem::take(&mut self.used_colors);
        self.colors.retain(|key, _| used_colors.contains(key));
        let used_dates = std::mem::take(&mut self.used_dates);
        self.dates.retain(|key, _| used_dates.contains(key));
        let used_editors = std::mem::take(&mut self.used_editors);
        self.editors.retain(|key, _| used_editors.contains(key));
        let used_textareas = std::mem::take(&mut self.used_textareas);
        self.textareas.retain(|key, _| used_textareas.contains(key));
        let used_vlists = std::mem::take(&mut self.used_vlists);
        self.vlists.retain(|key, _| used_vlists.contains(key));
        let used_scrollers = std::mem::take(&mut self.used_scrollers);
        self.scrollers.retain(|key, _| used_scrollers.contains(key));
        let used_docks = std::mem::take(&mut self.used_docks);
        self.docks.retain(|key, _| used_docks.contains(key));
        let used_nav_stacks = std::mem::take(&mut self.used_nav_stacks);
        self.nav_stacks
            .retain(|key, _| used_nav_stacks.contains(key));
        let used_resizables = std::mem::take(&mut self.used_resizables);
        self.resizables
            .retain(|key, _| used_resizables.contains(key));

        self.sync_dialogs(window, cx);
        self.sync_sheet(window, cx);
        self.sync_notifications(window, cx);
        self.sync_native_menus(window, cx);

        let dialog_layer = Root::render_dialog_layer(window, cx);
        let sheet_layer = Root::render_sheet_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        let show_footer = self.show_dev_chrome();
        let status = self.status.clone();

        v_flex()
            .size_full()
            .relative()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(v_flex().flex_1().min_h_0().child(body))
            .when(show_footer, |el| {
                el.child(
                    div()
                        .px_4()
                        .py_2()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .text_color(cx.theme().muted_foreground)
                        .child(status),
                )
                .child(gpui_fps::fps_monitor(window, cx))
            })
            // gpui-component 0.5.1 Root::render does not paint this layer.
            .children(dialog_layer)
            .children(sheet_layer)
            .children(notification_layer)
    }
}

fn widget_key(node: &Node, path: &str) -> String {
    node.id
        .as_deref()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.to_string())
}

#[cfg(test)]
mod widget_key_tests {
    use super::*;

    #[test]
    fn prefers_non_empty_node_id() {
        let named = Node {
            kind: "progress-circle".into(),
            id: Some("upload".into()),
            ..Node::default()
        };
        assert_eq!(widget_key(&named, "root-0-1"), "upload");
        let empty = Node {
            kind: "progress-circle".into(),
            id: Some(String::new()),
            ..Node::default()
        };
        assert_eq!(widget_key(&empty, "root-0-1"), "root-0-1");
        assert_eq!(
            widget_key(
                &Node {
                    kind: "progress-circle".into(),
                    ..Node::default()
                },
                "root-0-1"
            ),
            "root-0-1"
        );
    }
}

/// Step is drag granularity. Clojure's controlled value is accepted as-is
/// (then clamped to min/max). Compare f32 values exactly so a tiny-range
/// slider (e.g. 0 → 5e-5 with max 1e-4) is not discarded. `set_value`
/// notifies without emitting `SliderEvent::Change` or `Release`, so an
/// unchanged tree cannot loop.
fn slider_range(min: Option<f32>, max: Option<f32>) -> (f32, f32) {
    let min = min.unwrap_or(0.0);
    let max = max.unwrap_or(100.0);
    (min.min(max), min.max(max))
}

fn slider_step(step: Option<f32>) -> f32 {
    if step.unwrap_or(1.0) <= 0.0 {
        1.0
    } else {
        step.unwrap_or(1.0)
    }
}

fn slider_controlled_value(raw: Option<f32>, min: f32, max: f32) -> f32 {
    raw.unwrap_or(min).clamp(min, max)
}

fn slider_json_number(value: &Value) -> Option<f32> {
    match value {
        Value::Number(n) => n.as_f64().map(|n| n as f32),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn slider_range_thumbs(start: f32, end: f32, min: f32, max: f32) -> SliderValue {
    let start = start.clamp(min, max);
    let end = end.clamp(min, max);
    if start <= end {
        SliderValue::Range(start, end)
    } else {
        SliderValue::Range(end, start)
    }
}

/// Kit panics on logarithmic scale when `min <= 0` or `min >= max`.
fn slider_effective_scale(node: &Node, min: f32, max: f32) -> SliderScale {
    let requested = mapping::parse_slider_scale(node.scale.as_deref());
    if requested.is_logarithmic() && min > 0.0 && min < max {
        SliderScale::Logarithmic
    } else {
        SliderScale::Linear
    }
}

fn slider_wanted_value(node: &Node, min: f32, max: f32) -> SliderValue {
    match &node.value {
        Some(Value::Array(items)) if items.len() >= 2 => slider_range_thumbs(
            slider_json_number(&items[0]).unwrap_or(min),
            slider_json_number(&items[1]).unwrap_or(max),
            min,
            max,
        ),
        _ if node.range => {
            let end = node.number_value().unwrap_or(max).clamp(min, max);
            SliderValue::Range(min, end.max(min))
        }
        _ => SliderValue::Single(slider_controlled_value(node.number_value(), min, max)),
    }
}

fn slider_event_payload(value: SliderValue) -> Value {
    match value {
        SliderValue::Single(v) => json!(v),
        SliderValue::Range(start, end) => json!([start, end]),
    }
}

#[cfg(test)]
fn slider_slot_callback(
    event: &SliderEvent,
    on_change: Option<&str>,
    on_release: Option<&str>,
) -> Option<(String, Value)> {
    match event {
        SliderEvent::Change(changed) => {
            on_change.map(|id| (id.to_string(), slider_event_payload(*changed)))
        }
        SliderEvent::Release(changed) => {
            on_release.map(|id| (id.to_string(), slider_event_payload(*changed)))
        }
    }
}

/// Kit panics on logarithmic scale when `min <= 0` or `min >= max`. Warn
/// once when a new slot falls back to linear instead of asserting.
fn slider_log_scale_fallback(node: &Node, min: f32, max: f32) -> bool {
    mapping::parse_slider_scale(node.scale.as_deref()).is_logarithmic()
        && slider_effective_scale(node, min, max) == SliderScale::Linear
}

fn slider_value_changed(current: SliderValue, wanted: SliderValue) -> bool {
    current != wanted
}

/// Map crate `on_toggle_click` indices to ids. HashSet iteration order is
/// not stable, so multiple open ids follow original item order.
fn accordion_callback_value(ids: &[String], open_ixs: &[usize], multiple: bool) -> Value {
    if multiple {
        let open: HashSet<usize> = open_ixs.iter().copied().collect();
        json!(
            ids.iter()
                .enumerate()
                .filter(|(ix, _)| open.contains(ix))
                .map(|(_, id)| id.clone())
                .collect::<Vec<_>>()
        )
    } else {
        open_ixs
            .first()
            .and_then(|ix| ids.get(*ix))
            .map(|s| json!(s))
            .unwrap_or(Value::Null)
    }
}

/// One axis of a `scroll` viewport: a pixel size, or fill the parent.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ScrollExtent {
    Px(f32),
    Fill,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScrollViewport {
    width: ScrollExtent,
    height: ScrollExtent,
}

/// Size of the outer scroll viewport. `:size` is a square, matching
/// `apply_style` on other nodes. Omitted `:height` fills leftover column
/// space; omitted `:width` fills the parent width.
fn scroll_viewport(node: &Node) -> ScrollViewport {
    if let Some(size) = node.size {
        return ScrollViewport {
            width: ScrollExtent::Px(size),
            height: ScrollExtent::Px(size),
        };
    }
    ScrollViewport {
        width: node
            .width
            .map(ScrollExtent::Px)
            .unwrap_or(ScrollExtent::Fill),
        height: node
            .height
            .map(ScrollExtent::Px)
            .unwrap_or(ScrollExtent::Fill),
    }
}

#[cfg(test)]
fn select_opts(node: &Node) -> Vec<SelectOpt> {
    extra::select_sections(node.collection())
        .into_iter()
        .flat_map(|section| section.items)
        .map(SelectOpt::from)
        .collect()
}

fn select_flat_vec(node: &Node) -> SearchableVec<SelectOpt> {
    SearchableVec::new(
        node.collection()
            .iter()
            .map(extra::select_leaf_from_item)
            .map(SelectOpt::from)
            .collect::<Vec<_>>(),
    )
}

fn select_group_vec(node: &Node) -> SearchableVec<SelectGroup<SelectOpt>> {
    SearchableVec::new(
        extra::select_sections(node.collection())
            .into_iter()
            .map(|section| {
                SelectGroup::new(section.title)
                    .items(section.items.into_iter().map(SelectOpt::from))
            })
            .collect::<Vec<_>>(),
    )
}

fn apply_select_controlled_value<D>(
    state: &mut SelectState<D>,
    selected: Option<&SharedString>,
    window: &mut Window,
    cx: &mut Context<SelectState<D>>,
) where
    D: SearchableListDelegate + 'static,
    D::Item: SearchableListItem<Value = SharedString>,
{
    match selected {
        Some(id) => state.set_selected_value(id, window, cx),
        None => state.set_selected_index(None, window, cx),
    }
}

fn emit_select_confirm(this: &mut RootView, key: &str, value: Option<&SharedString>) {
    let Some(id) = this
        .selects
        .get(key)
        .and_then(|slot| slot.on_change.clone())
    else {
        return;
    };
    if let Some(slot) = this.selects.get_mut(key) {
        slot.selected = value.cloned();
    }
    match value {
        Some(selected) => this.emit_value(id, json!(selected.to_string())),
        None => this.emit_value(id, Value::Null),
    }
}

fn finish_select<D>(mut select: Select<D>, node: &Node, cx: &App) -> AnyElement
where
    D: gpui_component::searchable_list::SearchableListDelegate + 'static,
    <D::Item as SearchableListItem>::Value: PartialEq + Clone,
{
    if let Some(placeholder) = node.placeholder.clone() {
        select = select.placeholder(placeholder);
    }
    if node.disabled {
        select = select.disabled(true);
    }
    select = select.with_size(mapping::parse_scale(node.control_size.as_deref()));
    if node.cleanable {
        select = select.cleanable(true);
    }
    if let Some(prefix) = node.title_prefix.clone().filter(|s| !s.is_empty()) {
        select = select.title_prefix(prefix);
    }
    if let Some(width) = node.menu_width.filter(|n| n.is_finite() && *n > 0.0) {
        select = select.menu_width(px(width));
    }
    if let Some(height) = node.menu_max_h.filter(|n| n.is_finite() && *n > 0.0) {
        select = select.menu_max_h(px(height));
    }
    if let Some(placeholder) = node.search_placeholder.clone().filter(|s| !s.is_empty()) {
        select = select.search_placeholder(placeholder);
    }
    if let Some(appearance) = node.appearance {
        select = select.appearance(appearance);
    }
    if let Some(enabled) = node.focus_ring {
        select = select.focus_ring(enabled);
    }
    if let Some(name) = node.icon.as_deref().and_then(mapping::parse_icon) {
        select = select.icon(Icon::new(name));
    }
    if let Some(label) = node
        .accessibility_label
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        select = select.accessibility_label(label.to_string());
    }
    if let Some(empty) = node.empty.clone().filter(|s| !s.is_empty()) {
        select = select.empty(move |_, _| empty.clone());
    }
    apply_style(select, node, cx).into_any_element()
}

fn combo_flat_vec(node: &Node) -> SearchableVec<ComboOpt> {
    SearchableVec::new(
        node.collection()
            .iter()
            .map(extra::select_leaf_from_item)
            .map(ComboOpt::from)
            .collect::<Vec<_>>(),
    )
}

fn combo_group_vec(node: &Node) -> SearchableVec<SelectGroup<ComboOpt>> {
    SearchableVec::new(
        extra::select_sections(node.collection())
            .into_iter()
            .map(|section| {
                SelectGroup::new(section.title).items(section.items.into_iter().map(ComboOpt::from))
            })
            .collect::<Vec<_>>(),
    )
}

fn apply_combobox_selected_values<D>(
    state: &Entity<ComboboxState<D>>,
    selected: &[SharedString],
    window: &mut Window,
    cx: &mut Context<RootView>,
) where
    D: SearchableListDelegate + 'static,
    D::Item: SearchableListItem<Value = SharedString>,
{
    state.update(cx, |state, cx| {
        state.set_selected_values(selected, window, cx);
    });
}

fn apply_combobox_query<D>(
    state: &Entity<ComboboxState<D>>,
    query: Option<&str>,
    window: &mut Window,
    cx: &mut Context<RootView>,
) where
    D: SearchableListDelegate + 'static,
    <D::Item as SearchableListItem>::Value: PartialEq + Clone,
{
    let Some(desired) = query else {
        return;
    };
    state.update(cx, |state, cx| {
        if extra::should_set_combobox_query(state.query(cx).as_ref(), Some(desired)) {
            state.set_query(desired.to_string(), window, cx);
        }
    });
}

fn sync_combobox_handle(
    handle: &ComboboxStateHandle,
    set_selected: Option<&[SharedString]>,
    query: Option<&str>,
    window: &mut Window,
    cx: &mut Context<RootView>,
) {
    match handle {
        ComboboxStateHandle::Flat(state) => {
            if let Some(selected) = set_selected {
                apply_combobox_selected_values(state, selected, window, cx);
            }
            apply_combobox_query(state, query, window, cx);
        }
        ComboboxStateHandle::Grouped(state) => {
            if let Some(selected) = set_selected {
                apply_combobox_selected_values(state, selected, window, cx);
            }
            apply_combobox_query(state, query, window, cx);
        }
    }
}

fn subscribe_combobox<D>(
    state: &Entity<ComboboxState<D>>,
    key: String,
    window: &mut Window,
    cx: &mut Context<RootView>,
) where
    D: SearchableListDelegate + 'static,
    D::Item: SearchableListItem<Value = SharedString>,
{
    cx.subscribe_in(
        state,
        window,
        move |this, _, event: &ComboboxEvent<D>, window, cx| match event {
            ComboboxEvent::Change(values) => {
                emit_combobox_change(this, &key, values, window, cx);
            }
            ComboboxEvent::Confirm(values) => {
                emit_combobox_confirm(this, &key, values);
            }
        },
    )
    .detach();
}

fn finish_combobox<D>(mut combo: Combobox<D>, node: &Node, cx: &App) -> AnyElement
where
    D: SearchableListDelegate + 'static,
    <D::Item as SearchableListItem>::Value: PartialEq + Clone,
{
    if let Some(placeholder) = node.placeholder.clone() {
        combo = combo.placeholder(placeholder);
    }
    if node.disabled {
        combo = combo.disabled(true);
    }
    combo = combo.with_size(mapping::parse_scale(node.control_size.as_deref()));
    if node.cleanable {
        combo = combo.cleanable(true);
    }
    if let Some(width) = node.menu_width.filter(|n| n.is_finite() && *n > 0.0) {
        combo = combo.menu_width(px(width));
    }
    if let Some(height) = node.menu_max_h.filter(|n| n.is_finite() && *n > 0.0) {
        combo = combo.menu_max_h(px(height));
    }
    if let Some(placeholder) = node.search_placeholder.clone().filter(|s| !s.is_empty()) {
        combo = combo.search_placeholder(placeholder);
    }
    if let Some(appearance) = node.appearance {
        combo = combo.appearance(appearance);
    }
    if let Some(enabled) = node.focus_ring {
        combo = combo.focus_ring(enabled);
    }
    if let Some(name) = node.icon.as_deref().and_then(mapping::parse_icon) {
        combo = combo.icon(Icon::new(name));
    }
    if let Some(name) = node.check_icon.as_deref().and_then(mapping::parse_icon) {
        combo = combo.check_icon(Icon::new(name));
    }
    if let Some(empty) = node.empty.clone().filter(|s| !s.is_empty()) {
        combo = combo.empty(move |_, _| empty.clone());
    }
    apply_style(combo, node, cx).into_any_element()
}

#[cfg(test)]
fn combo_opts(node: &Node) -> Vec<ComboOpt> {
    extra::select_sections(node.collection())
        .into_iter()
        .flat_map(|section| section.items)
        .map(ComboOpt::from)
        .collect()
}

fn table_has_primitive_children(node: &Node) -> bool {
    node.children.iter().any(|child| {
        matches!(
            child.kind.as_str(),
            "table-header" | "table-body" | "table-footer" | "table-caption"
        )
    })
}

fn style_table_head_node(el: TableHead, node: &Node) -> TableHead {
    let el = match extra::table_align_node(node) {
        extra::TableAlign::End => el.text_right(),
        extra::TableAlign::Center => el.text_center(),
        extra::TableAlign::Start => el,
    };
    let el = if node.span > 1 {
        el.col_span(node.span as usize)
    } else {
        el
    };
    match node.width {
        Some(width) => el.w(px(width)),
        None => el,
    }
}

fn style_table_cell_node(el: TableCell, node: &Node) -> TableCell {
    let el = match extra::table_align_node(node) {
        extra::TableAlign::End => el.text_right(),
        extra::TableAlign::Center => el.text_center(),
        extra::TableAlign::Start => el,
    };
    let el = if node.span > 1 {
        el.col_span(node.span as usize)
    } else {
        el
    };
    match node.width {
        Some(width) => el.w(px(width)),
        None => el,
    }
}

fn style_table_head(el: TableHead, col: &Item) -> TableHead {
    let el = match extra::table_align(col) {
        extra::TableAlign::End => el.text_right(),
        extra::TableAlign::Center => el.text_center(),
        extra::TableAlign::Start => el,
    };
    let el = if col.span > 1 {
        el.col_span(col.span as usize)
    } else {
        el
    };
    match col.width {
        Some(width) => el.w(px(width)),
        None => el,
    }
}

fn style_table_cell(el: TableCell, col: &Item) -> TableCell {
    let el = match extra::table_align(col) {
        extra::TableAlign::End => el.text_right(),
        extra::TableAlign::Center => el.text_center(),
        extra::TableAlign::Start => el,
    };
    let el = if col.span > 1 {
        el.col_span(col.span as usize)
    } else {
        el
    };
    match col.width {
        Some(width) => el.w(px(width)),
        None => el,
    }
}

fn paint_table_row(row: &Item, columns: &[Item]) -> TableRow {
    let mut table_row = TableRow::new();
    if columns.is_empty() {
        let cells = if row.cells.is_empty() {
            vec![row.label_or_id()]
        } else {
            row.cells.clone()
        };
        for text in cells {
            table_row = table_row.child(TableCell::new().child(text));
        }
        return table_row;
    }
    for (ix, col) in columns.iter().enumerate() {
        let text = row.cells.get(ix).cloned().unwrap_or_default();
        table_row = table_row.child(style_table_cell(TableCell::new().child(text), col));
    }
    for text in row.cells.iter().skip(columns.len()) {
        table_row = table_row.child(TableCell::new().child(text.clone()));
    }
    table_row
}

fn paint_table_from_items(mut table: Table, node: &Node) -> Table {
    let columns = node.options.as_slice();
    let (body, footer) = extra::split_table_footer(&node.items);
    if !columns.is_empty() {
        let mut header_row = TableRow::new();
        for col in columns {
            header_row = header_row.child(style_table_head(
                TableHead::new().child(col.label_or_id()),
                col,
            ));
        }
        table = table.child(TableHeader::new().child(header_row));
    }
    let mut body_el = TableBody::new();
    for row in body {
        body_el = body_el.child(paint_table_row(row, columns));
    }
    table = table.child(body_el);
    if let Some(foot) = footer {
        table = table.child(TableFooter::new().child(paint_table_row(foot, columns)));
    }
    if let Some(caption) = node.text.clone().filter(|s| !s.is_empty()) {
        table = table.child(TableCaption::new().child(caption));
    }
    table
}

#[cfg(test)]
fn select_selected_index(
    items: &[SelectOpt],
    selected: Option<&str>,
) -> Option<gpui_component::IndexPath> {
    selected.and_then(|id| {
        items
            .iter()
            .position(|item| item.id.as_ref() == id)
            .map(|ix| gpui_component::IndexPath::default().row(ix))
    })
}

fn sidebar_header_title(node: &Node) -> Option<String> {
    if node.collapsed {
        None
    } else {
        node.title.clone()
    }
}

/// Titles that `SearchableVec::perform_search` filters on (gpui-component 0.5.1).
#[cfg(test)]
fn select_search_matches(items: &[SelectOpt], query: &str) -> Vec<String> {
    let q = query.to_lowercase();
    items
        .iter()
        .filter(|item| item.title().to_lowercase().contains(&q))
        .map(|item| item.id.to_string())
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
struct OuterLayout {
    width: Option<f32>,
    height: Option<f32>,
    size: Option<f32>,
    flex_fill: bool,
    /// Flex children must be allowed to shrink on both axes. GPUI's default
    /// min-size is content-sized, which otherwise lets long rows overflow.
    shrink_width: bool,
    shrink_height: bool,
    /// Scroll viewports fill parent width when `:width` / `:size` are omitted.
    full_width: bool,
}

fn outer_layout(node: &Node) -> OuterLayout {
    let flex_fill = node.flex.unwrap_or(0.0) >= 1.0;
    OuterLayout {
        width: node.width,
        height: node.height,
        size: node.size,
        flex_fill,
        shrink_width: flex_fill,
        shrink_height: flex_fill,
        full_width: node.kind == "scroll" && node.width.is_none() && node.size.is_none(),
    }
}

fn apply_outer_box_style<E: Styled>(mut el: E, node: &Node) -> E {
    let layout = outer_layout(node);
    if let Some(width) = layout.width {
        el = el.w(px(width));
    }
    if let Some(height) = layout.height {
        el = el.h(px(height));
    }
    if let Some(size) = layout.size {
        el = el.size(px(size));
    }
    if layout.flex_fill {
        // Flex items default to content-sized minimums. Allow shrinking on
        // both axes so long rows stay bounded and nested scrolling works.
        el = el.flex_1();
    }
    if layout.shrink_width {
        el = el.min_w_0();
    }
    if layout.shrink_height {
        el = el.min_h_0();
    }
    el
}

fn copy_outer_layout<E: Styled>(mut el: E, node: &Node) -> E {
    el = apply_outer_box_style(el, node);
    if outer_layout(node).full_width {
        el = el.w_full();
    }
    el
}

/// Crate widgets that are not `Styled` (spinner, badge, clipboard): a host
/// `div` owns Clojure layout and visual keys. The inner control is unchanged.
fn style_host(el: impl IntoElement, node: &Node, cx: &App) -> AnyElement {
    apply_style(div(), node, cx).child(el).into_any_element()
}

/// Keep a crate widget from filling leftover column height (`size_full` /
/// `overflow_hidden` inside a flex-1 scroll). The outer wrapper owns
/// `:width` / `:height` / `:size` / `:flex`; the inner widget stays
/// content-sized. Omitted width still stretches to the column.
fn content_sized(el: impl IntoElement, node: &Node, cx: &App) -> AnyElement {
    let mut wrap = v_flex().flex_none();
    if node.width.is_none() && node.size.is_none() {
        wrap = wrap.w_full();
    }
    apply_style(wrap, node, cx).child(el).into_any_element()
}

/// List/table/tree use crate `size_full()`. They need a bounded viewport or
/// they collapse to zero / steal leftover column height.
///
/// Outer wrapper owns Clojure layout geometry and visual keys. Inner
/// List/Table/Tree owns virtualization (`size_full` inside the wrapper).
/// `:size` is a square. Omitted width fills the parent. Default height
/// applies only when height, size, and flex-fill are all omitted.
fn dock_tabs(panels: Vec<std::sync::Arc<dyn gpui::base::dock::PanelView>>, cx: &App) -> DockLayout {
    let mut layout = DockLayout::tabs();
    for panel in panels {
        layout = layout.panel_view(panel, cx);
    }
    layout
}

fn spawn_clj_nav_page(
    template: &Node,
    path: String,
    cmd_tx: mpsc::Sender<Cmd>,
    cx: &mut Context<RootView>,
) -> Entity<extra::CljNavPage> {
    let live = Rc::new(RefCell::new(template.clone()));
    cx.new(|_| extra::CljNavPage::new(live, path, cmd_tx))
}

fn apply_nav_trail_plan(
    slot: &mut NavStackSlot,
    steps: Vec<extra::NavTrailStep>,
    motion: gpui::base::NavMotion,
    catalog: &HashMap<String, Node>,
    key: &str,
    cmd_tx: &mpsc::Sender<Cmd>,
    cx: &mut Context<RootView>,
) {
    let last = steps.len().saturating_sub(1);
    for (i, step) in steps.into_iter().enumerate() {
        let step_motion = if i == last {
            motion
        } else {
            gpui::base::NavMotion::Immediate
        };
        apply_nav_trail_step(slot, step, step_motion, catalog, key, cmd_tx, cx);
    }
}

fn apply_nav_trail_step(
    slot: &mut NavStackSlot,
    step: extra::NavTrailStep,
    motion: gpui::base::NavMotion,
    catalog: &HashMap<String, Node>,
    key: &str,
    cmd_tx: &mpsc::Sender<Cmd>,
    cx: &mut Context<RootView>,
) {
    match step {
        extra::NavTrailStep::Push(id) => {
            let Some(template) = catalog.get(&id) else {
                return;
            };
            let index = slot.entries.len();
            let page = spawn_clj_nav_page(
                template,
                extra::nav_page_path(key, index, &id),
                cmd_tx.clone(),
                cx,
            );
            slot.state.update(cx, |stack, cx| {
                stack.push(page.clone(), motion, cx);
            });
            slot.entries.push((id, page));
            slot.forward.clear();
        }
        extra::NavTrailStep::Pop => {
            slot.state.update(cx, |stack, cx| {
                let _ = stack.pop(motion, cx);
            });
            if let Some(entry) = slot.entries.pop() {
                slot.forward.push(entry);
            }
        }
        extra::NavTrailStep::Forward => {
            let Some(entry) = slot.forward.pop() else {
                return;
            };
            slot.state.update(cx, |stack, cx| {
                let _ = stack.forward(motion, cx);
            });
            slot.entries.push(entry);
        }
        extra::NavTrailStep::PopToRoot => {
            slot.state.update(cx, |stack, cx| {
                let _ = stack.pop_to_root(motion, cx);
            });
            while slot.entries.len() > 1 {
                if let Some(entry) = slot.entries.pop() {
                    slot.forward.push(entry);
                }
            }
        }
        extra::NavTrailStep::Replace(id) => {
            let Some(template) = catalog.get(&id) else {
                return;
            };
            let index = slot.entries.len().saturating_sub(1);
            let page = spawn_clj_nav_page(
                template,
                extra::nav_page_path(key, index, &id),
                cmd_tx.clone(),
                cx,
            );
            slot.state.update(cx, |stack, cx| {
                let _ = stack.replace(page.clone(), motion, cx);
            });
            if let Some(last) = slot.entries.last_mut() {
                *last = (id, page);
            } else {
                slot.entries.push((id, page));
            }
        }
        extra::NavTrailStep::Rebuild(new_ids) => {
            let views: Vec<(String, Entity<extra::CljNavPage>)> = new_ids
                .into_iter()
                .enumerate()
                .filter_map(|(index, id)| {
                    let template = catalog.get(&id)?;
                    let page = spawn_clj_nav_page(
                        template,
                        extra::nav_page_path(key, index, &id),
                        cmd_tx.clone(),
                        cx,
                    );
                    Some((id, page))
                })
                .collect();
            slot.state.update(cx, |stack, cx| {
                stack.clear(cx);
                for (_, page) in &views {
                    stack.push(page.clone(), gpui::base::NavMotion::Immediate, cx);
                }
            });
            slot.entries = views;
            slot.forward.clear();
        }
    }
}

/// Kit `forward_views()` → Clojure `:on-forward-change`. Deferred so the
/// callback cannot re-enter `export-tree` (and reset ids) during
/// `RootView::render`. Empty after first mount is skipped; later clears
/// (Push / Rebuild) still notify `[]`.
fn notify_nav_forward_change(
    slot: &mut NavStackSlot,
    on_forward_change: Option<String>,
    cmd_tx: &mpsc::Sender<Cmd>,
    window: &mut Window,
    cx: &mut Context<RootView>,
) {
    let nearest_first = extra::nav_forward_view_ids(&slot.forward);
    if slot.last_forward_notified.as_ref() == Some(&nearest_first) {
        return;
    }
    if slot.last_forward_notified.is_none() && nearest_first.is_empty() {
        slot.last_forward_notified = Some(nearest_first);
        return;
    }
    slot.last_forward_notified = Some(nearest_first.clone());
    let Some(callback_id) = on_forward_change.filter(|id| !id.is_empty()) else {
        return;
    };
    let cmd_tx = cmd_tx.clone();
    cx.defer_in(window, move |_, _, _| {
        protocol::send_callbacks(
            &cmd_tx,
            vec![protocol::CallbackCall::with_value(
                callback_id,
                json!(nearest_first),
            )],
        );
    });
}

fn viewport_sized(el: impl IntoElement, node: &Node, default_h: f32, cx: &App) -> AnyElement {
    let mut wrap = v_flex().min_h_0();
    if node.width.is_none() && node.size.is_none() {
        wrap = wrap.w_full();
    }
    if node.height.is_none() && node.size.is_none() && node.flex.unwrap_or(0.0) < 1.0 {
        wrap = wrap.h(px(default_h));
    }
    apply_style(wrap, node, cx).child(el).into_any_element()
}

/// Viewport/box geometry only. Visual Styled stays on the inner Kit widget.
///
/// Used by `MessageScroller` and `NavStack`, whose root `Styled` is a
/// documented style boundary. List / table / editor still use `viewport_sized`.
fn viewport_box_sized(el: impl IntoElement, node: &Node, default_h: f32) -> AnyElement {
    let mut wrap = v_flex().min_h_0();
    if node.width.is_none() && node.size.is_none() {
        wrap = wrap.w_full();
    }
    if node.height.is_none() && node.size.is_none() && node.flex.unwrap_or(0.0) < 1.0 {
        wrap = wrap.h(px(default_h));
    }
    apply_outer_box_style(wrap, node)
        .child(el)
        .into_any_element()
}

/// Layout contract for `ui/context-menu`.
///
/// The host is a flex column (`v_flex` + `min_h_0`), never a block `div`.
/// If the menu omitted `:flex`, leftover column height is inherited from any
/// flex-fill child so wrapping a `:flex 1` table/list/tree does not drop it.
/// An explicit wrapper `:flex` (including `0` / `0.5`) is never overridden.
fn context_menu_flex_fill(node: &Node) -> bool {
    match node.flex {
        Some(flex) => flex >= 1.0,
        None => node
            .children
            .iter()
            .any(|child| child.flex.unwrap_or(0.0) >= 1.0),
    }
}

/// Testable layout contract for `ui/context-menu`.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
struct ContextMenuWrap {
    flex_column: bool,
    flex_fill: bool,
    shrink_width: bool,
    shrink_height: bool,
}

#[cfg(test)]
fn context_menu_wrap(node: &Node) -> ContextMenuWrap {
    let flex_fill = context_menu_flex_fill(node);
    ContextMenuWrap {
        flex_column: true,
        flex_fill,
        shrink_width: flex_fill,
        shrink_height: true,
    }
}

fn emit_list_change(this: &RootView, key: &str, ix: IndexPath, cx: &App) {
    let Some(slot) = this.lists.get(key) else {
        return;
    };
    let Some(callback) = slot.on_change.clone() else {
        return;
    };
    if let Some(id) = slot.state.read(cx).delegate().id_at(ix) {
        this.emit_value(callback, json!(id));
    }
}

fn emit_list_activation(this: &RootView, key: &str, ix: IndexPath, cx: &App) {
    let Some(slot) = this.lists.get(key) else {
        return;
    };
    let Some(row_id) = slot.state.read(cx).delegate().id_at(ix) else {
        return;
    };
    protocol::send_callbacks(
        &this.cmd_tx,
        protocol::list_activation_calls(slot.on_change.clone(), slot.on_confirm.clone(), row_id),
    );
}

fn emit_combobox_change(
    this: &mut RootView,
    key: &str,
    values: &[SharedString],
    window: &mut Window,
    cx: &mut Context<RootView>,
) {
    let Some(slot) = this.comboboxes.get_mut(key) else {
        return;
    };
    // Match a later Clojure echo of these ids so set_selected_values
    // is a no-op and does not clear a still-open search query.
    slot.selected = values.to_vec();
    let payload = extra::combobox_payload(slot.multiple, values);
    if !slot.coalesce.on_change(payload) {
        return;
    }
    let key = key.to_string();
    cx.defer_in(window, move |this, _, _cx| {
        this.flush_pending_combobox_change(&key);
    });
}

fn emit_combobox_confirm(this: &mut RootView, key: &str, values: &[SharedString]) {
    let Some(slot) = this.comboboxes.get_mut(key) else {
        return;
    };
    slot.selected = values.to_vec();
    let pending = slot.coalesce.on_confirm();
    let on_change = slot.on_change.clone();
    let on_confirm = slot.on_confirm.clone();
    let confirm_payload = extra::combobox_payload(slot.multiple, values);
    let calls = if let Some(payload) = pending {
        protocol::combobox_activation_calls(on_change, on_confirm, payload)
    } else {
        protocol::combobox_activation_calls(None, on_confirm, confirm_payload)
    };
    protocol::send_callbacks(&this.cmd_tx, calls);
}

fn emit_table_activation(this: &RootView, key: &str, ix: usize, include_change: bool, cx: &App) {
    let Some(slot) = this.tables.get(key) else {
        return;
    };
    let Some(row_id) = slot.state.read(cx).delegate().id_at(ix) else {
        return;
    };
    let on_change = if include_change {
        slot.on_change.clone()
    } else {
        None
    };
    protocol::send_callbacks(
        &this.cmd_tx,
        protocol::table_activation_calls(on_change, slot.on_confirm.clone(), row_id),
    );
}

fn sync_list_selection(
    state: &Entity<ListState<RowListDelegate>>,
    selected: Option<&str>,
    window: &mut Window,
    cx: &mut Context<RootView>,
) {
    let decision = {
        let list = state.read(cx);
        rows::selection_sync(
            selected,
            list.selected_index().map(|ix| ix.row),
            |id| list.delegate().index_of(id).map(|ix| ix.row),
            |id| list.delegate().contains_id(id),
        )
    };
    match decision {
        SelectionSync::Select(ix) => state.update(cx, |list, cx| {
            list.set_selected_index(Some(IndexPath::new(ix)), window, cx);
        }),
        SelectionSync::Clear => state.update(cx, |list, cx| {
            list.set_selected_index(None, window, cx);
        }),
        SelectionSync::Keep => {}
    }
}

fn sync_tree_selection(
    state: &Entity<TreeState>,
    items: &[TreeItem],
    selected: Option<&str>,
    cx: &mut Context<RootView>,
) {
    let current = state.read(cx).selected_index();
    let decision = rows::selection_sync(
        selected,
        current,
        |id| rows::tree_visible_index(items, id),
        |id| rows::tree_contains_id(items, id),
    );
    match decision {
        SelectionSync::Select(ix) => {
            state.update(cx, |tree, cx| tree.set_selected_index(Some(ix), cx));
        }
        SelectionSync::Clear => {
            state.update(cx, |tree, cx| tree.set_selected_index(None, cx));
        }
        SelectionSync::Keep => {
            // Collapsed/filtered ids stay selected in Clojure. If the native
            // highlight now points at a different visible row, clear it.
            if let Some(id) = selected {
                if rows::tree_visible_index(items, id).is_none() {
                    let current_id = state
                        .read(cx)
                        .selected_entry()
                        .map(|entry| entry.item().id.to_string());
                    if current.is_some() && current_id.as_deref() != Some(id) {
                        state.update(cx, |tree, cx| tree.set_selected_index(None, cx));
                    }
                }
            }
        }
    }
}

/// Testable layout contract for `content_sized` wrappers.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
struct ContentWrap {
    width: Option<f32>,
    height: Option<f32>,
    size: Option<f32>,
    flex_fill: bool,
    fill_width: bool,
    flex_none: bool,
}

#[cfg(test)]
fn content_wrap(node: &Node) -> ContentWrap {
    let layout = outer_layout(node);
    ContentWrap {
        width: layout.width,
        height: layout.height,
        size: layout.size,
        flex_fill: layout.flex_fill,
        fill_width: layout.width.is_none() && layout.size.is_none(),
        flex_none: !layout.flex_fill,
    }
}

/// Testable layout contract for `row_intrinsic` wrappers.
#[cfg(test)]
fn row_intrinsic_wrap(node: &Node) -> ContentWrap {
    let layout = outer_layout(node);
    ContentWrap {
        width: layout.width,
        height: layout.height,
        size: layout.size,
        flex_fill: false,
        fill_width: false,
        flex_none: true,
    }
}

/// Testable layout contract for list/table/tree viewports.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
struct ViewportWrap {
    width: Option<f32>,
    height: Option<f32>,
    size: Option<f32>,
    flex_fill: bool,
    fill_width: bool,
    default_height: Option<f32>,
    visual: bool,
}

#[cfg(test)]
fn viewport_wrap(node: &Node, default_h: f32) -> ViewportWrap {
    let layout = outer_layout(node);
    let default_height = if node.height.is_none() && node.size.is_none() && !layout.flex_fill {
        Some(default_h)
    } else {
        None
    };
    ViewportWrap {
        width: layout.width,
        height: layout.height,
        size: layout.size,
        flex_fill: layout.flex_fill,
        fill_width: node.width.is_none() && node.size.is_none(),
        default_height,
        visual: node.padding.is_some() || node.bg.is_some() || node.border.is_some(),
    }
}

/// MessageScroller wrap contract: box geometry on the host wrapper, visual
/// Styled on Kit's MessageScroller root.
#[cfg(test)]
fn message_scroller_style_split(node: &Node) -> (ViewportWrap, bool) {
    let mut wrap = viewport_wrap(node, 400.0);
    let kit_root_visual = wrap.visual
        || node.gap.is_some()
        || node.color.is_some()
        || node.font_size.is_some()
        || node.shadow;
    wrap.visual = false;
    (wrap, kit_root_visual)
}

fn with_tooltip(el: AnyElement, node: &Node, key: &str) -> AnyElement {
    let Some(text) = node.tooltip.clone().filter(|s| !s.is_empty()) else {
        return el;
    };
    copy_outer_layout(div().id(eid(&format!("{key}-tip"))), node)
        .tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx))
        .child(el)
        .into_any_element()
}

/// Kit `Styled` refinements: gap, padding, type, colors, alignment.
/// Not box geometry (`:width` / `:height` / `:size` / `:flex`).
fn apply_kit_visual_style<E: Styled>(el: E, node: &Node, cx: &App) -> E {
    let mut el = mapping::apply_visual_style(el, node);
    if node_theme_pref(node).is_some() {
        if node.color.is_none() {
            el = el.text_color(cx.theme().foreground);
        }
        if node.bg.is_none()
            && matches!(
                node.kind.as_str(),
                "window" | "vstack" | "hstack" | "scroll" | "group-box"
            )
        {
            el = el.bg(cx.theme().background);
        }
    }
    el
}

fn apply_style<E: Styled>(el: E, node: &Node, cx: &App) -> E {
    apply_outer_box_style(apply_kit_visual_style(el, node, cx), node)
}

fn node_theme_pref(node: &Node) -> Option<&str> {
    node.theme.as_deref().filter(|theme| !theme.is_empty())
}

#[derive(Clone)]
enum ThemeApply {
    Appearance(ThemeMode),
    Named(Rc<ThemeConfig>),
}

fn resolve_theme(node: &Node, window: &Window, cx: &App) -> Option<ThemeApply> {
    let pref = node_theme_pref(node)?;
    if catalog::is_appearance(pref) {
        let mode = match catalog::normalize(pref).as_str() {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::from(window.appearance()),
        };
        Some(ThemeApply::Appearance(mode))
    } else {
        match catalog::lookup(pref, ThemeMode::from(window.appearance()), cx) {
            Some(config) => Some(ThemeApply::Named(config)),
            None => {
                eprintln!("[host] unknown gpui-component theme {pref:?}; keeping current");
                None
            }
        }
    }
}

fn apply_theme_pref(pref: &str, window: &Window, cx: &mut App) {
    if catalog::is_appearance(pref) {
        catalog::reset_default_palettes(cx);
        let mode = match catalog::normalize(pref).as_str() {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::from(window.appearance()),
        };
        Theme::change(mode, None, cx);
        return;
    }
    match catalog::lookup(pref, ThemeMode::from(window.appearance()), cx) {
        Some(config) => {
            let already =
                catalog::names_equal(Theme::global(cx).theme_name(), config.name.as_ref());
            if !already {
                Theme::global_mut(cx).apply_config(&config);
            }
        }
        None => {
            eprintln!("[host] unknown gpui-component theme {pref:?}; keeping current");
        }
    }
}

fn activate_theme(applied: &ThemeApply, cx: &mut App) {
    match applied {
        ThemeApply::Appearance(mode) => {
            catalog::reset_default_palettes(cx);
            Theme::change(*mode, None, cx);
        }
        ThemeApply::Named(config) => {
            Theme::global_mut(cx).apply_config(config);
        }
    }
}

/// Switches gpui-component's global theme around a subtree during layout/paint.
/// Widgets such as Button read `cx.theme()` in `RenderOnce::render`, which runs
/// in `request_layout`, not when Clojure builds the tree.
///
/// Theme is process-global (`Theme::change` / `apply_config` on `App`). Nested
/// scopes work because layout/prepaint/paint of a subtree are synchronous: we
/// restore the previous `Theme` before a sibling is laid out. A second window
/// would share that global and is not supported today. Headless GPUI tests
/// cannot exercise this without a real window and GPU; Clojure tests cover
/// that `:theme` serializes per node.
struct ThemeScope {
    applied: ThemeApply,
    child: AnyElement,
}

impl ThemeScope {
    fn new(applied: ThemeApply, child: AnyElement) -> Self {
        Self { applied, child }
    }
}

fn with_theme_apply<R>(applied: &ThemeApply, cx: &mut App, f: impl FnOnce(&mut App) -> R) -> R {
    let prev = Theme::global(cx).clone();
    activate_theme(applied, cx);
    let result = f(cx);
    *Theme::global_mut(cx) = prev;
    result
}

impl IntoElement for ThemeScope {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ThemeScope {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        with_theme_apply(&self.applied, cx, |cx| {
            (self.child.request_layout(window, cx), ())
        })
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        with_theme_apply(&self.applied, cx, |cx| {
            let _ = self.child.prepaint(window, cx);
        });
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        with_theme_apply(&self.applied, cx, |cx| {
            self.child.paint(window, cx);
        });
    }
}

fn eid(path: &str) -> SharedString {
    SharedString::from(path.to_string())
}

fn quit_host(cx: &mut App) {
    cx.quit();
    // Unbundled macOS binaries often ignore `[NSApp terminate:]`, which is
    // what GPUI's quit() schedules. The window is already gone; kill the
    // host so Clojure sees the socket close and exits too.
    #[cfg(target_os = "macos")]
    std::process::exit(0);
}

pub fn open_window(
    nrepl_port: u16,
    cmd_tx: mpsc::Sender<Cmd>,
    event_rx: async_channel::Receiver<HostEvent>,
    cx: &mut App,
) {
    use gpui::{Bounds, TitlebarOptions, WindowBounds, WindowOptions, size};

    // GPUI's macOS default is to keep the NSApplication running after the
    // last window closes. The close-button path also goes through an
    // async try_borrow_mut; if App is already borrowed, on_window_closed
    // never fires. Hook should-close too, and always quit this
    // single-window host — don't wait for windows().is_empty().
    cx.on_window_closed(|cx, _window_id| {
        quit_host(cx);
    })
    .detach();

    let bounds = Bounds::centered(None, size(px(580.), px(820.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("clj-gpui".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        |window, cx| {
            let view = cx.new(|cx| RootView::new(nrepl_port, cmd_tx, event_rx, window, cx));
            let weak = view.downgrade();
            cx.on_action(move |action: &action_bridge::CljAction, app| {
                let _ = weak.update(app, |this, _cx| {
                    this.handle_clj_action(action);
                });
            });
            let root = cx.new(|cx| Root::new(view, window, cx));
            window.on_window_should_close(cx, |_, cx| {
                quit_host(cx);
                true
            });
            root
        },
    )
    .unwrap();
    cx.activate(true);
}

#[cfg(test)]
mod zenity_tests {
    use super::{ZenityPick, zenity_from_output};
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    fn exit(code: i32) -> ExitStatus {
        ExitStatus::from_raw(code << 8)
    }

    #[test]
    fn success_with_path_is_picked() {
        assert_eq!(
            zenity_from_output(exit(0), b"/tmp/docs\n", b""),
            ZenityPick::Picked("/tmp/docs".into())
        );
    }

    #[test]
    fn success_with_empty_stdout_is_cancelled() {
        assert_eq!(
            zenity_from_output(exit(0), b"  \n", b""),
            ZenityPick::Cancelled
        );
    }

    #[test]
    fn exit_1_is_user_cancel() {
        assert_eq!(zenity_from_output(exit(1), b"", b""), ZenityPick::Cancelled);
    }

    #[test]
    fn other_nonzero_is_failure() {
        assert_eq!(
            zenity_from_output(exit(255), b"", b"display is not set\n"),
            ZenityPick::Failed("display is not set".into())
        );
    }
}

#[cfg(test)]
mod scroll_viewport_tests {
    use super::{Node, ScrollExtent, scroll_viewport};

    fn node_with(width: Option<f32>, height: Option<f32>, size: Option<f32>) -> Node {
        Node {
            width,
            height,
            size,
            ..Node::default()
        }
    }

    #[test]
    fn flex_scroll_no_height_fills_parent() {
        let v = scroll_viewport(&node_with(None, None, None));
        assert_eq!(v.width, ScrollExtent::Fill);
        assert_eq!(v.height, ScrollExtent::Fill);
    }

    #[test]
    fn fixed_height_keeps_full_width() {
        let v = scroll_viewport(&node_with(None, Some(220.0), None));
        assert_eq!(v.width, ScrollExtent::Fill);
        assert_eq!(v.height, ScrollExtent::Px(220.0));
    }

    #[test]
    fn explicit_width_constrains_viewport() {
        let v = scroll_viewport(&node_with(Some(300.0), None, None));
        assert_eq!(v.width, ScrollExtent::Px(300.0));
        assert_eq!(v.height, ScrollExtent::Fill);
    }

    #[test]
    fn explicit_width_and_height() {
        let v = scroll_viewport(&node_with(Some(300.0), Some(220.0), None));
        assert_eq!(v.width, ScrollExtent::Px(300.0));
        assert_eq!(v.height, ScrollExtent::Px(220.0));
    }

    #[test]
    fn size_is_a_square_viewport() {
        let v = scroll_viewport(&node_with(Some(300.0), Some(220.0), Some(180.0)));
        assert_eq!(v.width, ScrollExtent::Px(180.0));
        assert_eq!(v.height, ScrollExtent::Px(180.0));
    }
}

#[cfg(test)]
mod select_control_tests {
    use super::{
        Node, SelectOpt, outer_layout, select_opts, select_search_matches, select_selected_index,
        sidebar_header_title,
    };
    use crate::protocol::Item;
    use gpui_kit::SharedString;

    fn select_node(value: Option<serde_json::Value>, ids: &[&str]) -> Node {
        Node {
            kind: "select".into(),
            value,
            options: ids
                .iter()
                .map(|id| Item {
                    id: Some((*id).into()),
                    label: Some((*id).into()),
                    ..Item::default()
                })
                .collect(),
            ..Node::default()
        }
    }

    fn opt(id: &str, label: &str) -> SelectOpt {
        SelectOpt {
            id: SharedString::from(id.to_string()),
            label: SharedString::from(label.to_string()),
            disabled: false,
            display: None,
        }
    }

    #[test]
    fn nil_and_missing_values_clear_selection() {
        let items = select_opts(&select_node(None, &["clj", "rs"]));
        assert_eq!(select_selected_index(&items, None), None);
        let items = select_opts(&select_node(Some(serde_json::Value::Null), &["clj"]));
        assert_eq!(select_selected_index(&items, None), None);
    }

    #[test]
    fn value_a_to_b_updates_index() {
        let items = select_opts(&select_node(None, &["clj", "rs", "go"]));
        let a = select_selected_index(&items, Some("clj")).unwrap();
        let b = select_selected_index(&items, Some("rs")).unwrap();
        assert_eq!(a.row, 0);
        assert_eq!(b.row, 1);
        assert_ne!(a, b);
    }

    #[test]
    fn disappeared_option_clears_selection() {
        let items = select_opts(&select_node(None, &["rs", "go"]));
        assert_eq!(select_selected_index(&items, Some("clj")), None);
    }

    #[test]
    fn searchable_matches_filter_on_title_not_id() {
        let items = vec![opt("clj", "Clojure"), opt("rs", "Rust"), opt("go", "Go")];
        assert_eq!(
            select_search_matches(&items, "clo"),
            vec!["clj".to_string()]
        );
        assert!(
            select_search_matches(&items, "clj").is_empty(),
            "filter is on title, not id"
        );
        assert_eq!(select_search_matches(&items, "ust"), vec!["rs".to_string()]);
        assert!(select_search_matches(&items, "python").is_empty());
    }

    #[test]
    fn grouped_options_use_section_index_paths() {
        let node = Node {
            kind: "select".into(),
            options: vec![
                Item {
                    label: Some("Lisp".into()),
                    items: vec![Item {
                        id: Some("clj".into()),
                        label: Some("Clojure".into()),
                        ..Item::default()
                    }],
                    ..Item::default()
                },
                Item {
                    label: Some("Systems".into()),
                    items: vec![
                        Item {
                            id: Some("rs".into()),
                            label: Some("Rust".into()),
                            ..Item::default()
                        },
                        Item {
                            id: Some("go".into()),
                            label: Some("Go".into()),
                            disabled: true,
                            ..Item::default()
                        },
                    ],
                    ..Item::default()
                },
            ],
            ..Node::default()
        };
        let items = select_opts(&node);
        assert_eq!(items.len(), 3);
        assert!(items[2].disabled);
        let rs = crate::extra::select_index(node.collection(), Some("rs")).unwrap();
        assert_eq!(rs.section, 1);
        assert_eq!(rs.row, 0);
        assert!(crate::extra::select_is_grouped(node.collection()));
    }

    #[test]
    fn collapsed_sidebar_omits_text_header() {
        let expanded = Node {
            kind: "sidebar".into(),
            title: Some("Demo".into()),
            ..Node::default()
        };
        assert_eq!(sidebar_header_title(&expanded).as_deref(), Some("Demo"));

        let collapsed = Node {
            collapsed: true,
            ..expanded
        };
        assert_eq!(sidebar_header_title(&collapsed), None);
    }

    #[test]
    fn tooltip_wrapper_copies_width_height_flex_and_scroll_fill() {
        let button = Node {
            kind: "button".into(),
            width: Some(200.0),
            tooltip: Some("Save".into()),
            ..Node::default()
        };
        let layout = outer_layout(&button);
        assert_eq!(layout.width, Some(200.0));
        assert!(!layout.flex_fill);
        assert!(!layout.full_width);

        let column = Node {
            kind: "vstack".into(),
            flex: Some(1.0),
            tooltip: Some("col".into()),
            ..Node::default()
        };
        let layout = outer_layout(&column);
        assert!(layout.flex_fill);
        assert!(layout.shrink_width);
        assert!(layout.shrink_height);
        assert!(!layout.full_width);

        let label = Node {
            kind: "label".into(),
            width: Some(300.0),
            tooltip: Some("hint".into()),
            ..Node::default()
        };
        assert_eq!(outer_layout(&label).width, Some(300.0));

        let scroll = Node {
            kind: "scroll".into(),
            flex: Some(1.0),
            tooltip: Some("list".into()),
            ..Node::default()
        };
        let layout = outer_layout(&scroll);
        assert!(layout.flex_fill);
        assert!(layout.full_width);
        assert_eq!(layout.height, None);

        let fixed = Node {
            kind: "scroll".into(),
            height: Some(220.0),
            tooltip: Some("box".into()),
            ..Node::default()
        };
        let layout = outer_layout(&fixed);
        assert_eq!(layout.height, Some(220.0));
        assert!(!layout.flex_fill);
        assert!(!layout.shrink_width);
        assert!(!layout.shrink_height);
        assert!(layout.full_width);
    }
}

#[cfg(test)]
mod combobox_control_tests {
    use super::{Node, combo_opts};
    use crate::protocol::Item;

    #[test]
    fn grouped_combobox_options_flatten_group_children() {
        let node = Node {
            kind: "combobox".into(),
            options: vec![
                Item {
                    label: Some("Lisp".into()),
                    items: vec![Item {
                        id: Some("clj".into()),
                        label: Some("Clojure".into()),
                        ..Item::default()
                    }],
                    ..Item::default()
                },
                Item {
                    label: Some("Systems".into()),
                    items: vec![
                        Item {
                            id: Some("rs".into()),
                            label: Some("Rust".into()),
                            ..Item::default()
                        },
                        Item {
                            id: Some("go".into()),
                            label: Some("Go".into()),
                            disabled: true,
                            ..Item::default()
                        },
                    ],
                    ..Item::default()
                },
            ],
            ..Node::default()
        };
        let items = combo_opts(&node);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].id.as_ref(), "clj");
        assert_eq!(items[1].id.as_ref(), "rs");
        assert!(items[2].disabled);
        let rs = crate::extra::select_index(node.collection(), Some("rs")).unwrap();
        assert_eq!(rs.section, 1);
        assert_eq!(rs.row, 0);
        assert!(crate::extra::select_is_grouped(node.collection()));
        let groups = extra_combo_sections(&node);
        assert_eq!(groups, vec!["Lisp", "Systems"]);
    }

    fn extra_combo_sections(node: &Node) -> Vec<String> {
        crate::extra::select_sections(node.collection())
            .into_iter()
            .map(|section| section.title)
            .collect()
    }
}

#[cfg(test)]
mod slider_control_tests {
    use super::{
        Node, SliderEvent, SliderScale, SliderValue, slider_controlled_value,
        slider_effective_scale, slider_event_payload, slider_log_scale_fallback, slider_range,
        slider_range_thumbs, slider_slot_callback, slider_step, slider_value_changed,
        slider_wanted_value,
    };
    use serde_json::json;

    #[test]
    fn controlled_value_ignores_step_when_syncing() {
        let (lo, hi) = slider_range(Some(0.0), Some(100.0));
        assert_eq!(slider_step(Some(5.0)), 5.0);
        let wanted = SliderValue::Single(slider_controlled_value(Some(42.0), lo, hi));
        assert_eq!(wanted, SliderValue::Single(42.0));
        assert!(
            slider_value_changed(SliderValue::Single(40.0), wanted),
            "40 → 42 with step 5 must update the host entity"
        );
    }

    #[test]
    fn unchanged_value_does_not_need_set_value() {
        assert!(!slider_value_changed(
            SliderValue::Single(40.0),
            SliderValue::Single(40.0)
        ));
        assert!(slider_value_changed(
            SliderValue::Single(40.0),
            SliderValue::Single(40.1)
        ));
    }

    #[test]
    fn tiny_range_controlled_value_is_applied() {
        let (lo, hi) = slider_range(Some(0.0), Some(0.0001));
        assert_eq!((lo, hi), (0.0, 0.0001));
        let wanted = SliderValue::Single(slider_controlled_value(Some(0.00005), lo, hi));
        assert_eq!(wanted, SliderValue::Single(0.00005));
        assert!(
            slider_value_changed(SliderValue::Single(0.0), wanted),
            "0 → 5e-5 on a 0..1e-4 slider must update the host entity"
        );
        assert!(!slider_value_changed(wanted, wanted));
    }

    #[test]
    fn min_max_clamping() {
        assert_eq!(slider_range(Some(100.0), Some(0.0)), (0.0, 100.0));
        assert_eq!(slider_controlled_value(Some(150.0), 0.0, 100.0), 100.0);
        assert_eq!(slider_controlled_value(Some(-5.0), 0.0, 100.0), 0.0);
        assert_eq!(slider_controlled_value(None, 10.0, 20.0), 10.0);
        assert_eq!(slider_step(Some(0.0)), 1.0);
        assert_eq!(slider_step(None), 1.0);
    }

    #[test]
    fn range_thumbs_clamp_and_swap() {
        assert_eq!(
            slider_range_thumbs(20.0, 70.0, 0.0, 100.0),
            SliderValue::Range(20.0, 70.0)
        );
        assert_eq!(
            slider_range_thumbs(90.0, 10.0, 0.0, 100.0),
            SliderValue::Range(10.0, 90.0)
        );
        assert_eq!(
            slider_range_thumbs(-5.0, 150.0, 0.0, 100.0),
            SliderValue::Range(0.0, 100.0)
        );
    }

    #[test]
    fn array_value_is_range_without_range_flag() {
        let node = Node {
            value: Some(json!([20, 70])),
            ..Node::default()
        };
        assert_eq!(
            slider_wanted_value(&node, 0.0, 100.0),
            SliderValue::Range(20.0, 70.0)
        );
        assert_eq!(
            slider_event_payload(SliderValue::Range(20.0, 70.0)),
            json!([20.0, 70.0])
        );
    }

    #[test]
    fn range_flag_with_scalar_uses_min_to_value() {
        let node = Node {
            range: true,
            value: Some(json!(40)),
            ..Node::default()
        };
        assert_eq!(
            slider_wanted_value(&node, 0.0, 100.0),
            SliderValue::Range(0.0, 40.0)
        );
        let omitted = Node {
            range: true,
            ..Node::default()
        };
        assert_eq!(
            slider_wanted_value(&omitted, 10.0, 80.0),
            SliderValue::Range(10.0, 80.0)
        );
    }

    #[test]
    fn logarithmic_needs_positive_min() {
        let log = Node {
            scale: Some("logarithmic".into()),
            ..Node::default()
        };
        assert_eq!(
            slider_effective_scale(&log, 0.25, 4.0),
            SliderScale::Logarithmic
        );
        assert_eq!(
            slider_effective_scale(&log, 0.0, 100.0),
            SliderScale::Linear
        );
        assert_eq!(
            slider_effective_scale(&log, 10.0, 10.0),
            SliderScale::Linear
        );
        let linear = Node::default();
        assert_eq!(
            slider_effective_scale(&linear, 0.25, 4.0),
            SliderScale::Linear
        );
        assert!(slider_log_scale_fallback(&log, 0.0, 100.0));
        assert!(slider_log_scale_fallback(&log, 10.0, 10.0));
        assert!(!slider_log_scale_fallback(&log, 0.25, 4.0));
        assert!(!slider_log_scale_fallback(&linear, 0.0, 100.0));
    }

    #[test]
    fn release_emits_on_release_payload() {
        let single = slider_slot_callback(
            &SliderEvent::Release(SliderValue::Single(42.0)),
            Some("change"),
            Some("release"),
        );
        assert_eq!(single, Some(("release".into(), json!(42.0))));

        let range = slider_slot_callback(
            &SliderEvent::Release(SliderValue::Range(20.0, 70.0)),
            Some("change"),
            Some("release"),
        );
        assert_eq!(range, Some(("release".into(), json!([20.0, 70.0]))));

        let change = slider_slot_callback(
            &SliderEvent::Change(SliderValue::Single(10.0)),
            Some("change"),
            Some("release"),
        );
        assert_eq!(change, Some(("change".into(), json!(10.0))));
    }

    #[test]
    fn missing_on_release_is_silent_and_set_value_has_no_event() {
        // Host only forwards Kit SliderEvent. SliderState::set_value notifies
        // without emitting Change or Release, so a controlled echo cannot fire
        // either callback.
        assert_eq!(
            slider_slot_callback(
                &SliderEvent::Release(SliderValue::Single(42.0)),
                Some("change"),
                None
            ),
            None
        );
        assert_eq!(
            slider_slot_callback(
                &SliderEvent::Change(SliderValue::Range(1.0, 2.0)),
                None,
                Some("release")
            ),
            None
        );
    }
}

#[cfg(test)]
mod accordion_control_tests {
    use super::accordion_callback_value;
    use serde_json::json;

    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn multiple_open_ids_follow_item_order_not_hashset_order() {
        let items = ids(&["audio", "display", "network"]);
        // Crate HashSet iteration can yield any order; 2 then 0 is the
        // reverse of item order.
        let value = accordion_callback_value(&items, &[2, 0], true);
        assert_eq!(value, json!(["audio", "network"]));
        let shuffled = accordion_callback_value(&items, &[1, 0], true);
        assert_eq!(shuffled, json!(["audio", "display"]));
    }

    #[test]
    fn comma_in_id_stays_one_array_entry() {
        let items = ids(&["audio,advanced", "display"]);
        let value = accordion_callback_value(&items, &[1, 0], true);
        assert_eq!(value, json!(["audio,advanced", "display"]));
    }

    #[test]
    fn single_select_sends_one_id_or_null() {
        let items = ids(&["audio", "display"]);
        assert_eq!(
            accordion_callback_value(&items, &[1], false),
            json!("display")
        );
        assert_eq!(accordion_callback_value(&items, &[], false), json!(null));
    }
}

#[cfg(test)]
mod widget_wrap_tests {
    use super::{
        Node, content_wrap, context_menu_wrap, message_scroller_style_split, outer_layout,
        row_intrinsic_wrap, viewport_wrap,
    };
    use crate::extra;
    use crate::mapping;
    use crate::protocol::Item;
    use gpui_kit::{Styled, div};
    use serde_json::json;

    #[test]
    fn avatar_group_row_is_flex_none_without_full_width() {
        let node = Node {
            kind: "avatar-group".into(),
            ..Node::default()
        };
        let row = row_intrinsic_wrap(&node);
        let column = content_wrap(&node);
        assert!(row.flex_none);
        assert!(!row.fill_width);
        assert!(!row.flex_fill);
        assert!(column.fill_width);

        let grow = Node {
            kind: "avatar-group".into(),
            flex: Some(1.0),
            ..Node::default()
        };
        let row = row_intrinsic_wrap(&grow);
        assert!(row.flex_none);
        assert!(!row.fill_width);
        assert!(!row.flex_fill);

        let styled = Node {
            kind: "avatar-group".into(),
            gap: Some(8.0),
            padding: Some(4.0),
            width: Some(200.0),
            flex: Some(1.0),
            ..Node::default()
        };
        let split = crate::overlay::avatar_group_style_split(&styled);
        assert_eq!(split.kit_gap, Some(8.0));
        assert_eq!(split.kit_padding, Some(4.0));
        assert_eq!(split.wrap_width, Some(200.0));
        assert!(split.wrap_flex_fill);
        assert_eq!(
            split.wrap_workaround_w,
            crate::overlay::avatar_group_content_width(&styled)
        );
    }

    #[test]
    fn accordion_default_is_full_width_flex_none() {
        let node = Node {
            kind: "accordion".into(),
            ..Node::default()
        };
        let wrap = content_wrap(&node);
        assert!(wrap.fill_width);
        assert!(wrap.flex_none);
        assert!(!wrap.flex_fill);
        assert_eq!(wrap.width, None);
        assert_eq!(wrap.height, None);
    }

    #[test]
    fn accordion_outer_owns_width_height_size_flex() {
        let sized = Node {
            kind: "accordion".into(),
            width: Some(240.0),
            height: Some(80.0),
            ..Node::default()
        };
        let wrap = content_wrap(&sized);
        assert_eq!(wrap.width, Some(240.0));
        assert_eq!(wrap.height, Some(80.0));
        assert!(!wrap.fill_width);
        assert!(wrap.flex_none);

        let square = Node {
            kind: "description-list".into(),
            size: Some(180.0),
            width: Some(300.0),
            ..Node::default()
        };
        let wrap = content_wrap(&square);
        assert_eq!(wrap.size, Some(180.0));
        assert!(!wrap.fill_width);

        let grow = Node {
            kind: "accordion".into(),
            flex: Some(1.0),
            ..Node::default()
        };
        let wrap = content_wrap(&grow);
        assert!(wrap.flex_fill);
        assert!(!wrap.flex_none);
        assert!(wrap.fill_width);
    }

    #[test]
    fn list_table_tree_viewport_default_height() {
        for kind in ["list", "data-table", "tree"] {
            let node = Node {
                kind: kind.into(),
                ..Node::default()
            };
            let wrap = viewport_wrap(&node, 200.0);
            assert!(wrap.fill_width, "{kind}");
            assert_eq!(wrap.default_height, Some(200.0), "{kind}");
            assert!(!wrap.flex_fill, "{kind}");
            assert_eq!(wrap.size, None, "{kind}");
        }
        let nav = Node {
            kind: "nav-stack".into(),
            ..Node::default()
        };
        let wrap = viewport_wrap(&nav, 200.0);
        assert!(wrap.fill_width);
        assert_eq!(wrap.default_height, Some(200.0));
        assert!(!wrap.flex_fill);
    }

    #[test]
    fn list_viewport_explicit_width_height_size_flex_and_visual() {
        let wide = Node {
            kind: "list".into(),
            width: Some(320.0),
            height: Some(180.0),
            padding: Some(8.0),
            bg: Some("#111111".into()),
            border: Some("#222222".into()),
            ..Node::default()
        };
        let wrap = viewport_wrap(&wide, 200.0);
        assert_eq!(wrap.width, Some(320.0));
        assert_eq!(wrap.height, Some(180.0));
        assert!(!wrap.fill_width);
        assert_eq!(wrap.default_height, None);
        assert!(wrap.visual);

        let square = Node {
            kind: "data-table".into(),
            size: Some(160.0),
            width: Some(300.0),
            ..Node::default()
        };
        let wrap = viewport_wrap(&square, 220.0);
        assert_eq!(wrap.size, Some(160.0));
        assert!(!wrap.fill_width);
        assert_eq!(wrap.default_height, None);

        let grow = Node {
            kind: "tree".into(),
            flex: Some(1.0),
            ..Node::default()
        };
        let wrap = viewport_wrap(&grow, 200.0);
        assert!(wrap.flex_fill);
        assert!(wrap.fill_width);
        assert_eq!(wrap.default_height, None);
    }

    #[test]
    fn message_scroller_root_owns_visual_style() {
        let node = Node {
            kind: "message-scroller".into(),
            padding: Some(8.0),
            gap: Some(4.0),
            bg: Some("#112233".into()),
            border: Some("#445566".into()),
            height: Some(320.0),
            ..Node::default()
        };
        let (wrap, kit_root_visual) = message_scroller_style_split(&node);
        assert_eq!(wrap.height, Some(320.0));
        assert!(wrap.fill_width);
        assert_eq!(wrap.default_height, None);
        assert!(
            !wrap.visual,
            "visual padding/bg/border must stay on Kit MessageScroller root"
        );
        assert!(kit_root_visual);

        let mut root = mapping::apply_visual_style(div(), &node);
        assert!(root.style().padding.is_some());
        assert!(root.style().background.is_some());

        let omitted = Node {
            kind: "message-scroller".into(),
            padding: Some(8.0),
            ..Node::default()
        };
        let (wrap, kit_root_visual) = message_scroller_style_split(&omitted);
        assert_eq!(wrap.default_height, Some(400.0));
        assert!(wrap.fill_width);
        assert!(!wrap.visual);
        assert!(kit_root_visual);
    }

    #[test]
    fn chart_outer_viewport_uses_horizontal_bar_height() {
        let node = Node {
            kind: "chart".into(),
            variant: Some("bar".into()),
            alignment: Some("left".into()),
            items: (0..8)
                .map(|i| Item {
                    id: Some(format!("r{i}")),
                    label: Some(format!("row-{i}")),
                    value: Some(json!(10 + i)),
                    ..Item::default()
                })
                .collect(),
            ..Node::default()
        };
        let default_h = extra::chart_viewport(&node).1;
        let wrap = viewport_wrap(&node, default_h);
        assert_eq!(wrap.default_height, Some(8.0 * 28.0 + 40.0));
        assert_eq!(wrap.height, None);
        assert!(!wrap.flex_fill);
    }

    #[test]
    fn chart_outer_viewport_keeps_flex_without_fixed_height() {
        let node = Node {
            kind: "chart".into(),
            variant: Some("bar".into()),
            alignment: Some("left".into()),
            flex: Some(1.0),
            items: (0..8)
                .map(|i| Item {
                    id: Some(format!("r{i}")),
                    label: Some(format!("row-{i}")),
                    value: Some(json!(1)),
                    ..Item::default()
                })
                .collect(),
            ..Node::default()
        };
        let wrap = viewport_wrap(&node, extra::chart_viewport(&node).1);
        assert_eq!(wrap.default_height, None);
        assert!(wrap.flex_fill);
    }

    #[test]
    fn context_menu_is_flex_column_not_block_div() {
        let node = Node {
            kind: "context-menu".into(),
            children: vec![Node {
                kind: "label".into(),
                ..Node::default()
            }],
            ..Node::default()
        };
        let wrap = context_menu_wrap(&node);
        assert!(wrap.flex_column);
        assert!(!wrap.flex_fill);
        assert!(!wrap.shrink_width);
        assert!(wrap.shrink_height);
    }

    #[test]
    fn context_menu_inherits_flex_only_when_omitted() {
        let child = Node {
            kind: "data-table".into(),
            flex: Some(1.0),
            ..Node::default()
        };
        let omitted = Node {
            kind: "context-menu".into(),
            children: vec![child.clone()],
            ..Node::default()
        };
        let inherited = context_menu_wrap(&omitted);
        assert!(inherited.flex_column);
        assert!(inherited.flex_fill);
        assert!(inherited.shrink_width);
        assert!(inherited.shrink_height);

        let explicit_zero = Node {
            kind: "context-menu".into(),
            flex: Some(0.0),
            children: vec![child],
            ..Node::default()
        };
        let wrap = context_menu_wrap(&explicit_zero);
        assert!(wrap.flex_column);
        assert!(!wrap.flex_fill);
        assert!(!wrap.shrink_width);
        assert!(wrap.shrink_height);
    }

    #[test]
    fn context_menu_own_flex_fills_without_flex_child() {
        let node = Node {
            kind: "context-menu".into(),
            flex: Some(1.0),
            children: vec![Node {
                kind: "label".into(),
                ..Node::default()
            }],
            ..Node::default()
        };
        let wrap = context_menu_wrap(&node);
        assert!(wrap.flex_column);
        assert!(wrap.flex_fill);
        assert!(wrap.shrink_width);
        assert!(wrap.shrink_height);
    }

    #[test]
    fn spinner_badge_clipboard_layout_keys_are_on_the_node() {
        for kind in ["spinner", "badge", "clipboard"] {
            let node = Node {
                kind: kind.into(),
                width: Some(24.0),
                height: Some(24.0),
                flex: Some(1.0),
                ..Node::default()
            };
            let layout = outer_layout(&node);
            assert_eq!(layout.width, Some(24.0), "{kind}");
            assert_eq!(layout.height, Some(24.0), "{kind}");
            assert!(layout.flex_fill, "{kind}");
        }
    }
}
