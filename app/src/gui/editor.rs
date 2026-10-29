//! The theme editor's window, put beside the main window so the theme can be seen whole while
//! it is changed. The editor sends its draft to the main window (the `theme-draft` event), and
//! the main window goes back to the chosen theme when the editor's window is gone.

use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

/// The label of the editor's window, which the web UI also goes by.
pub const LABEL: &str = "theme-editor";
/// The event that carries the draft (or `null` once the editor is gone) to the main window.
pub const DRAFT_EVENT: &str = "theme-draft";

const SIZE: LogicalSize<f64> = LogicalSize {
    width: 400.0,
    height: 760.0,
};
const MIN_SIZE: LogicalSize<f64> = LogicalSize {
    width: 380.0,
    height: 420.0,
};
/// The space between the two windows.
const GAP: f64 = 8.0;

/// Open the editor's window, or bring it to the front when it is open.
pub fn open<R: Runtime>(handle: &AppHandle<R>, title: &str) -> tauri::Result<()> {
    if let Some(window) = handle.get_webview_window(LABEL) {
        window.unminimize()?;
        return window.set_focus();
    }
    // Shown once it is in place. No system frame, as the main window: the editor's own header
    // is its title bar.
    let window = WebviewWindowBuilder::new(handle, LABEL, WebviewUrl::App("index.html".into()))
        .title(title)
        .inner_size(SIZE.width, SIZE.height)
        .min_inner_size(MIN_SIZE.width, MIN_SIZE.height)
        .decorations(false)
        .visible(false)
        .build()?;
    if let Some(main) = handle.get_webview_window("main") {
        place_beside(&window, &main)?;
    }
    window.show()?;
    window.set_focus()
}

/// Put `window` to the right of `main`, or to its left where there is no room on the right, at
/// the same height and no taller than the screen; over the edge of `main` where neither fits.
fn place_beside<R: Runtime>(
    window: &WebviewWindow<R>,
    main: &WebviewWindow<R>,
) -> tauri::Result<()> {
    let Some(monitor) = main.current_monitor()? else {
        return Ok(());
    };
    let area = monitor.work_area();
    let scale = window.scale_factor()?;
    let gap = (GAP * scale).round() as i32;
    let (at, size) = (main.outer_position()?, main.outer_size()?);
    window.set_size(LogicalSize::new(
        SIZE.width,
        SIZE.height.min(area.size.height as f64 / scale),
    ))?;
    let outer = window.outer_size()?;
    let (width, height) = (outer.width as i32, outer.height as i32);
    let (left, right) = (area.position.x, area.position.x + area.size.width as i32);
    let x = if at.x + size.width as i32 + gap + width <= right {
        at.x + size.width as i32 + gap
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
