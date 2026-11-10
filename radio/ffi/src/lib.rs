//! The app's door into the node: three C functions and JSON across them.
//! No rights reserved.

use std::{
    ffi::{CStr, CString, c_char},
    sync::OnceLock,
};

use pyrate_node::{Config, Node};

static NODE: OnceLock<Node> = OnceLock::new();
static RELEASE_KEYS: std::sync::Mutex<Vec<[u8; 32]>> = std::sync::Mutex::new(Vec::new());

fn release_keys() -> Vec<[u8; 32]> {
    RELEASE_KEYS.lock().unwrap().clone()
}

/// The release keys this build trusts, one hex key per line. Call before `pyrate_start`.
#[unsafe(no_mangle)]
pub extern "C" fn pyrate_set_release_keys(text: *const c_char) {
    *RELEASE_KEYS.lock().unwrap() = text_of(text).lines().filter_map(|l| pyrate_node::store_hex32(l.trim())).collect();
}

/// Runs one call across the door. A panic in the node becomes an answer the app can show,
/// never an abort of the whole app: unwinding out of an `extern` function ends the process.
fn guard<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or(fallback)
}

fn answer(req: &str) -> String {
    guard(serde_json::json!({ "error": "the node stumbled on that; try again" }).to_string(), || {
        match (NODE.get(), serde_json::from_str::<serde_json::Value>(req)) {
            (Some(n), Ok(v)) => n.call(&v),
            (None, _) => serde_json::json!({ "error": "node not started" }),
            (_, Err(e)) => serde_json::json!({ "error": e.to_string() }),
        }
        .to_string()
    })
}

fn pixels(hash: &str) -> Option<(u32, u32, Vec<u8>)> {
    guard(None, || NODE.get().and_then(|n| n.art_pixels(hash)))
}

/// The device's hold key, as 64 hex characters, unwrapped by the phone's secure hardware.
/// Call before `pyrate_start`; without it the node keeps a key beside its data.
#[unsafe(no_mangle)]
pub extern "C" fn pyrate_set_hold_key(hex: *const c_char) {
    guard((), || {
        if let Some(k) = pyrate_node::store_hex32(text(hex).trim()) {
            pyrate_node::set_hold_key(k);
        }
    })
}

fn text_of(p: *const c_char) -> String {
    text(p)
}
static START_MUTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn text(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// Starts the node once. Returns 0 on success, 1 if already running, -1 on failure.
#[unsafe(no_mangle)]
pub extern "C" fn pyrate_start(dir: *const c_char, bootstrap: *const c_char) -> i32 {
    guard(-1, || start(dir, bootstrap))
}

fn start(dir: *const c_char, bootstrap: *const c_char) -> i32 {
    if NODE.get().is_some() {
        return 1;
    }
    let dir = text(dir);
    let bootstrap = text(bootstrap).split(|c: char| c == ',' || c.is_whitespace()).filter_map(|s| s.parse().ok()).collect();
    let cfg = Config {
        dir: dir.into(),
        quota: 2_000_000_000,
        lantern: false,
        bootstrap,
        public_dns: cfg!(target_os = "android"),
        private: true,
        ballast: 1_000_000_000,
        carry_all: false, release_keys: release_keys(),
    };
    match Node::start(cfg) {
        Ok(n) => {
            if START_MUTED.load(std::sync::atomic::Ordering::SeqCst) {
                n.call(&serde_json::json!({ "cmd": "volume", "value": 0.0 }));
                pyrate_node::say("started muted, for testing");
            }
            let _ = NODE.set(n);
            0
        }
        Err(e) => {
            eprintln!("pyrate: {e:#}");
            -1
        }
    }
}

/// One JSON request in, one JSON answer out. Free the answer with `pyrate_free`.
#[unsafe(no_mangle)]
pub extern "C" fn pyrate_call(req: *const c_char) -> *mut c_char {
    CString::new(answer(&text(req))).unwrap_or_default().into_raw()
}

#[unsafe(no_mangle)]
pub extern "C" fn pyrate_free(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

/// A picture for the app, decoded here in memory-safe code: 8 bytes of width and height
/// (little-endian u32 each), then RGBA. Null if the picture has not arrived. Free with
/// `pyrate_free_bytes`, passing the length written to `len`.
#[unsafe(no_mangle)]
pub extern "C" fn pyrate_art(hash: *const c_char, len: *mut usize) -> *mut u8 {
    let Some((w, h, px)) = pixels(&text(hash)) else { return std::ptr::null_mut() };
    let mut out = Vec::with_capacity(8 + px.len());
    out.extend(w.to_le_bytes());
    out.extend(h.to_le_bytes());
    out.extend(px);
    let mut b = out.into_boxed_slice();
    unsafe { *len = b.len() };
    let p = b.as_mut_ptr();
    std::mem::forget(b);
    p
}

#[unsafe(no_mangle)]
pub extern "C" fn pyrate_free_bytes(p: *mut u8, len: usize) {
    if !p.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(p, len)) });
    }
}

/// Hands Rust the Android context, which the audio output needs.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_black_hostis_pyrate_MainActivity_initNative(
    env: jni::JNIEnv,
    _class: jni::objects::JClass,
    context: jni::objects::JObject,
) {
    // Once a process: the Activity and the playback service may both ask, and a second
    // initialisation would trip ndk-context's assertion and abort the app.
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let Ok(vm) = env.get_java_vm() else { return };
        let Ok(ctx) = env.new_global_ref(context) else { return };
        unsafe {
            ndk_context::initialize_android_context(vm.get_java_vm_pointer() as *mut _, ctx.as_obj().as_raw() as *mut _);
        }
        std::mem::forget(ctx);
    });
}

/// Test launches pass `--ez mute true`, so the whole chain runs without a sound.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_black_hostis_pyrate_MainActivity_setStartMuted(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
    muted: jni::sys::jboolean,
) {
    START_MUTED.store(muted != 0, std::sync::atomic::Ordering::SeqCst);
}

/// The same door for Android's own side (the lock screen, Bluetooth, the car, the alarm),
/// which may need the node while the app's screen is closed.
#[cfg(target_os = "android")]
mod android {
    use jni::JNIEnv;
    use jni::objects::{JClass, JString};
    use jni::sys::{jbyteArray, jint, jstring};

    fn s(env: &mut JNIEnv, j: &JString) -> String {
        env.get_string(j).map(|v| v.into()).unwrap_or_default()
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_black_hostis_pyrate_Native_setReleaseKeys(mut env: JNIEnv, _c: JClass, keys: JString) {
        let text = s(&mut env, &keys);
        *super::RELEASE_KEYS.lock().unwrap() = text.lines().filter_map(|l| pyrate_node::store_hex32(l.trim())).collect();
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_black_hostis_pyrate_Native_setHoldKey(mut env: JNIEnv, _c: JClass, key: JString) {
        let text = s(&mut env, &key);
        if let Some(k) = pyrate_node::store_hex32(text.trim()) {
            pyrate_node::set_hold_key(k);
        }
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_black_hostis_pyrate_Native_start(mut env: JNIEnv, _c: JClass, dir: JString, boot: JString) -> jint {
        let dir = std::ffi::CString::new(s(&mut env, &dir)).unwrap_or_default();
        let boot = std::ffi::CString::new(s(&mut env, &boot)).unwrap_or_default();
        super::pyrate_start(dir.as_ptr(), boot.as_ptr())
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_black_hostis_pyrate_Native_call(mut env: JNIEnv, _c: JClass, req: JString) -> jstring {
        let req = s(&mut env, &req);
        env.new_string(super::answer(&req)).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut())
    }

    /// Width and height (4 bytes each, big-endian) then RGBA, or null.
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_black_hostis_pyrate_Native_art(mut env: JNIEnv, _c: JClass, hash: JString) -> jbyteArray {
        let h = s(&mut env, &hash);
        let Some((w, ht, px)) = super::pixels(&h) else { return std::ptr::null_mut() };
        let mut out = Vec::with_capacity(8 + px.len());
        out.extend(w.to_be_bytes());
        out.extend(ht.to_be_bytes());
        out.extend(px);
        env.byte_array_from_slice(&out).map(|a| a.into_raw()).unwrap_or(std::ptr::null_mut())
    }
}
