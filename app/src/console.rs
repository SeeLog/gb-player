//! Console access for the CLI in Windows release builds.

/// Release builds on Windows use the GUI subsystem, so the CLI has to borrow the console of
/// the shell it was started from to print anything.
#[cfg(windows)]
pub fn attach_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
pub fn attach_console() {}
