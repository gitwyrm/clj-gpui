//! Capture a CGWindow by id, including windows covered by other apps.
//!
//! xcap lists `kCGWindowListOptionOnScreenOnly` and snapshots the window's
//! screen rect. GPUI also stops its display link when macOS reports the
//! window occluded, so waiting for the next presented frame never runs
//! while Evalight is in front. We snapshot `CGRectNull` +
//! `kCGWindowListOptionIncludingWindow` from the window server backing
//! store instead, using the NSWindow's `windowNumber`.

// CGWindowListCreateImage is deprecated in favor of ScreenCaptureKit; it is
// still the occlusion-friendly backing-store read on current GPUI macOS SDKs.
#![allow(deprecated)]

use image::RgbaImage;
use objc2_app_kit::NSView;
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGDataProvider, CGImage, CGWindowID, CGWindowImageOption, CGWindowListCreateImage,
    CGWindowListOption,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// `kCGWindowImageBoundsIgnoreFraming | ShouldBeOpaque | BestResolution`
const IMAGE_OPTIONS: u32 = (1 << 0) | (1 << 1) | (1 << 3);

fn cg_rect_null() -> CGRect {
    // CGRectNull: origin at infinity, empty size.
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
    // `Window::window_handle` is GPUI's AnyWindowHandle. The AppKit NSView
    // comes from the raw-window-handle trait.
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return None;
    };
    let view = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
    let number = view.window()?.windowNumber();
    (number > 0).then_some(number as u32)
}

pub fn capture_window_id(window_id: u32) -> Option<RgbaImage> {
    let cg_image = CGWindowListCreateImage(
        cg_rect_null(),
        CGWindowListOption::OptionIncludingWindow,
        window_id as CGWindowID,
        CGWindowImageOption::from_bits_retain(IMAGE_OPTIONS),
    );
    let width = CGImage::width(cg_image.as_deref());
    let height = CGImage::height(cg_image.as_deref());
    if width == 0 || height == 0 {
        return None;
    }
    let data_provider = CGImage::data_provider(cg_image.as_deref());
    let data = CGDataProvider::data(data_provider.as_deref())?.to_vec();
    let bytes_per_row = CGImage::bytes_per_row(cg_image.as_deref());
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
    RgbaImage::from_raw(width as u32, height as u32, buffer)
}
