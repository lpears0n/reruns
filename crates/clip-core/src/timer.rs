//! Precise sleeps while recording (Windows).
//!
//! GStreamer's capture sources pace themselves by sleeping until the next
//! frame slot, and a sleep is only as fine as the process's timer: 15.6 ms
//! (64 ticks a second) unless the process asks for better. Windows 11
//! also ignores that request for a process with no visible window, which
//! is Reruns in the tray while a game is in front. On a 64 Hz tick a
//! 60 fps source wakes late, lands in the following slot, and captures
//! again at once: the second grab has nothing new and the old picture is
//! sent again. That was the owner's clips freezing for a few frames about
//! five times a second (the beat of 64 against 60), with the source
//! delivering 61–69 frames a second for the 60 asked.
//!
//! [`PreciseTimer`] asks for 1 ms ticks and tells Windows to honour that
//! in the background, for as long as one is held. It costs a little idle
//! power, so it is held only while a capture pipeline exists.

/// Held while recording; see the module docs. Does nothing off Windows.
#[derive(Debug)]
pub struct PreciseTimer(());

impl PreciseTimer {
    pub fn hold() -> Self {
        imp::change(true);
        Self(())
    }
}

impl Drop for PreciseTimer {
    fn drop(&mut self) {
        imp::change(false);
    }
}

#[cfg(windows)]
mod imp {
    use std::sync::Mutex;

    use windows::Win32::{
        Media::{timeBeginPeriod, timeEndPeriod},
        System::Threading::{
            GetCurrentProcess, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
            ProcessPowerThrottling, SetProcessInformation,
        },
    };

    /// `PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION` (Windows 11).
    const IGNORE_TIMER_RESOLUTION: u32 = 0x4;

    /// Holders right now: recordings overlap for a moment when one is
    /// rebuilt, and the last to go must be the one that lets go.
    static HOLDERS: Mutex<usize> = Mutex::new(0);

    pub fn change(hold: bool) {
        let mut holders = HOLDERS.lock().unwrap();
        if hold {
            *holders += 1;
            if *holders == 1 {
                apply(true);
            }
        } else {
            *holders = holders.saturating_sub(1);
            if *holders == 0 {
                apply(false);
            }
        }
    }

    fn apply(on: bool) {
        // With both bits under our control and off, Windows neither slows
        // this process down in the background nor ignores its timer
        // request; with no bits controlled, it decides for itself again.
        let controlled = if on {
            PROCESS_POWER_THROTTLING_EXECUTION_SPEED | IGNORE_TIMER_RESOLUTION
        } else {
            0
        };
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: controlled,
            StateMask: 0,
        };
        // SAFETY: plain Win32 calls on this process with a struct that
        // lives across the call; begin/end periods are paired by HOLDERS.
        unsafe {
            if on {
                timeBeginPeriod(1);
            } else {
                timeEndPeriod(1);
            }
            let size = std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32;
            let set = |state: &PROCESS_POWER_THROTTLING_STATE| {
                SetProcessInformation(
                    GetCurrentProcess(),
                    ProcessPowerThrottling,
                    std::ptr::from_ref(state).cast(),
                    size,
                )
            };
            if set(&state).is_err() && on {
                // Windows 10 doesn't know the timer bit; the speed bit
                // alone still applies there.
                let fallback = PROCESS_POWER_THROTTLING_STATE {
                    ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
                    ..state
                };
                if let Err(e) = set(&fallback) {
                    tracing::warn!("can't opt out of background throttling: {e}");
                }
            }
        }
        tracing::debug!(on, "precise timer");
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn change(_hold: bool) {}
}
