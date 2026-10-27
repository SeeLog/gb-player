//! The mini player: the main window shrunk to a handheld console with the song on its screen
//! and the transport on its buttons, above other windows if wanted. The web UI draws it from
//! `LibraryView::mini`; this module only changes the window. Where the mini player was, and the
//! place and size of the full window, are kept in the settings, so each comes back where it
//! was, also after a restart.

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

/// Where `window` is and how big, unless it is minimized (and so has no place of its own).
fn bounds<R: Runtime>(window: &WebviewWindow<R>) -> Option<Bounds> {
    if window.is_minimized().unwrap_or(false) {
        return None;
    }
    let at = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    Some(Bounds {
        x: at.x,
        y: at.y,
        width: size.width,
        height: size.height,
        maximized: false,
    })
}

/// Move `window` to where `bounds` says, when that is still on a screen (one may have been
/// unplugged since).
fn place<R: Runtime>(window: &WebviewWindow<R>, bounds: &Bounds) -> tauri::Result<()> {
    let on_screen = window.available_monitors()?.iter().any(|m| {
        let (at, size) = (m.position(), m.size());
        // The top left corner, a little in, so the window can still be grabbed.
        let (x, y) = (bounds.x + 40, bounds.y + 10);
        x >= at.x && y >= at.y && x < at.x + size.width as i32 && y < at.y + size.height as i32
    });
    if on_screen {
        window.set_position(PhysicalPosition::new(bounds.x, bounds.y))?;
    }
    Ok(())
}

/// Shape `window` as the mini player, where it last was.
pub fn show_mini<R: Runtime>(window: &WebviewWindow<R>, mini: &Mini) -> tauri::Result<()> {
    // Not resizable at all: the size limits alone still let the edges of a window without a
    // frame be dragged a little on Windows, and the new size would stay.
    window.set_resizable(false)?;
    window.set_maximizable(false)?;
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
        let maximized = window.is_maximized()?;
        if maximized {
            window.unmaximize()?;
        }
        if window.is_minimized()? {
            window.unminimize()?;
        }
        mini.full = bounds(window).map(|b| Bounds { maximized, ..b });
        mini.on = true;
        show_mini(window, mini)
    } else {
        remember(window, mini);
        mini.on = false;
        show_full(window, mini.full)
    }
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
        window.set_shadow(true),
        match full {
            Some(b) => window
                .set_size(PhysicalSize::new(b.width, b.height))
                .and_then(|_| place(window, &b)),
            None => window.set_size(FULL_SIZE),
        },
        match full {
            Some(b) if b.maximized => window.maximize(),
            _ => Ok(()),
        },
    ];
    results.into_iter().collect()
}

/// Start the full window at a usable size if it is smaller than the full window may be, as when
/// the mini player's size was left behind.
pub fn check_full<R: Runtime>(window: &WebviewWindow<R>, mini: &Mini) -> tauri::Result<()> {
    let size = window
        .inner_size()?
        .to_logical::<f64>(window.scale_factor()?);
    if size.width < FULL_MIN.width || size.height < FULL_MIN.height {
        show_full(window, mini.full)?;
    }
    Ok(())
}

/// Keep where the mini player is now, when it is the mini player.
pub fn remember<R: Runtime>(window: &WebviewWindow<R>, mini: &mut Mini) {
    if mini.on {
        if let Some(b) = bounds(window) {
            mini.bounds = Some(b);
        }
    }
}
