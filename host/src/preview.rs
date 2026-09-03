//! Snapshot the live native window for Evalight's Preview pane.
//!
//! GPUI does not expose framebuffer readback. Linux and Windows spawn this
//! same binary as a helper (PID B):
//!
//! ```text
//! clj-gpui --capture-preview --pid <host-pid> [--title <window-title>] [--wid <id>]
//! ```
//!
//! The helper uses [xcap] to find that PID's window and write a PNG to stdout.
//! A second process is required on Windows: `xcap::Window::all()` skips
//! windows owned by the current process so `GetWindowText` cannot deadlock
//! the GPUI message loop. The parent waits on a background thread, never the
//! UI thread, because Windows capture may `PrintWindow` the host.
//!
//! macOS captures in-process. A helper is a different PID, and recent macOS
//! will snapshot a visible window from that process while refusing the same
//! window once Evalight covers it. ScreenCaptureKit's desktop-independent
//! window filter reads our own window's backing store. GPUI 0.2.2 stops its
//! display link while occluded (zed#63217); this host overrides
//! `GPUIWindow`'s `occlusionState` so painting continues. Zed's
//! `inactive_frame_interval` (zed#62628) is not in 0.2.2 and does not
//! control occlusion.
//!
//! Failure is empty stdout / `None`. Never write the PNG to the host logs.

use base64::{engine::general_purpose::STANDARD, Engine as _};
#[cfg(not(target_os = "macos"))]
use std::io::Read;
use std::io::Write;
#[cfg(not(target_os = "macos"))]
use std::process::{Child, Command, ExitStatus, Stdio};
#[cfg(not(target_os = "macos"))]
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
#[path = "preview_macos.rs"]
mod preview_macos;

const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
#[cfg(not(target_os = "macos"))]
const HELPER_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureRequest {
    pub pid: u32,
    pub title: Option<String>,
    pub window_id: Option<u32>,
}

/// Parse `--capture-preview --pid N [--title T] [--wid W]` from argv (including argv0).
pub fn parse_capture_args<I, S>(args: I) -> Option<CaptureRequest>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut capture = false;
    let mut pid = None;
    let mut title = None;
    let mut window_id = None;
    let mut iter = args.into_iter();
    let _argv0 = iter.next();
    while let Some(arg) = iter.next() {
        match arg.as_ref() {
            "--capture-preview" => capture = true,
            "--pid" => {
                pid = iter.next().and_then(|s| s.as_ref().parse().ok());
            }
            "--title" => {
                title = iter.next().map(|s| s.as_ref().to_string());
            }
            "--wid" => {
                window_id = iter.next().and_then(|s| s.as_ref().parse().ok());
            }
            _ => {}
        }
    }
    if capture {
        Some(CaptureRequest {
            pid: pid.unwrap_or(0),
            title: title.filter(|t| !t.is_empty()),
            window_id: window_id.filter(|id| *id > 0),
        })
    } else {
        None
    }
}

/// Helper entry: write a PNG to stdout, or nothing. Always succeeds the process.
pub fn run_helper(request: CaptureRequest) {
    if let Some(png) = capture_pid(request.pid, request.title.as_deref(), request.window_id) {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(&png);
        let _ = out.flush();
    }
}

/// Platform window id for `--wid` (macOS `windowNumber`). Other platforms: `None`.
pub fn native_window_id(window: &gpui::Window) -> Option<u32> {
    #[cfg(target_os = "macos")]
    {
        preview_macos::cg_window_id_from_gpui(window)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        None
    }
}

/// Disable GPUI's macOS "don't paint while occluded" display-link gate.
/// No-op on other platforms. Call before the window is created.
pub fn keep_painting_when_occluded() {
    #[cfg(target_os = "macos")]
    preview_macos::keep_painting_when_occluded();
}

/// Start the display link again now that `occlusionState` always looks visible.
/// Main thread, after a `GPUIWindow` exists.
pub fn restart_occluded_display_link() {
    #[cfg(target_os = "macos")]
    preview_macos::restart_occluded_display_link();
}

/// Capture this host process's window. Call off the UI thread.
pub fn capture_host_window(title: &str, window_id: Option<u32>) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let _ = title;
        // Give the display link one tick after `cx.notify()` dirtied the
        // window. Unfocused GPUI skips `present` unless the invalidator is
        // dirty, so the notify + short wait is what actually puts pixels in
        // the Metal layer while Evalight is in front.
        std::thread::sleep(std::time::Duration::from_millis(50));
        let image = preview_macos::capture_this_process(window_id)?;
        return Some(STANDARD.encode(rgba_to_png(&image)?));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let png = spawn_helper(std::process::id(), Some(title), window_id)?;
        Some(STANDARD.encode(png))
    }
}

#[cfg(not(target_os = "macos"))]
fn spawn_helper(pid: u32, title: Option<&str>, window_id: Option<u32>) -> Option<Vec<u8>> {
    let exe = std::env::current_exe().ok()?;
    let mut cmd = Command::new(exe);
    cmd.arg("--capture-preview")
        .arg("--pid")
        .arg(pid.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env_remove("CLJ_GPUI_PORT");
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        cmd.arg("--title").arg(title);
    }
    if let Some(window_id) = window_id.filter(|id| *id > 0) {
        cmd.arg("--wid").arg(window_id.to_string());
    }
    let mut child = cmd.spawn().ok()?;
    let stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::BufReader::new(stdout).read_to_end(&mut buf);
        buf
    });
    match wait_with_timeout(&mut child, HELPER_TIMEOUT) {
        Some(status) if status.success() => png_or_none(reader.join().ok()?),
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            None
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Option<ExitStatus> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => return None,
        }
    }
}

fn capture_pid(pid: u32, title: Option<&str>, window_id: Option<u32>) -> Option<Vec<u8>> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        #[cfg(target_os = "macos")]
        {
            if let Some(image) = preview_macos::capture_this_process(window_id) {
                return rgba_to_png(&image);
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = window_id;
        }
        if pid == 0 {
            return None;
        }
        let window = select_window(pid, title)?;
        let image = window.capture_image().ok()?;
        rgba_to_png(&image)
    }));
    result.ok().flatten()
}

fn select_window(pid: u32, title: Option<&str>) -> Option<xcap::Window> {
    let windows = xcap::Window::all().ok()?;
    windows
        .into_iter()
        .filter_map(|window| {
            let rank = window_rank(&window, pid, title)?;
            Some((rank, window))
        })
        .max_by_key(|(rank, _)| *rank)
        .map(|(_, window)| window)
}

fn window_rank(window: &xcap::Window, pid: u32, title: Option<&str>) -> Option<(u8, u64)> {
    if window.pid().ok()? != pid {
        return None;
    }
    // xcap's macOS is_minimized is `!kCGWindowIsOnscreen`, which is also true
    // for occluded / other-Space windows. Skip that filter there.
    #[cfg(not(target_os = "macos"))]
    if window.is_minimized().ok()? {
        return None;
    }
    let width = window.width().ok()? as u64;
    let height = window.height().ok()? as u64;
    let area = width.saturating_mul(height);
    if area == 0 {
        return None;
    }
    let name = window.title().ok().unwrap_or_default();
    let title_rank = match title {
        Some(want) if name == want => 3,
        Some(want) if name.eq_ignore_ascii_case(want) => 2,
        Some(want) if name.contains(want) => 1,
        Some(_) => 0,
        None => 1,
    };
    Some((title_rank, area))
}

fn rgba_to_png(image: &image::RgbaImage) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .ok()?;
    png_or_none(buf)
}

fn png_or_none(bytes: Vec<u8>) -> Option<Vec<u8>> {
    (bytes.len() >= 8 && bytes.starts_with(&PNG_MAGIC)).then_some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_capture_args_reads_pid_title_and_wid() {
        let got = parse_capture_args([
            "clj-gpui",
            "--capture-preview",
            "--pid",
            "4242",
            "--title",
            "TodoMVC",
            "--wid",
            "99",
        ]);
        assert_eq!(
            got,
            Some(CaptureRequest {
                pid: 4242,
                title: Some("TodoMVC".into()),
                window_id: Some(99),
            })
        );
    }

    #[test]
    fn parse_capture_args_ignores_normal_host_argv() {
        assert_eq!(parse_capture_args(["clj-gpui", "--protocol-test"]), None);
        assert_eq!(parse_capture_args(["clj-gpui"]), None);
    }

    #[test]
    fn unknown_pid_does_not_panic() {
        assert!(capture_pid(1, Some("clj-gpui-no-such-window"), None).is_none());
    }

    #[test]
    fn rgba_png_has_magic() {
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        let png = rgba_to_png(&image).expect("png");
        assert!(png.starts_with(&PNG_MAGIC));
    }

    #[test]
    fn keep_painting_helpers_do_not_panic() {
        keep_painting_when_occluded();
        restart_occluded_display_link();
    }
}
