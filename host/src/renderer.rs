use crate::catalog;
use crate::extra;
use crate::mapping;
use crate::overlay;
use crate::preview;
use crate::protocol::{self, Cmd, HostEvent, Item, Node};
use crate::rows::{self, RowListDelegate, RowTableDelegate, SelectionSync};
use gpui::{
    canvas, div, prelude::*, px, rgb, size, AnyElement, App, Axis, Bounds, ClickEvent, Context,
    DismissEvent, Element, ElementId, Entity, Focusable, GlobalElementId, InspectorElementId,
    Keystroke, LayoutId, PathPromptOptions, Pixels, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    accordion::Accordion,
    alert::Alert,
    avatar::Avatar,
    badge::Badge,
    breadcrumb::{Breadcrumb, BreadcrumbItem},
    button::{Button, ButtonVariants as _, Toggle, ToggleVariants as _},
    checkbox::Checkbox,
    clipboard::Clipboard,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    date_picker::{DatePicker, DatePickerEvent, DatePickerState},
    description_list::DescriptionList,
    divider::Divider,
    dock::{DockArea, DockItem},
    group_box::{GroupBox, GroupBoxVariants as _},
    h_flex,
    input::{
        Input, InputEvent, InputState, NumberInput, NumberInputEvent, OtpInput, OtpState,
        StepAction,
    },
    kbd::Kbd,
    link::Link,
    list::{List, ListEvent, ListState},
    menu::{ContextMenuExt as _, DropdownMenu as _},
    notification::Notification,
    popover::Popover,
    progress::Progress,
    radio::{Radio, RadioGroup},
    resizable::{h_resizable, resizable_panel, v_resizable, ResizableState},
    scroll::ScrollableElement as _,
    select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState},
    sidebar::{Sidebar, SidebarMenu, SidebarMenuItem},
    skeleton::Skeleton,
    slider::{Slider, SliderEvent, SliderState, SliderValue},
    spinner::Spinner,
    switch::Switch,
    tab::{Tab, TabBar},
    table::{Table, TableEvent, TableState},
    tag::Tag,
    theme::{Theme, ThemeConfig, ThemeMode},
    tooltip::Tooltip,
    tree::{tree, TreeItem, TreeState},
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, IndexPath, Root, Sizable as _,
    WindowExt as _,
};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc;

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

struct SliderSlot {
    state: Entity<SliderState>,
    min: f32,
    max: f32,
    step: f32,
    on_change: Option<String>,
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
}

impl SelectItem for SelectOpt {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.id
    }
}

struct SelectSlot {
    state: Entity<SelectState<SearchableVec<SelectOpt>>>,
    searchable: bool,
    on_change: Option<String>,
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
    lists: HashMap<String, ListSlot>,
    tables: HashMap<String, TableSlot>,
    trees: HashMap<String, TreeSlot>,
    otps: HashMap<String, OtpSlot>,
    colors: HashMap<String, ColorSlot>,
    dates: HashMap<String, DateSlot>,
    editors: HashMap<String, InputSlot>,
    vlists: HashMap<String, Entity<extra::VirtualListView>>,
    docks: HashMap<String, DockSlot>,
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
    used_lists: HashSet<String>,
    used_tables: HashSet<String>,
    used_trees: HashSet<String>,
    used_otps: HashSet<String>,
    used_colors: HashSet<String>,
    used_dates: HashSet<String>,
    used_editors: HashSet<String>,
    used_vlists: HashSet<String>,
    used_docks: HashSet<String>,
    _appearance: Subscription,
    _window_bounds: Subscription,
    _keystrokes: Subscription,
    next_submit_seq: u64,
    tree_seq: Option<u64>,
    applied_title: String,
    applied_window_size: Option<(i32, i32)>,
    native_window_id: Option<u32>,
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
            lists: HashMap::new(),
            tables: HashMap::new(),
            trees: HashMap::new(),
            otps: HashMap::new(),
            colors: HashMap::new(),
            dates: HashMap::new(),
            editors: HashMap::new(),
            vlists: HashMap::new(),
            docks: HashMap::new(),
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
            used_lists: HashSet::new(),
            used_tables: HashSet::new(),
            used_trees: HashSet::new(),
            used_otps: HashSet::new(),
            used_colors: HashSet::new(),
            used_dates: HashSet::new(),
            used_editors: HashSet::new(),
            used_vlists: HashSet::new(),
            used_docks: HashSet::new(),
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
        for slot in self.inputs.values().chain(self.editors.values()) {
            let Some(id) = slot.on_escape.clone() else {
                continue;
            };
            if slot.state.read(cx).focus_handle(cx).is_focused(window) {
                let _ = self.cmd_tx.send(Cmd::Callback {
                    id,
                    value: None,
                    seq: None,
                });
                break;
            }
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
        // Do not wait for the next presented frame. GPUI stops its macOS
        // display link while the window is occluded (Evalight in front),
        // so on_next_frame would never run until the native window is
        // visible again.
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

    fn flush_callback_queue(&mut self) {
        let Some(tree) = self.tree.as_ref() else {
            return;
        };
        let Some(calls) = self.callback_queue.next(tree) else {
            return;
        };
        // Share the existing sequence allocator with input-submit responses.
        // The matching Tree is the barrier, not a timer or an arbitrary paint.
        self.next_submit_seq = self.next_submit_seq.saturating_add(1);
        let seq = self.next_submit_seq;
        self.callback_queue.sent(seq);
        protocol::send_callbacks_seq(&self.cmd_tx, calls, Some(seq));
    }

    fn schedule_input_change_flush(
        key: String,
        editor: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        cx.defer_in(window, move |this, _, _cx| {
            this.flush_input_change(&key, editor);
        });
    }

    fn flush_input_change(&mut self, key: &str, editor: bool) {
        let slot = if editor {
            self.editors.get_mut(key)
        } else {
            self.inputs.get_mut(key)
        };
        let Some(slot) = slot else {
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
        let Some(payload) = extra::input_change_payload(slot.as_number, &value) else {
            slot.change.clear();
            return;
        };
        self.emit_value(id, payload);
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
                state.read(cx).focus_handle(cx).focus(window);
            }
            if refresh {
                Self::schedule_input_change_flush(key.to_string(), false, window, cx);
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
                        Self::schedule_input_change_flush(key_owned.clone(), false, window, cx);
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
            state.read(cx).focus_handle(cx).focus(window);
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
        let value = slider_controlled_value(node.number_value(), lo, hi);

        if let Some(slot) = self.sliders.get_mut(key) {
            if (slot.min - lo).abs() <= f32::EPSILON
                && (slot.max - hi).abs() <= f32::EPSILON
                && (slot.step - step).abs() <= f32::EPSILON
            {
                slot.on_change = node.on_change.clone();
                let current = match slot.state.read(cx).value() {
                    SliderValue::Single(v) => v,
                    SliderValue::Range(_, end) => end,
                };
                // `set_value` notifies without emitting Change, so applying
                // Clojure's current value cannot loop. Step is drag granularity
                // only; a 40→42 update with step 5 must still land on 42.
                if slider_value_changed(current, value) {
                    slot.state.update(cx, |s, cx| {
                        s.set_value(value, window, cx);
                    });
                }
                return slot.state.clone();
            }
        }

        let state = cx.new(|_cx| {
            SliderState::new()
                .min(lo)
                .max(hi)
                .step(step)
                .default_value(value)
        });
        let key_owned = key.to_string();
        cx.subscribe(&state, move |this, _, event: &SliderEvent, _cx| {
            let SliderEvent::Change(changed) = event;
            let number = match changed {
                SliderValue::Single(v) => *v,
                SliderValue::Range(_, end) => *end,
            };
            let Some(id) = this
                .sliders
                .get(&key_owned)
                .and_then(|slot| slot.on_change.clone())
            else {
                return;
            };
            this.emit_value(id, json!(number));
        })
        .detach();
        self.sliders.insert(
            key.to_string(),
            SliderSlot {
                state: state.clone(),
                min: lo,
                max: hi,
                step,
                on_change: node.on_change.clone(),
                bar_px: None,
                settle: 0,
            },
        );
        state
    }

    fn select_slot(
        &mut self,
        key: &str,
        node: &Node,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SelectState<SearchableVec<SelectOpt>>> {
        self.used_selects.insert(key.to_string());
        let items = select_opts(node);
        let selected_index = select_selected_index(&items, node.string_value().as_deref());
        // SearchableVec implements perform_search; Vec<T> does not (0.5.1).

        if let Some(slot) = self.selects.get_mut(key) {
            if slot.searchable == node.searchable {
                slot.on_change = node.on_change.clone();
                slot.state.update(cx, |state, cx| {
                    state.set_items(SearchableVec::new(items.clone()), window, cx);
                    state.set_selected_index(selected_index, window, cx);
                });
                return slot.state.clone();
            }
        }

        let searchable = node.searchable;
        let state = cx.new(|cx| {
            let built = SelectState::new(SearchableVec::new(items), selected_index, window, cx);
            built.searchable(searchable)
        });
        let key_owned = key.to_string();
        cx.subscribe(
            &state,
            move |this, _, event: &SelectEvent<SearchableVec<SelectOpt>>, _cx| {
                let SelectEvent::Confirm(value) = event;
                let Some(id) = this
                    .selects
                    .get(&key_owned)
                    .and_then(|slot| slot.on_change.clone())
                else {
                    return;
                };
                match value {
                    Some(selected) => this.emit_value(id, json!(selected.to_string())),
                    None => this.emit_value(id, Value::Null),
                }
            },
        )
        .detach();
        self.selects.insert(
            key.to_string(),
            SelectSlot {
                state: state.clone(),
                searchable,
                on_change: node.on_change.clone(),
            },
        );
        state
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
                let mut button = Button::new(eid(&key)).label(label);
                button = apply_button_variant(button, node);
                if node.compact {
                    button = button.compact();
                }
                if node.disabled {
                    button = button.disabled(true);
                }
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
            "text-field" => {
                let state = self.input_slot(&key, node, window, cx);
                apply_style(Input::new(&state), node, cx).into_any_element()
            }
            "switch" => self.render_switch(node, &key, cx),
            "toggle" => self.render_toggle(node, &key, cx),
            "radio-group" => self.render_radio_group(node, &key, cx),
            "slider" => self.render_slider(node, &key, window, cx),
            "progress" => self.render_progress(node, cx),
            "divider" => self.render_divider(node, cx),
            "spinner" => self.render_spinner(node, cx),
            "tag" => self.render_tag(node, cx),
            "alert" => self.render_alert(node, &key, cx),
            "skeleton" => self.render_skeleton(node, cx),
            "kbd" => self.render_kbd(node, cx),
            "link" => self.render_link(node, &key, cx),
            "group-box" => self.render_group_box(node, path, &key, window, cx),
            "badge" => self.render_badge(node, path, window, cx),
            "tabs" => self.render_tabs(node, &key, cx),
            "select" => self.render_select(node, &key, window, cx),
            "icon" => self.render_icon(node, cx),
            "clipboard" => self.render_clipboard(node, &key, cx),
            "breadcrumb" => self.render_breadcrumb(node, &key, cx),
            "avatar" => self.render_avatar(node, cx),
            "accordion" => self.render_accordion(node, path, &key, window, cx),
            "description-list" => self.render_description_list(node, cx),
            "dialog" => div().into_any_element(),
            "popover" => self.render_popover(node, &key, cx),
            "dropdown-menu" => self.render_dropdown_menu(node, &key, window, cx),
            "context-menu" => self.render_context_menu(node, path, &key, window, cx),
            "list" => self.render_list(node, &key, window, cx),
            "table" => self.render_table(node, &key, window, cx),
            "tree" => self.render_tree(node, &key, window, cx),
            "sheet" => div().into_any_element(),
            "notification" => div().into_any_element(),
            "number-input" => self.render_number_input(node, &key, window, cx),
            "otp-input" => self.render_otp_input(node, &key, window, cx),
            "color-picker" => self.render_color_picker(node, &key, window, cx),
            "date-picker" => self.render_date_picker(node, &key, window, cx),
            "editor" => self.render_editor(node, &key, window, cx),
            "virtual-list" => self.render_virtual_list(node, &key, window, cx),
            "chart" => viewport_sized(extra::paint_chart(node, &key, cx), node, 180.0, cx),
            "markdown" | "html" => apply_style(v_flex().id(eid(&key)), node, cx)
                .child(extra::paint_markdown(node, &key, window, cx))
                .into_any_element(),
            "sidebar" => self.render_sidebar(node, &key, cx),
            "settings" => viewport_sized(
                extra::build_settings(node, &key, &self.cmd_tx),
                node,
                360.0,
                cx,
            ),
            "dock" => self.render_dock(node, &key, window, cx),
            "resizable" => self.render_resizable(node, path, &key, window, cx),
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
        apply_style(Progress::new().value(value), node, cx).into_any_element()
    }

    fn render_divider(&self, node: &Node, cx: &App) -> AnyElement {
        let mut divider = if mapping::parse_axis(node.orientation.as_deref()) == Axis::Vertical {
            Divider::vertical()
        } else {
            Divider::horizontal()
        };
        if node.dashed {
            divider = divider.dashed();
        }
        if let Some(label) = node.text.clone().filter(|s| !s.is_empty()) {
            divider = divider.label(label);
        }
        apply_style(divider, node, cx).into_any_element()
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

    fn render_select(
        &mut self,
        node: &Node,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.select_slot(key, node, window, cx);
        let mut select = Select::new(&state);
        if let Some(placeholder) = node.placeholder.clone() {
            select = select.placeholder(placeholder);
        }
        if node.disabled {
            select = select.disabled(true);
        }
        select = select.with_size(mapping::parse_scale(node.control_size.as_deref()));
        apply_style(select, node, cx).into_any_element()
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
        let mut avatar =
            Avatar::new().with_size(mapping::parse_scale(node.control_size.as_deref()));
        if let Some(name) = node.text.clone().or(node.title.clone()) {
            avatar = avatar.name(name);
        }
        apply_style(avatar, node, cx).into_any_element()
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

    fn render_table(
        &mut self,
        node: &Node,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.table_slot(key, node, window, cx);
        viewport_sized(Table::new(&state), node, 220.0, cx)
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
                    overlay::bind_dialog_callbacks(dialog, key.clone(), emit.clone(), close.clone())
                });
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
        cx.subscribe(
            &state,
            move |this, otp, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    if let Some(id) = this.otps.get(&key_owned).and_then(|s| s.on_change.clone()) {
                        let value = otp.read(cx).value().to_string();
                        this.emit_value(id, json!(value));
                    }
                }
                InputEvent::Blur => {
                    if let Some(id) = this.otps.get(&key_owned).and_then(|s| s.on_blur.clone()) {
                        let value = otp.read(cx).value().to_string();
                        this.emit_value(id, json!(value));
                    }
                }
                _ => {}
            },
        )
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
        viewport_sized(Input::new(&state).h_full(), node, 200.0, cx)
    }

    fn editor_slot(
        &mut self,
        key: &str,
        node: &Node,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
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
                Self::schedule_input_change_flush(key.to_string(), true, window, cx);
            }
            return state;
        }
        let placeholder = node.placeholder.clone().unwrap_or_default();
        let state = cx.new(|cx| {
            InputState::new(window, cx)
                .code_editor(language)
                .placeholder(placeholder)
                .default_value(wanted)
                .rows(12)
        });
        self.editors.insert(
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
                    let Some(slot) = this.editors.get_mut(&key_owned) else {
                        return;
                    };
                    let value = input.read(cx).value().to_string();
                    if slot.change.on_change(value) {
                        Self::schedule_input_change_flush(key_owned.clone(), true, window, cx);
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

    fn render_sidebar(&self, node: &Node, _key: &str, cx: &App) -> AnyElement {
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
        let mut sidebar = match extra::parse_sidebar_side(node) {
            gpui_component::Side::Right => Sidebar::right(),
            _ => Sidebar::left(),
        };
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
        let area =
            cx.new(|cx| DockArea::new(SharedString::from(key.to_string()), None, window, cx));
        let weak = area.downgrade();
        let mut panels: HashMap<String, Entity<extra::CljPanel>> = HashMap::new();
        let mut by_side: HashMap<&str, Vec<std::sync::Arc<dyn gpui_component::dock::PanelView>>> =
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
                .push(std::sync::Arc::new(panel.clone()));
            panels.insert(id, panel);
        }
        area.update(cx, |dock, cx| {
            if let Some(center) = by_side.remove("center") {
                if !center.is_empty() {
                    dock.set_center(DockItem::tabs(center, &weak, window, cx), window, cx);
                }
            }
            if let Some(left) = by_side.remove("left") {
                if !left.is_empty() {
                    dock.set_left_dock(
                        DockItem::tabs(left, &weak, window, cx),
                        node.width.map(px),
                        true,
                        window,
                        cx,
                    );
                }
            }
            if let Some(right) = by_side.remove("right") {
                if !right.is_empty() {
                    dock.set_right_dock(
                        DockItem::tabs(right, &weak, window, cx),
                        Some(px(240.)),
                        true,
                        window,
                        cx,
                    );
                }
            }
            if let Some(bottom) = by_side.remove("bottom") {
                if !bottom.is_empty() {
                    let bottom_h = node
                        .height
                        .map(|h| (h * 0.34).clamp(64.0, 140.0))
                        .unwrap_or(96.0);
                    dock.set_bottom_dock(
                        DockItem::tabs(bottom, &weak, window, cx),
                        Some(px(bottom_h)),
                        true,
                        window,
                        cx,
                    );
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
                if child.kind == "dialog" || child.kind == "sheet" || child.kind == "notification" {
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
        self.used_lists.clear();
        self.used_tables.clear();
        self.used_trees.clear();
        self.used_otps.clear();
        self.used_colors.clear();
        self.used_dates.clear();
        self.used_editors.clear();
        self.used_vlists.clear();
        self.used_docks.clear();
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
        let used_vlists = std::mem::take(&mut self.used_vlists);
        self.vlists.retain(|key, _| used_vlists.contains(key));
        let used_docks = std::mem::take(&mut self.used_docks);
        self.docks.retain(|key, _| used_docks.contains(key));
        let used_resizables = std::mem::take(&mut self.used_resizables);
        self.resizables
            .retain(|key, _| used_resizables.contains(key));

        self.sync_dialogs(window, cx);
        self.sync_sheet(window, cx);
        self.sync_notifications(window, cx);

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

fn parse_color(value: &str) -> Option<u32> {
    let value = value.trim().trim_start_matches('#');
    u32::from_str_radix(value, 16).ok()
}

/// Step is drag granularity. Clojure's controlled value is accepted as-is
/// (then clamped to min/max). Compare f32 values exactly so a tiny-range
/// slider (e.g. 0 → 5e-5 with max 1e-4) is not discarded. `set_value`
/// notifies without emitting `SliderEvent::Change`, so an unchanged tree
/// cannot loop.
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

fn slider_value_changed(current: f32, wanted: f32) -> bool {
    current != wanted
}

/// Map crate `on_toggle_click` indices to ids. HashSet iteration order is
/// not stable, so multiple open ids follow original item order.
fn accordion_callback_value(ids: &[String], open_ixs: &[usize], multiple: bool) -> Value {
    if multiple {
        let open: HashSet<usize> = open_ixs.iter().copied().collect();
        json!(ids
            .iter()
            .enumerate()
            .filter(|(ix, _)| open.contains(ix))
            .map(|(_, id)| id.clone())
            .collect::<Vec<_>>())
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

fn apply_button_variant(button: Button, node: &Node) -> Button {
    match node.variant.as_deref() {
        Some("primary") => button.primary(),
        Some("ghost") => button.ghost(),
        Some("text") => button.text(),
        Some("danger") => button.danger(),
        Some("outline") => button.outline(),
        _ if node.primary => button.primary(),
        _ => button,
    }
}

fn select_opts(node: &Node) -> Vec<SelectOpt> {
    node.collection()
        .iter()
        .map(|item| SelectOpt {
            id: SharedString::from(item.id_or_label()),
            label: SharedString::from(item.label_or_id()),
        })
        .collect()
}

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

fn copy_outer_layout<E: Styled>(mut el: E, node: &Node) -> E {
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
        el = el.flex_1();
    }
    if layout.shrink_width {
        el = el.min_w_0();
    }
    if layout.shrink_height {
        el = el.min_h_0();
    }
    if layout.full_width {
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

fn with_tooltip(el: AnyElement, node: &Node, key: &str) -> AnyElement {
    let Some(text) = node.tooltip.clone().filter(|s| !s.is_empty()) else {
        return el;
    };
    copy_outer_layout(div().id(eid(&format!("{key}-tip"))), node)
        .tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx))
        .child(el)
        .into_any_element()
}

fn apply_style<E: Styled>(mut el: E, node: &Node, cx: &App) -> E {
    if let Some(gap) = node.gap {
        el = el.gap(px(gap));
    }
    if let Some(padding) = node.padding {
        el = el.p(px(padding));
    }
    if let Some(width) = node.width {
        el = el.w(px(width));
    }
    if let Some(height) = node.height {
        el = el.h(px(height));
    }
    if let Some(size) = node.size {
        el = el.size(px(size));
    }
    let layout = outer_layout(node);
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
    if let Some(font_size) = node.font_size {
        el = el.text_size(px(font_size));
    }
    if let Some(family) = &node.font_family {
        el = el.font_family(family.clone());
    }
    if let Some(weight) = &node.font_weight {
        el = match weight.as_str() {
            "thin" => el.font_weight(gpui::FontWeight::THIN),
            "extralight" | "extra-light" | "ultralight" => {
                el.font_weight(gpui::FontWeight::EXTRA_LIGHT)
            }
            "bold" => el.font_weight(gpui::FontWeight::BOLD),
            "semibold" | "semi-bold" => el.font_weight(gpui::FontWeight::SEMIBOLD),
            "medium" => el.font_weight(gpui::FontWeight::MEDIUM),
            "light" => el.font_weight(gpui::FontWeight::LIGHT),
            _ => el.font_weight(gpui::FontWeight::NORMAL),
        };
    }
    if let Some(color) = node.color.as_deref().and_then(parse_color) {
        el = el.text_color(rgb(color));
    }
    if let Some(bg) = node.bg.as_deref().and_then(parse_color) {
        el = el.bg(rgb(bg));
    }
    if let Some(border) = node.border.as_deref().and_then(parse_color) {
        el = el.border_1().border_color(rgb(border));
    }
    if let Some(border) = node.border_bottom.as_deref().and_then(parse_color) {
        el = el.border_b_1().border_color(rgb(border));
    }
    if node.strikethrough {
        el = el.line_through();
    }
    if node.shadow {
        el = el.shadow_lg();
    }
    match node.align.as_deref() {
        Some("center") => el = el.items_center(),
        Some("end") => el = el.items_end(),
        Some("start") => el = el.items_start(),
        _ => {}
    }
    match node.justify.as_deref() {
        Some("center") => el = el.justify_center(),
        Some("end") | Some("right") => el = el.justify_end(),
        Some("between") => el = el.justify_between(),
        _ => {}
    }
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
    use gpui::{size, Bounds, TitlebarOptions, WindowBounds, WindowOptions};

    // GPUI's macOS default is to keep the NSApplication running after the
    // last window closes. The close-button path also goes through an
    // async try_borrow_mut; if App is already borrowed, on_window_closed
    // never fires. Hook should-close too, and always quit this
    // single-window host — don't wait for windows().is_empty().
    cx.on_window_closed(|cx| {
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
    use super::{zenity_from_output, ZenityPick};
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
    use super::{scroll_viewport, Node, ScrollExtent};

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
        outer_layout, select_opts, select_search_matches, select_selected_index,
        sidebar_header_title, Node, SelectOpt,
    };
    use crate::protocol::Item;
    use gpui::SharedString;

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
mod slider_control_tests {
    use super::{slider_controlled_value, slider_range, slider_step, slider_value_changed};

    #[test]
    fn controlled_value_ignores_step_when_syncing() {
        let (lo, hi) = slider_range(Some(0.0), Some(100.0));
        assert_eq!(slider_step(Some(5.0)), 5.0);
        let wanted = slider_controlled_value(Some(42.0), lo, hi);
        assert_eq!(wanted, 42.0);
        assert!(
            slider_value_changed(40.0, wanted),
            "40 → 42 with step 5 must update the host entity"
        );
    }

    #[test]
    fn unchanged_value_does_not_need_set_value() {
        assert!(!slider_value_changed(40.0, 40.0));
        assert!(slider_value_changed(40.0, 40.1));
    }

    #[test]
    fn tiny_range_controlled_value_is_applied() {
        let (lo, hi) = slider_range(Some(0.0), Some(0.0001));
        assert_eq!((lo, hi), (0.0, 0.0001));
        let wanted = slider_controlled_value(Some(0.00005), lo, hi);
        assert_eq!(wanted, 0.00005);
        assert!(
            slider_value_changed(0.0, wanted),
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
    use super::{content_wrap, context_menu_wrap, outer_layout, viewport_wrap, Node};

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
        for kind in ["list", "table", "tree"] {
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
            kind: "table".into(),
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
            kind: "table".into(),
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
