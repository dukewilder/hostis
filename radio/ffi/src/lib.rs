//! The app's door into the node: three C functions and JSON across them.
//! No rights reserved.

use std::{
    ffi::{CStr, CString, c_char},
    sync::OnceLock,
};

use pyrate_node::{Config, Node};

static NODE: OnceLock<Node> = OnceLock::new();
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
    let answer = match (NODE.get(), serde_json::from_str::<serde_json::Value>(&text(req))) {
        (Some(n), Ok(v)) => n.call(&v),
        (None, _) => serde_json::json!({ "error": "node not started" }),
        (_, Err(e)) => serde_json::json!({ "error": e.to_string() }),
    };
    CString::new(answer.to_string()).unwrap_or_default().into_raw()
}

#[unsafe(no_mangle)]
pub extern "C" fn pyrate_free(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
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
    let Ok(vm) = env.get_java_vm() else { return };
    let Ok(ctx) = env.new_global_ref(context) else { return };
    unsafe {
        ndk_context::initialize_android_context(vm.get_java_vm_pointer() as *mut _, ctx.as_obj().as_raw() as *mut _);
    }
    std::mem::forget(ctx);
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
