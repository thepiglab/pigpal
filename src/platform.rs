//! Keep the operating system from throttling an active capture.
//!
//! On macOS, App Nap slows timers and lowers the priority of apps whose windows
//! are hidden, minimized or occluded. A recording must keep receiving and saving
//! monitor data in all of those states, so while a capture is running we hold a
//! user-initiated, latency-critical activity. That also prevents idle system
//! sleep (the display may still sleep). Dropping the guard ends the activity.

#[cfg(target_os = "macos")]
pub struct Activity {
    token: objc2::rc::Retained<objc2_foundation::NSObject>,
}

#[cfg(target_os = "macos")]
impl Activity {
    pub fn begin(reason: &str) -> Option<Self> {
        use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
        let reason = NSString::from_str(reason);
        let options = NSActivityOptions::NSActivityUserInitiated
            | NSActivityOptions::NSActivityLatencyCritical;
        // SAFETY: documented Foundation API; the returned token is retained by
        // `Retained` and handed back to `endActivity` exactly once on drop.
        let token = unsafe {
            NSProcessInfo::processInfo().beginActivityWithOptions_reason(options, &reason)
        };
        Some(Self { token })
    }
}

#[cfg(target_os = "macos")]
impl Drop for Activity {
    fn drop(&mut self) {
        // SAFETY: `token` came from `beginActivityWithOptions:reason:` above.
        unsafe { objc2_foundation::NSProcessInfo::processInfo().endActivity(&self.token) };
    }
}

/// Other platforms do not nap background apps in the same way.
#[cfg(not(target_os = "macos"))]
pub struct Activity;

#[cfg(not(target_os = "macos"))]
impl Activity {
    pub fn begin(_reason: &str) -> Option<Self> {
        Some(Self)
    }
}
