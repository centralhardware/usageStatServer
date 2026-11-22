//! The macOS calls: frontmost app (AppKit), window title (Accessibility), idle time and lock
//! state (CoreGraphics).

use crate_shim::*;
use std::time::Duration;
use usage_agent::tracker::FrontApp;

mod crate_shim {
    pub use core_foundation::base::{CFType, TCFType};
    pub use core_foundation::boolean::CFBoolean;
    pub use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    pub use core_foundation::string::{CFString, CFStringRef};
    pub use core_foundation_sys::base::CFTypeRef;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(element: CFTypeRef, attribute: CFStringRef, value: *mut CFTypeRef) -> i32;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
    fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
}

/// Whether this process may read other apps' windows; `prompt` asks the user once.
pub fn accessibility_trusted(prompt: bool) -> bool {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let value = if prompt { CFBoolean::true_value() } else { CFBoolean::false_value() };
        let opts = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
        AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef())
    }
}

/// Runs the main run loop for up to `d`, so AppKit sees workspace changes; sleeps when it has
/// nothing to wait on.
pub fn pump_run_loop(d: Duration) {
    use core_foundation::runloop::{CFRunLoop, kCFRunLoopDefaultMode};
    let started = std::time::Instant::now();
    unsafe { CFRunLoop::run_in_mode(kCFRunLoopDefaultMode, d, false) };
    if let Some(rest) = d.checked_sub(started.elapsed()) {
        std::thread::sleep(rest);
    }
}

pub fn front_app() -> Option<FrontApp> {
    use objc2_app_kit::NSWorkspace;
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let bundle_id = app.bundleIdentifier()?.to_string();
    let name = app.localizedName().map_or_else(|| bundle_id.clone(), |n| n.to_string());
    let title = window_title(app.processIdentifier());
    Some(FrontApp { bundle_id, name, title })
}

/// The focused window's title, or "" without permission or a window.
fn window_title(pid: i32) -> String {
    unsafe {
        let el = AXUIElementCreateApplication(pid);
        if el.is_null() {
            return String::new();
        }
        let el = CFType::wrap_under_create_rule(el);
        let mut win: CFTypeRef = std::ptr::null();
        let attr = CFString::from_static_string("AXFocusedWindow");
        if AXUIElementCopyAttributeValue(el.as_CFTypeRef(), attr.as_concrete_TypeRef(), &mut win) != 0 || win.is_null() {
            return String::new();
        }
        let win = CFType::wrap_under_create_rule(win);
        let mut title: CFTypeRef = std::ptr::null();
        let attr = CFString::from_static_string("AXTitle");
        if AXUIElementCopyAttributeValue(win.as_CFTypeRef(), attr.as_concrete_TypeRef(), &mut title) != 0 || title.is_null() {
            return String::new();
        }
        let title = CFType::wrap_under_create_rule(title);
        title.downcast::<CFString>().map(|s| s.to_string().chars().take(1024).collect()).unwrap_or_default()
    }
}

/// Seconds since the last keyboard or mouse input in this session.
pub fn idle_seconds() -> f64 {
    // kCGEventSourceStateCombinedSessionState, kCGAnyInputEventType
    unsafe { CGEventSourceSecondsSinceLastEventType(0, u32::MAX) }
}

pub fn screen_locked() -> bool {
    unsafe {
        let d = CGSessionCopyCurrentDictionary();
        if d.is_null() {
            return false;
        }
        let d: CFDictionary<CFString, CFType> = CFDictionary::wrap_under_create_rule(d);
        d.find(CFString::from_static_string("CGSSessionScreenIsLocked"))
            .and_then(|v| v.downcast::<CFBoolean>())
            .is_some_and(bool::from)
    }
}

pub fn device_name() -> String {
    objc2_foundation::NSHost::currentHost().localizedName().map_or_else(|| "Mac".into(), |n| n.to_string())
}
