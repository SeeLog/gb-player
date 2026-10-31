//! Windows that open beside the main window, so that what they change can be seen in it: the
//! theme editor's and the settings'.

#[cfg(target_os = "macos")]
use tauri::LogicalPosition;
use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

/// The space between the two windows.
const GAP: f64 = 8.0;

/// Give `window` the icon entry drawn for the taskbar. Tauri's default is the 32 px entry, which
/// Windows shrinks to the taskbar's 24 px and so blurs the pixel art.
#[cfg(windows)]
pub fn set_taskbar_icon<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    let size = (24.0 * window.scale_factor()?).round() as u32;
    window.set_icon(tauri::image::Image::from_app_icon_resource(size)?)
}

#[cfg(not(windows))]
pub fn set_taskbar_icon<R: Runtime>(_window: &WebviewWindow<R>) -> tauri::Result<()> {
    Ok(())
}

/// Undo what macOS and the window library do when a window changes: the system's window buttons
/// go back to their own place, as the webview's parent only moves them again when it is drawn,
/// and going in or out of full screen takes the keyboard away from the page.
#[cfg(target_os = "macos")]
pub fn after_window_change<R: Runtime>(window: &tauri::Window<R>) {
    let Ok(ns_window) = window.ns_window() else {
        return;
    };
    // SAFETY: this is the NSWindow of `window`, which keeps it alive; window events come on the
    // main thread.
    let ns_window = unsafe { &*(ns_window as *const objc2_app_kit::NSWindow) };
    if let Some(view) = ns_window.contentView() {
        view.setNeedsDisplay(true);
    }
    if let Some(webview) = window.get_webview_window(window.label()) {
        let _ = AsRef::<tauri::webview::Webview<R>>::as_ref(&webview).set_focus();
    }
}

#[cfg(not(target_os = "macos"))]
pub fn after_window_change<R: Runtime>(_window: &tauri::Window<R>) {}

/// Open the window `label`, or bring it to the front when it is open.
pub fn open<R: Runtime>(
    handle: &AppHandle<R>,
    label: &str,
    title: &str,
    size: LogicalSize<f64>,
    min_size: LogicalSize<f64>,
) -> tauri::Result<()> {
    if let Some(window) = handle.get_webview_window(label) {
        window.unminimize()?;
        return window.set_focus();
    }
    // Shown once it is in place. No system frame, as the main window: the page's own header
    // is its title bar, under the system's window buttons on macOS.
    let builder = WebviewWindowBuilder::new(handle, label, WebviewUrl::App("index.html".into()))
        .title(title)
        .inner_size(size.width, size.height)
        .min_inner_size(min_size.width, min_size.height)
        .decorations(cfg!(target_os = "macos"))
        .visible(false);
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(LogicalPosition::new(14.0, 24.0));
    let window = builder.build()?;
    set_taskbar_icon(&window)?;
    if let Some(main) = handle.get_webview_window("main") {
        place_beside(&window, &main, size)?;
    }
    window.show()?;
    window.set_focus()
}

/// Put `window` to the right of `main`, or to its left where there is no room on the right, at
/// the same height and no taller than the screen; over the edge of `main` where neither fits.
fn place_beside<R: Runtime>(
    window: &WebviewWindow<R>,
    main: &WebviewWindow<R>,
    size: LogicalSize<f64>,
) -> tauri::Result<()> {
    let Some(monitor) = main.current_monitor()? else {
        return Ok(());
    };
    let area = monitor.work_area();
    let scale = window.scale_factor()?;
    let gap = (GAP * scale).round() as i32;
    let (at, main_size) = (main.outer_position()?, main.outer_size()?);
    window.set_size(LogicalSize::new(
        size.width,
        size.height.min(area.size.height as f64 / scale),
    ))?;
    let outer = window.outer_size()?;
    let (width, height) = (outer.width as i32, outer.height as i32);
    let (left, right) = (area.position.x, area.position.x + area.size.width as i32);
    let x = if at.x + main_size.width as i32 + gap + width <= right {
        at.x + main_size.width as i32 + gap
    } else if at.x - gap - width >= left {
        at.x - gap - width
    } else {
        right - width
    };
    let bottom = area.position.y + area.size.height as i32;
    let y =
        at.y.clamp(area.position.y, (bottom - height).max(area.position.y));
    window.set_position(PhysicalPosition::new(x, y))
}
