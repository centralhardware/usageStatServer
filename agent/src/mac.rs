//! The macOS calls: frontmost app (AppKit), window title (Accessibility), idle time and lock
//! state (CoreGraphics).

use crate_shim::*;
use std::time::Duration;
use usage_agent::tracker::{FrontApp, Playing};

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
    let category = app_category(&app, &bundle_id);
    let domain = front_domain(&bundle_id, &title);
    Some(FrontApp { bundle_id, name, title, domain, category })
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

/// The Mac's name as set in System Settings → General → Sharing.
pub fn device_name() -> String {
    std::process::Command::new("/usr/sbin/scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Mac".into())
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const std::ffi::c_char) -> core_foundation_sys::dictionary::CFMutableDictionaryRef;
    fn IOServiceGetMatchingService(main_port: u32, matching: core_foundation_sys::dictionary::CFMutableDictionaryRef) -> u32;
    fn IORegistryEntryCreateCFProperty(entry: u32, key: CFStringRef, allocator: core_foundation_sys::base::CFAllocatorRef, options: u32) -> CFTypeRef;
    fn IOObjectRelease(object: u32) -> i32;
}

/// The Mac's IOPlatformUUID (the same id battery-agent reports), or its name if unreadable.
pub fn device_id() -> String {
    unsafe {
        let service = IOServiceGetMatchingService(0, IOServiceMatching(c"IOPlatformExpertDevice".as_ptr()));
        if service == 0 {
            return device_name();
        }
        let key = CFString::from_static_string("IOPlatformUUID");
        let value = IORegistryEntryCreateCFProperty(service, key.as_concrete_TypeRef(), core_foundation_sys::base::kCFAllocatorDefault, 0);
        IOObjectRelease(service);
        if value.is_null() {
            return device_name();
        }
        CFType::wrap_under_create_rule(value).downcast::<CFString>().map_or_else(device_name, |s| s.to_string())
    }
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPMCopyAssertionsByProcess(by_process: *mut CFDictionaryRef) -> i32;
}

/// Who is playing media, from the power assertions: (pid, owner bundle id if named in the
/// assertion). Players and Chromium take PreventUserIdleDisplaySleep themselves ("Video Wake Lock");
/// Safari's WebKit playback is held by runningboardd and names the app ("WebKit Media Playback").
fn media_assertions() -> Vec<(i32, Option<String>)> {
    use core_foundation::array::CFArray;
    use core_foundation::number::CFNumber;
    let mut raw: CFDictionaryRef = std::ptr::null();
    if unsafe { IOPMCopyAssertionsByProcess(&mut raw) } != 0 || raw.is_null() {
        return Vec::new();
    }
    let by_pid: CFDictionary<CFNumber, CFArray<CFDictionary<CFString, CFType>>> = unsafe { CFDictionary::wrap_under_create_rule(raw) };
    let (pids, lists) = by_pid.get_keys_and_values();
    let text = |a: &CFDictionary<CFString, CFType>, k: &'static str| {
        a.find(CFString::from_static_string(k)).and_then(|v| v.downcast::<CFString>()).map(|s| s.to_string()).unwrap_or_default()
    };
    let mut out = Vec::new();
    for (pid, list) in pids.into_iter().zip(lists) {
        let Some(pid) = unsafe { CFNumber::wrap_under_get_rule(pid as _) }.to_i32() else { continue };
        let list: CFArray<CFDictionary<CFString, CFType>> = unsafe { CFArray::wrap_under_get_rule(list as _) };
        for a in list.iter() {
            if let Some(owner) = usage_agent::tracker::webkit_media_owner(&text(&a, "AssertName")) {
                out.push((pid, Some(owner)));
            } else if text(&a, "AssertType") == "PreventUserIdleDisplaySleep" {
                out.push((pid, None));
            }
        }
    }
    // Named owners (Safari) first: they are certain; a bare display assertion may be anything.
    out.sort_by_key(|(_, o)| o.is_none());
    out
}

/// AppleScript that returns a browser's front tab URL, by bundle id; Firefox has none.
fn tab_url_script(bundle_id: &str) -> Option<&'static str> {
    match bundle_id {
        "com.apple.Safari" | "com.apple.SafariTechnologyPreview" => Some("URL of current tab of front window"),
        "com.google.Chrome" | "com.google.Chrome.canary" | "com.brave.Browser" | "com.microsoft.edgemac"
        | "company.thebrowser.Browser" | "com.vivaldi.Vivaldi" | "com.operasoftware.Opera" => Some("URL of active tab of front window"),
        _ => None,
    }
}

fn tab_domain(bundle_id: &str) -> Option<String> {
    let expr = tab_url_script(bundle_id)?;
    let out = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", &format!("tell application id \"{bundle_id}\" to get {expr}")])
        .output()
        .ok()?;
    usage_agent::tracker::domain(String::from_utf8_lossy(&out.stdout).trim())
}

/// The app playing video now, if any, with the browser tab's domain as its source. A web
/// browser's own helper processes take the assertion, so a pid is mapped to its app bundle.
pub fn playing() -> Option<Playing> {
    use objc2_app_kit::NSRunningApplication;
    for (pid, owner) in media_assertions() {
        let app = match &owner {
            Some(b) => NSRunningApplication::runningApplicationsWithBundleIdentifier(&objc2_foundation::NSString::from_str(b)).firstObject(),
            None => NSRunningApplication::runningApplicationWithProcessIdentifier(pid),
        };
        let Some(app) = app else { continue };
        let Some(bundle) = app.bundleIdentifier().map(|b| b.to_string()) else { continue };
        // Helper processes (com.google.Chrome.helper…) belong to their browser.
        let bundle = tab_url_script_owner(&bundle).unwrap_or(bundle);
        let name = app.localizedName().map_or_else(|| bundle.clone(), |n| n.to_string());
        let source = tab_domain(&bundle).unwrap_or_else(|| name.clone());
        return Some(Playing { bundle_id: bundle, app_name: name, source });
    }
    None
}

/// The browser a helper process belongs to, from its bundle id prefix.
fn tab_url_script_owner(bundle: &str) -> Option<String> {
    ["com.google.Chrome", "com.brave.Browser", "com.microsoft.edgemac", "company.thebrowser.Browser", "com.vivaldi.Vivaldi"]
        .into_iter()
        .find(|b| bundle != *b && bundle.starts_with(b))
        .map(str::to_owned)
}

thread_local! {
    static CATEGORIES: std::cell::RefCell<std::collections::HashMap<String, String>> = Default::default();
    static LAST_DOMAIN: std::cell::RefCell<(String, String, String, bool)> = Default::default();
}

/// LSApplicationCategoryType from the app's Info.plist, cached per bundle id.
fn app_category(app: &objc2_app_kit::NSRunningApplication, bundle_id: &str) -> String {
    if let Some(c) = CATEGORIES.with_borrow(|m| m.get(bundle_id).cloned()) {
        return c;
    }
    let category = app
        .bundleURL()
        .and_then(|u| u.path())
        .and_then(|p| plist::Value::from_file(format!("{p}/Contents/Info.plist")).ok())
        .and_then(|v| v.as_dictionary()?.get("LSApplicationCategoryType")?.as_string().map(str::to_owned))
        .unwrap_or_default();
    CATEGORIES.with_borrow_mut(|m| m.insert(bundle_id.to_owned(), category.clone()));
    category
}

/// The front tab's domain when the app is a browser; asked again only when the title changes,
/// since it costs an osascript run.
fn front_domain(bundle_id: &str, title: &str) -> String {
    if tab_url_script(bundle_id).is_none() {
        return String::new();
    }
    // Asked again when the title changes, and once more on the next sample after that, since
    // the URL can lag the title during a tab switch.
    if let Some(d) = LAST_DOMAIN.with_borrow(|(b, t, d, confirmed)| (*confirmed && b == bundle_id && t == title).then(|| d.clone())) {
        return d;
    }
    let d = tab_domain(bundle_id).unwrap_or_default();
    LAST_DOMAIN.with_borrow_mut(|v| {
        let confirmed = v.0 == bundle_id && v.1 == title;
        *v = (bundle_id.to_owned(), title.to_owned(), d.clone(), confirmed);
    });
    d
}
