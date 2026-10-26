//! Background loop-point analysis for every song of the loaded ROM.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

use gb_music::{analyze, Driver, Song, SongLength, ANALYSIS_FRAMES};

/// Finds song loop points in the background, current song first.
pub struct Analyzer {
    queue: Arc<Mutex<VecDeque<usize>>>,
    pub results: Receiver<(usize, SongLength)>,
    cancel: Arc<AtomicBool>,
}

impl Analyzer {
    pub fn start(rom: Arc<[u8]>, driver: Driver) -> Analyzer {
        let queue = Arc::new(Mutex::new((0..driver.songs.len()).collect::<VecDeque<_>>()));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, results) = channel();
        let (q, c) = (queue.clone(), cancel.clone());
        std::thread::spawn(move || loop {
            if c.load(Ordering::Relaxed) {
                return;
            }
            let Some(index) = q.lock().unwrap().pop_front() else {
                return;
            };
            let song: &Song = &driver.songs[index];
            let length =
                analyze(&rom, &driver, song, ANALYSIS_FRAMES).unwrap_or(SongLength::Unknown);
            if tx.send((index, length)).is_err() {
                return;
            }
        });
        Analyzer {
            queue,
            results,
            cancel,
        }
    }

    pub fn prioritize(&self, index: usize) {
        let mut q = self.queue.lock().unwrap();
        if let Some(pos) = q.iter().position(|&i| i == index) {
            q.remove(pos);
            q.push_front(index);
        }
    }
}

impl Drop for Analyzer {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
