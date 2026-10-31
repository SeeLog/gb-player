//! The mini player: the main window shrunk to a handheld console with the song on its screen
//! and the transport on its buttons, above other windows if wanted. The web UI draws it from
//! `LibraryView::mini`; this module only changes the window. Where the mini player was, and the
//! place and size of the full window, are kept in the settings, so each comes back where it
//! was, also after a restart.

use tauri::webview::Webview;
use tauri::{LogicalSize, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

use crate::settings::{Bounds, Mini};

/// The smallest the full window may be, as in `tauri.conf.json`.
const FULL_MIN: LogicalSize<f64> = LogicalSize {
    width: 760.0,
    height: 600.0,
};
/// The size of the full window when none was kept, as in `tauri.conf.json`.
const FULL_SIZE: LogicalSize<f64> = LogicalSize {
    width: 1040.0,
    height: 760.0,
};
/// The mini player's size, which cannot be changed: it is drawn to fit.
const MINI_SIZE: LogicalSize<f64> = LogicalSize {
    width: 288.0,
    height: 468.0,
};

/// Whether bounds are kept in points. macOS places windows in points, the same on every screen;
/// its physical pixels mean something else on each screen, so a window that is created on one
/// screen and moved to another would end up in the wrong place at the wrong size.
const KEPT_IN_POINTS: bool = cfg!(target_os = "macos");

/// How many physical pixels one unit of kept bounds is at `scale`.
fn kept_unit(scale: f64) -> f64 {
    if KEPT_IN_POINTS {
        scale
    } else {
        1.0
    }
}

/// `value` in kept bounds, in physical pixels at `unit`.
fn to_physical(value: f64, unit: f64) -> f64 {
    (value * unit).round()
}

/// Where `window` is and how big, unless it is minimized (and so has no place of its own).
fn bounds<R: Runtime>(window: &WebviewWindow<R>) -> Option<Bounds> {
    if window.is_minimized().unwrap_or(false) {
        return None;
    }
    let at = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    let unit = kept_unit(window.scale_factor().ok()?);
    Some(Bounds {
        x: (at.x as f64 / unit).round() as i32,
        y: (at.y as f64 / unit).round() as i32,
        width: (size.width as f64 / unit).round() as u32,
        height: (size.height as f64 / unit).round() as u32,
        maximized: false,
    })
}

/// Move `window` to where `bounds` says, when that is still on a screen (one may have been
/// unplugged since).
fn place<R: Runtime>(window: &WebviewWindow<R>, bounds: &Bounds) -> tauri::Result<()> {
    let on_screen = window.available_monitors()?.iter().any(|m| {
        let unit = kept_unit(m.scale_factor());
        let (at, size) = (m.position(), m.size());
        let (left, top) = (at.x as f64 / unit, at.y as f64 / unit);
        let (width, height) = (size.width as f64 / unit, size.height as f64 / unit);
        // The top left corner, a little in, so the window can still be grabbed.
        let (x, y) = ((bounds.x + 40) as f64, (bounds.y + 10) as f64);
        x >= left && y >= top && x < left + width && y < top + height
    });
    if on_screen {
        let unit = kept_unit(window.scale_factor()?);
        window.set_position(PhysicalPosition::new(
            to_physical(bounds.x as f64, unit) as i32,
            to_physical(bounds.y as f64, unit) as i32,
        ))?;
    }
    Ok(())
}

/// Give `window` the size in `bounds`.
fn resize<R: Runtime>(window: &WebviewWindow<R>, bounds: &Bounds) -> tauri::Result<()> {
    let unit = kept_unit(window.scale_factor()?);
    window.set_size(PhysicalSize::new(
        to_physical(bounds.width as f64, unit) as u32,
        to_physical(bounds.height as f64, unit) as u32,
    ))
}

/// Show the system's window buttons over the page's title bar, or hide them for the mini player,
/// which draws its own. Only macOS has them (see tauri.macos.conf.json); other systems have the
/// page's buttons instead. The frame itself stays: taking it away with `set_decorations` would
/// also drop the page's place under the title bar, and it would not reliably come back.
#[cfg(target_os = "macos")]
fn set_system_buttons<R: Runtime>(window: &WebviewWindow<R>, on: bool) -> tauri::Result<()> {
    use objc2_app_kit::{NSWindow, NSWindowButton};

    let target = window.clone();
    window.run_on_main_thread(move || {
        let Ok(ns_window) = target.ns_window() else {
            return;
        };
        // SAFETY: this is the NSWindow of `target`, which keeps it alive, on the main thread.
        let ns_window = unsafe { &*(ns_window as *const NSWindow) };
        for button in [
            NSWindowButton::CloseButton,
            NSWindowButton::MiniaturizeButton,
            NSWindowButton::ZoomButton,
        ] {
            if let Some(button) = ns_window.standardWindowButton(button) {
                button.setHidden(!on);
            }
        }
    })
}

#[cfg(not(target_os = "macos"))]
fn set_system_buttons<R: Runtime>(_window: &WebviewWindow<R>, _on: bool) -> tauri::Result<()> {
    Ok(())
}

/// Shape `window` as the mini player, where it last was.
pub fn show_mini<R: Runtime>(window: &WebviewWindow<R>, mini: &Mini) -> tauri::Result<()> {
    // Not resizable at all: the size limits alone still let the edges of a window without a
    // frame be dragged a little on Windows, and the new size would stay.
    window.set_resizable(false)?;
    window.set_maximizable(false)?;
    set_system_buttons(window, false)?;
    // The window is transparent (see tauri.conf.json) and the page draws the console with the
    // desktop around it; the shadow would draw a box around that.
    window.set_shadow(false)?;
    window.set_min_size(Some(MINI_SIZE))?;
    window.set_max_size(Some(MINI_SIZE))?;
    window.set_size(MINI_SIZE)?;
    window.set_always_on_top(mini.on_top)?;
    if let Some(b) = &mini.bounds {
        place(window, b)?;
    }
    Ok(())
}

/// Turn the full window into the mini player, or back.
pub fn set<R: Runtime>(window: &WebviewWindow<R>, mini: &mut Mini, on: bool) -> tauri::Result<()> {
    if on == mini.on {
        return Ok(());
    }
    if on {
        // macOS animates out of full screen and the window can't change shape meanwhile, so this
        // only leaves it; the shortcut once more makes the mini player.
        if window.is_fullscreen()? {
            return window.set_fullscreen(false);
        }
        let maximized = window.is_maximized()?;
        if maximized {
            window.unmaximize()?;
        }
        if window.is_minimized()? {
            window.unminimize()?;
        }
        mini.full = bounds(window).map(|b| Bounds { maximized, ..b });
        mini.on = true;
        show_mini(window, mini)?;
    } else {
        remember(window, mini);
        mini.on = false;
        show_full(window, mini.full)?;
    }
    // Changing the window's style on macOS takes the keyboard away from the page.
    AsRef::<Webview<R>>::as_ref(window).set_focus()
}

/// Shape `window` as the full window again, at `full` when it was kept and at the default size
/// otherwise. Every step is tried even when one fails, so the window is never left with the mini
/// player's size; the first error is returned.
pub fn show_full<R: Runtime>(window: &WebviewWindow<R>, full: Option<Bounds>) -> tauri::Result<()> {
    let results = [
        window.set_always_on_top(false),
        window.set_max_size(None::<LogicalSize<f64>>),
        window.set_min_size(Some(FULL_MIN)),
        window.set_resizable(true),
        window.set_maximizable(true),
        set_system_buttons(window, true),
        window.set_shadow(true),
        match full {
            Some(b) => resize(window, &b).and_then(|_| place(window, &b)),
            None => window.set_size(FULL_SIZE),
        },
        match full {
            Some(b) if b.maximized => window.maximize(),
            _ => Ok(()),
        },
    ];
    results.into_iter().collect()
}

/// Start the full window where it was, if that is kept here rather than by the window-state
/// plugin; and at a usable size if it is smaller than the full window may be, as when the mini
/// player's size was left behind.
pub fn check_full<R: Runtime>(window: &WebviewWindow<R>, mini: &Mini) -> tauri::Result<()> {
    if KEPT_IN_POINTS && mini.full.is_some() {
        return show_full(window, mini.full);
    }
    let size = window
        .inner_size()?
        .to_logical::<f64>(window.scale_factor()?);
    if size.width < FULL_MIN.width || size.height < FULL_MIN.height {
        show_full(window, mini.full)?;
    }
    Ok(())
}

/// What to come back to from a window maximized over `now`: the place and size it had before, on
/// the screen it is maximized on, as it may have been moved to another one since.
fn before_maximized(before: Bounds, now: Bounds) -> Bounds {
    let inside = before.x >= now.x
        && before.y >= now.y
        && before.x < now.x + now.width as i32
        && before.y < now.y + now.height as i32;
    if inside {
        before
    } else {
        Bounds {
            x: now.x,
            y: now.y,
            ..before
        }
    }
}

/// Keep where the mini player is now, when it is the mini player; and where the full window is,
/// where the window-state plugin doesn't (see `KEPT_IN_POINTS`). One in full screen keeps only
/// what it had before.
pub fn remember<R: Runtime>(window: &WebviewWindow<R>, mini: &mut Mini) {
    if mini.on {
        if let Some(b) = bounds(window) {
            mini.bounds = Some(b);
        }
    } else if KEPT_IN_POINTS && !window.is_fullscreen().unwrap_or(false) {
        let maximized = window.is_maximized().unwrap_or(false);
        let kept = match (mini.full.filter(|_| maximized), bounds(window)) {
            (Some(before), Some(now)) => Some(before_maximized(before, now)),
            (before, now) => now.or(before),
        };
        if let Some(b) = kept {
            mini.full = Some(Bounds { maximized, ..b });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32, width: u32, height: u32) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
            maximized: false,
        }
    }

    #[test]
    fn before_maximized_stays_on_the_screen_it_is_maximized_on() {
        let screen = at(3440, 30, 2560, 1410);
        // Already there: unchanged.
        let there = at(4200, 193, 1040, 760);
        assert_eq!(before_maximized(there, screen), there);
        // On another screen: the same size, moved here.
        let elsewhere = at(1200, 193, 1040, 760);
        assert_eq!(before_maximized(elsewhere, screen), at(3440, 30, 1040, 760));
    }
}
