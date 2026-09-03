//! Capture this process's GPUI window, including when Evalight covers it.
//!
//! Two separate jobs:
//!
//! 1. Keep GPUI painting. 0.2.2 stops the CVDisplayLink unless
//!    `NSWindowOcclusionStateVisible` is set
//!    ([zed#63217](https://github.com/zed-industries/zed/issues/63217)).
//!    Override `-[GPUIWindow occlusionState]` (and `GPUIPanel`, not
//!    `NSWindow`) so the display link keeps presenting. Install this on
//!    the first `capture-preview` only, so ordinary apps keep GPUI's
//!    occlusion power-saving until Preview is used. Then re-run
//!    `windowDidChangeOcclusionState:` so a link that already stopped
//!    starts again.
//! 2. Read those pixels in-process with ScreenCaptureKit's
//!    desktop-independent window filter. A helper is a different PID;
//!    recent macOS will not give that process an occluded window.
//!    `CGWindowListCreateImage` is a fallback if ScreenCaptureKit is
//!    missing or refuses.
//!
//! `WindowOptions::inactive_frame_interval`
//! ([zed#62628](https://github.com/zed-industries/zed/pull/62628)) is not in
//! 0.2.2 and only throttles unfocused animation. It does not keep the
//! display link running while covered.

#![allow(deprecated)] // CGWindowListCreateImage*; ScreenCaptureKit is preferred.

use gpui_kit as gpui;
use std::ffi::c_char;
use std::ptr::NonNull;
use std::sync::{mpsc, Once};
use std::time::Duration;

use block2::RcBlock;
use image::RgbaImage;
use objc2::ffi::{class_addMethod, class_replaceMethod};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{msg_send, sel, AnyThread, ClassType, MainThreadMarker};
use objc2_app_kit::NSView;
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGDataProvider, CGImage, CGWindowID, CGWindowImageOption, CGWindowListCreateImage,
    CGWindowListOption,
};
use objc2_foundation::NSError;
use objc2_screen_capture_kit::{
    SCContentFilter, SCScreenshotManager, SCShareableContent, SCStreamConfiguration, SCWindow,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// `NSWindowOcclusionStateVisible`. GPUI only starts its display link when
/// this bit is set on `-[NSWindow occlusionState]`.
const OCCLUSION_STATE_VISIBLE: usize = 1 << 1;

const IMAGE_OPTIONS: CGWindowImageOption =
    CGWindowImageOption::from_bits_retain((1 << 0) | (1 << 1) | (1 << 3));
const SCK_TIMEOUT: Duration = Duration::from_secs(2);

fn cg_rect_null() -> CGRect {
    CGRect {
        origin: CGPoint {
            x: f64::INFINITY,
            y: f64::INFINITY,
        },
        size: CGSize {
            width: 0.0,
            height: 0.0,
        },
    }
}

pub fn cg_window_id_from_gpui(window: &gpui::Window) -> Option<u32> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return None;
    };
    let view = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
    let number = view.window()?.windowNumber();
    (number > 0).then_some(number as u32)
}

/// Install the occlusion override if needed, then re-run GPUI's occlusion
/// handler so a display link that already stopped is started again.
///
/// GPUI has no public `set_draw_while_occluded` in 0.2.2. Overriding the
/// getter on `GPUIWindow` / `GPUIPanel` only (not `NSWindow`) makes
/// `start_display_link` and `windowDidChangeOcclusionState:` take the visible
/// branch. Call from `capture-preview`, not window creation.
pub fn restart_occluded_display_link() {
    install_occlusion_override();
    kick_gpui_windows_to_paint();
}

fn install_occlusion_override() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| unsafe {
        override_occlusion_state(c"GPUIWindow");
        override_occlusion_state(c"GPUIPanel");
    });
}

unsafe fn override_occlusion_state(class_name: &std::ffi::CStr) {
    let Some(cls) = AnyClass::get(class_name) else {
        return;
    };
    let sel = sel!(occlusionState);
    // SAFETY: `imp` matches `-[NSWindow occlusionState]` (`Q@:`). Only
    // GPUIWindow / GPUIPanel are patched, never NSWindow.
    unsafe {
        let imp: Imp = std::mem::transmute(
            occlusion_state_always_visible
                as unsafe extern "C-unwind" fn(*mut AnyObject, Sel) -> usize,
        );
        let types: *const c_char = c"Q@:".as_ptr() as *const c_char;
        let cls_ptr = cls as *const AnyClass as *mut AnyClass;
        if !class_addMethod(cls_ptr, sel, imp, types).as_bool() {
            let _ = class_replaceMethod(cls_ptr, sel, imp, types);
        }
    }
}

unsafe extern "C-unwind" fn occlusion_state_always_visible(
    _this: *mut AnyObject,
    _cmd: Sel,
) -> usize {
    OCCLUSION_STATE_VISIBLE
}

fn kick_gpui_windows_to_paint() {
    let Some(_mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(app_cls) = AnyClass::get(c"NSApplication") else {
        return;
    };
    unsafe {
        let app: *mut AnyObject = msg_send![app_cls, sharedApplication];
        if app.is_null() {
            return;
        }
        let windows: *mut AnyObject = msg_send![app, windows];
        if windows.is_null() {
            return;
        }
        let count: usize = msg_send![windows, count];
        for i in 0..count {
            let window: *mut AnyObject = msg_send![windows, objectAtIndex: i];
            if window.is_null() {
                continue;
            }
            let window = &*window;
            let name = window.class().name();
            if name != c"GPUIWindow" && name != c"GPUIPanel" {
                continue;
            }
            let none: Option<&AnyObject> = None;
            let _: () = msg_send![window, windowDidChangeOcclusionState: none];
        }
    }
}

/// In-process capture used by Evalight. Do not spawn a helper: that PID is
/// not the window owner, so occluded snapshots are rejected.
pub fn capture_this_process(window_id: Option<u32>) -> Option<RgbaImage> {
    let window_id = window_id.filter(|id| *id > 0);
    capture_sck(window_id).or_else(|| window_id.and_then(capture_window_id))
}

fn capture_window_id(window_id: u32) -> Option<RgbaImage> {
    let cg_image = CGWindowListCreateImage(
        cg_rect_null(),
        CGWindowListOption::OptionIncludingWindow,
        window_id as CGWindowID,
        IMAGE_OPTIONS,
    );
    cg_image_to_rgba(cg_image.as_deref())
}

fn capture_sck(window_id: Option<u32>) -> Option<RgbaImage> {
    let window = sck_content(true)
        .and_then(|content| find_sc_window(&content, window_id))
        .or_else(|| sck_content(false).and_then(|content| find_sc_window(&content, window_id)))?;
    screenshot_sc_window(&window)
}

fn sck_content(current_process: bool) -> Option<Retained<SCShareableContent>> {
    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(
        move |content: *mut SCShareableContent, _err: *mut NSError| {
            let _ = tx.send(unsafe { Retained::retain(content) });
        },
    );
    unsafe {
        if current_process && class_responds_to_current_process_content() {
            SCShareableContent::getCurrentProcessShareableContentWithCompletionHandler(&block);
        } else {
            SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
                true, false, &block,
            );
        }
    }
    rx.recv_timeout(SCK_TIMEOUT).ok().flatten()
}

fn class_responds_to_current_process_content() -> bool {
    let sel = sel!(getCurrentProcessShareableContentWithCompletionHandler:);
    unsafe { msg_send![SCShareableContent::class(), respondsToSelector: sel] }
}

fn find_sc_window(content: &SCShareableContent, want: Option<u32>) -> Option<Retained<SCWindow>> {
    let windows = unsafe { content.windows() };
    let pid = std::process::id() as i32;
    let mut best: Option<(i64, Retained<SCWindow>)> = None;
    for i in 0..windows.count() {
        let window = windows.objectAtIndex(i);
        let id = unsafe { window.windowID() };
        if want == Some(id) && id > 0 {
            return Some(window);
        }
        let owner = unsafe { window.owningApplication() };
        let owner_pid = owner
            .as_ref()
            .map(|app| unsafe { app.processID() as i32 })
            .unwrap_or(0);
        if owner_pid != pid || unsafe { window.windowLayer() } != 0 {
            continue;
        }
        let frame = unsafe { window.frame() };
        let area = (frame.size.width * frame.size.height) as i64;
        if best.as_ref().map(|(a, _)| *a).unwrap_or(-1) < area {
            best = Some((area, window));
        }
    }
    best.map(|(_, window)| window)
}

fn screenshot_sc_window(window: &SCWindow) -> Option<RgbaImage> {
    let filter = unsafe {
        SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), window)
    };
    let config = unsafe { SCStreamConfiguration::new() };
    unsafe {
        config.setShowsCursor(false);
        let rect = filter.contentRect();
        let scale = f64::from(filter.pointPixelScale());
        let width = (rect.size.width * scale).round() as usize;
        let height = (rect.size.height * scale).round() as usize;
        if width > 0 && height > 0 {
            config.setWidth(width);
            config.setHeight(height);
        }
    }
    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(move |image: *mut CGImage, _err: *mut NSError| {
        let _ = tx.send(retain_cg_image(image));
    });
    unsafe {
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            &filter,
            &config,
            Some(&block),
        );
    }
    let image = rx.recv_timeout(SCK_TIMEOUT).ok().flatten()?;
    cg_image_to_rgba(Some(&image))
}

fn retain_cg_image(ptr: *mut CGImage) -> Option<CFRetained<CGImage>> {
    NonNull::new(ptr).map(|ptr| unsafe { CFRetained::retain(ptr) })
}

fn cg_image_to_rgba(cg_image: Option<&CGImage>) -> Option<RgbaImage> {
    let width = CGImage::width(cg_image);
    let height = CGImage::height(cg_image);
    if width == 0 || height == 0 {
        return None;
    }
    let data_provider = CGImage::data_provider(cg_image);
    let data = CGDataProvider::data(data_provider.as_deref())?.to_vec();
    let bytes_per_row = CGImage::bytes_per_row(cg_image);
    if bytes_per_row < width * 4 {
        return None;
    }
    let mut buffer = Vec::with_capacity(width * height * 4);
    for row in data.chunks_exact(bytes_per_row) {
        buffer.extend_from_slice(&row[..width * 4]);
    }
    for bgra in buffer.chunks_exact_mut(4) {
        bgra.swap(0, 2);
    }
    let image = RgbaImage::from_raw(width as u32, height as u32, buffer)?;
    image.pixels().any(|p| p.0[3] > 8).then_some(image)
}
