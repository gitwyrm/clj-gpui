//! Snapshot the live native window for Evalight's Preview pane.
//!
//! GPUI does not expose framebuffer readback. The host process (PID A) spawns
//! this same binary as a helper (PID B):
//!
//! ```text
//! clj-gpui --capture-preview --pid <host-pid> [--title <window-title>]
//! ```
//!
//! The helper uses [xcap] to find that PID's window and write a PNG to stdout.
//! A second process is required on Windows: `xcap::Window::all()` skips
//! windows owned by the current process so `GetWindowText` cannot deadlock
//! the GPUI message loop. The parent waits on a background thread, never the
//! UI thread, because Windows capture may `PrintWindow` the host.
//!
//! Failure is empty stdout / `None`. Never write the PNG to the host logs.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::io::{Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const HELPER_TIMEOUT: Duration = Duration::from_secs(5);

/// Parse `--capture-preview --pid N [--title T]` from argv (including argv0).
pub fn parse_capture_args<I, S>(args: I) -> Option<(u32, Option<String>)>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut capture = false;
    let mut pid = None;
    let mut title = None;
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
            _ => {}
        }
    }
    if capture {
        Some((pid.unwrap_or(0), title.filter(|t| !t.is_empty())))
    } else {
        None
    }
}

/// Helper entry: write a PNG to stdout, or nothing. Always succeeds the process.
pub fn run_helper(pid: u32, title: Option<&str>) {
    if let Some(png) = capture_pid(pid, title) {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(&png);
        let _ = out.flush();
    }
}

/// Capture this host process's window from a helper. Call off the UI thread.
pub fn capture_host_window(title: &str) -> Option<String> {
    let png = spawn_helper(std::process::id(), Some(title))?;
    Some(STANDARD.encode(png))
}

fn spawn_helper(pid: u32, title: Option<&str>) -> Option<Vec<u8>> {
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

fn capture_pid(pid: u32, title: Option<&str>) -> Option<Vec<u8>> {
    if pid == 0 {
        return None;
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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
    fn parse_capture_args_reads_pid_and_title() {
        let got = parse_capture_args([
            "clj-gpui",
            "--capture-preview",
            "--pid",
            "4242",
            "--title",
            "TodoMVC",
        ]);
        assert_eq!(got, Some((4242, Some("TodoMVC".into()))));
    }

    #[test]
    fn parse_capture_args_ignores_normal_host_argv() {
        assert_eq!(parse_capture_args(["clj-gpui", "--protocol-test"]), None);
        assert_eq!(parse_capture_args(["clj-gpui"]), None);
    }

    #[test]
    fn unknown_pid_does_not_panic() {
        assert!(capture_pid(1, Some("clj-gpui-no-such-window")).is_none());
    }

    #[test]
    fn rgba_png_has_magic() {
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        let png = rgba_to_png(&image).expect("png");
        assert!(png.starts_with(&PNG_MAGIC));
    }
}
