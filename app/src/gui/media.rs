//! The OS media controls: media keys, headset buttons and the system's now-playing display
//! (System Media Transport Controls on Windows, MPRIS on Linux, Now Playing on macOS). Their
//! buttons run the same commands as the UI's, and the ticker keeps the song they show current.

use std::sync::{Mutex, PoisonError};

use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig};
use tauri::{AppHandle, Manager, WebviewWindow};

use super::{commands, App};

/// Whether the song in the audio output plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Playback {
    Stopped,
    Paused,
    Playing,
}

/// What the OS shows: the song, its game (or folder) and whether it plays. The names are empty
/// while nothing was played.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NowPlaying {
    pub title: String,
    pub game: String,
    pub playback: Playback,
}

/// The media controls, if the OS has them.
pub struct Media(Mutex<Option<MediaControls>>);

impl Media {
    /// Attach to the OS media controls for `window`; without them the app works as before.
    pub fn new(window: Option<&WebviewWindow>) -> Media {
        let controls = window.map(attach).transpose();
        Media(Mutex::new(controls.unwrap_or_else(|e| {
            eprintln!("media controls: {e}");
            None
        })))
    }
}

fn attach(window: &WebviewWindow) -> Result<MediaControls, String> {
    #[cfg(windows)]
    let hwnd = Some(window.hwnd().map_err(|e| e.to_string())?.0);
    #[cfg(not(windows))]
    let hwnd = None;
    let config = PlatformConfig {
        display_name: "GB Player",
        dbus_name: "gb_player",
        hwnd,
    };
    let mut controls = MediaControls::new(config).map_err(|e| format!("{e:?}"))?;
    let handle = window.app_handle().clone();
    controls
        .attach(move |event| on_event(&handle, event))
        .map_err(|e| format!("{e:?}"))?;
    // macOS only takes a player that has played for one that can wait for the media keys. The
    // first update of the ticker corrects this right away.
    if cfg!(target_os = "macos") {
        controls
            .set_playback(MediaPlayback::Playing { progress: None })
            .map_err(|e| format!("{e:?}"))?;
    }
    Ok(controls)
}

fn on_event(handle: &AppHandle, event: MediaControlEvent) {
    let playing = || handle.state::<App>().lock().now_playing().playback == Playback::Playing;
    match event {
        MediaControlEvent::Toggle => commands::toggle(handle.state()),
        MediaControlEvent::Play if !playing() => commands::toggle(handle.state()),
        MediaControlEvent::Pause if playing() => commands::toggle(handle.state()),
        MediaControlEvent::Next => commands::skip(handle.clone(), handle.state(), 1),
        MediaControlEvent::Previous => commands::skip(handle.clone(), handle.state(), -1),
        MediaControlEvent::Stop => commands::stop(handle.state()),
        MediaControlEvent::Raise => {
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
        _ => {}
    }
}

/// Show `now` in the OS. On the main thread, where macOS wants it.
pub fn show(handle: &AppHandle, now: NowPlaying) {
    let main = handle.clone();
    let _ = handle.run_on_main_thread(move || {
        let media = main.state::<Media>();
        let mut controls = media.0.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(controls) = controls.as_mut() else {
            return;
        };
        // Empty names rather than none, which Windows would take as keeping the old ones.
        let metadata = MediaMetadata {
            title: Some(&now.title),
            album: Some(&now.game),
            artist: Some(&now.game),
            ..MediaMetadata::default()
        };
        let playback = match now.playback {
            // macOS gives the media keys to a player that isn't stopped, and with none it opens
            // Apple Music. Toggling starts a song here, so wait for the key as if paused.
            Playback::Stopped if cfg!(target_os = "macos") => {
                MediaPlayback::Paused { progress: None }
            }
            Playback::Stopped => MediaPlayback::Stopped,
            Playback::Paused => MediaPlayback::Paused { progress: None },
            Playback::Playing => MediaPlayback::Playing { progress: None },
        };
        if let Err(e) = controls
            .set_metadata(metadata)
            .and_then(|()| controls.set_playback(playback))
        {
            eprintln!("media controls: {e:?}");
        }
    });
}
