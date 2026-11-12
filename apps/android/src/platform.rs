//! Android platform services through JNI: the system clipboard, opening a link in another app, the
//! insets of the system bars and the keyboard, and the soft keyboard's text buffer. The Java half is
//! `packaging/android/java` (`PaperActivity`).
//!
//! Every call goes through [`with_activity`], the one place that turns android-activity's raw VM
//! and activity pointers into `jni` handles; the rest is safe `jni` calls. A Java exception or a
//! JNI error becomes `None` and a log line, never a crash.

use jni::objects::{JObject, JString};
use jni::{Env, JValue, jni_sig, jni_str};
use winit::platform::android::activity::AndroidApp;

/// `Intent.FLAG_ACTIVITY_NEW_TASK`: the link opens in the other app's own task.
const FLAG_ACTIVITY_NEW_TASK: i32 = 0x1000_0000;
/// The label of a copied clip, shown by some keyboards.
const CLIP_LABEL: &str = "PaperCraft";

/// Runs `body` on this thread's JNI attachment with the activity. `None` when the VM is missing or
/// any call failed (a pending Java exception is cleared and logged).
#[allow(unsafe_code, reason = "android-activity hands out raw JavaVM and Activity pointers; this is the one place they become jni handles")]
fn with_activity<T>(app: &AndroidApp, what: &str, body: impl FnOnce(&mut Env, &JObject) -> jni::errors::Result<T>) -> Option<T> {
    let vm_ptr = app.vm_as_ptr();
    let activity_ptr = app.activity_as_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return None;
    }
    // SAFETY: both pointers come from android-activity, which keeps the process JavaVM and the
    // activity's global reference alive for as long as `app` exists; neither is null (checked).
    let vm = unsafe { jni::JavaVM::from_raw(vm_ptr.cast()) };
    let activity: jni::sys::jobject = activity_ptr.cast();
    let result = vm.attach_current_thread(|env| -> jni::errors::Result<T> {
        // SAFETY: the reference is valid while `app` lives (above); a `Cast` never deletes it.
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity)? };
        let answer = body(env, &activity);
        if env.exception_check() {
            env.exception_clear();
        }
        answer
    });
    result.map_err(|error| log::warn!("{what}: {error}")).ok()
}

/// The activity's clipboard manager.
fn clipboard_manager<'a>(env: &mut Env<'a>, activity: &JObject) -> jni::errors::Result<JObject<'a>> {
    let service = env.new_string("clipboard")?;
    env.call_method(activity, jni_str!("getSystemService"), jni_sig!((java.lang.String) -> java.lang.Object), &[JValue::Object(&service)])?.l()
}

/// Puts `text` on the system clipboard.
pub fn copy_text(app: &AndroidApp, text: &str) {
    with_activity(app, "copy", |env, activity| {
        let manager = clipboard_manager(env, activity)?;
        let label = env.new_string(CLIP_LABEL)?;
        let body = env.new_string(text)?;
        let clip = env
            .call_static_method(
                jni_str!("android/content/ClipData"),
                jni_str!("newPlainText"),
                jni_sig!((java.lang.CharSequence, java.lang.CharSequence) -> android.content.ClipData),
                &[JValue::Object(&label), JValue::Object(&body)],
            )?
            .l()?;
        env.call_method(&manager, jni_str!("setPrimaryClip"), jni_sig!((android.content.ClipData) -> void), &[JValue::Object(&clip)])?;
        Ok(())
    });
}

/// The text on the system clipboard, if there is any.
pub fn read_text(app: &AndroidApp) -> Option<String> {
    with_activity(app, "paste", |env, activity| {
        let manager = clipboard_manager(env, activity)?;
        let clip = env.call_method(&manager, jni_str!("getPrimaryClip"), jni_sig!(() -> android.content.ClipData), &[])?.l()?;
        if clip.is_null() || env.call_method(&clip, jni_str!("getItemCount"), jni_sig!(() -> int), &[])?.i()? < 1 {
            return Ok(None);
        }
        let item = env.call_method(&clip, jni_str!("getItemAt"), jni_sig!((int) -> android.content.ClipData::Item), &[JValue::Int(0)])?.l()?;
        let text = env
            .call_method(&item, jni_str!("coerceToText"), jni_sig!((android.content.Context) -> java.lang.CharSequence), &[JValue::Object(activity)])?
            .l()?;
        if text.is_null() {
            return Ok(None);
        }
        let text = env.call_method(&text, jni_str!("toString"), jni_sig!(() -> java.lang.String), &[])?.l()?;
        string_of(env, text)
    })
    .flatten()
}

/// Opens `url` in the app the user picked for it (the browser for web links). Other schemes are
/// refused and logged.
pub fn open_url(app: &AndroidApp, url: &str) {
    if !is_link(url) {
        log::warn!("open: refused a link that is not http, https, mailto or tel");
        return;
    }
    with_activity(app, "open", |env, activity| {
        let text = env.new_string(url)?;
        let uri = env
            .call_static_method(jni_str!("android/net/Uri"), jni_str!("parse"), jni_sig!((java.lang.String) -> android.net.Uri), &[JValue::Object(&text)])?
            .l()?;
        let action = env.new_string("android.intent.action.VIEW")?;
        let intent = env.new_object(
            jni_str!("android/content/Intent"),
            jni_sig!((java.lang.String, android.net.Uri) -> void),
            &[JValue::Object(&action), JValue::Object(&uri)],
        )?;
        env.call_method(&intent, jni_str!("addFlags"), jni_sig!((int) -> android.content.Intent), &[JValue::Int(FLAG_ACTIVITY_NEW_TASK)])?;
        env.call_method(activity, jni_str!("startActivity"), jni_sig!((android.content.Intent) -> void), &[JValue::Object(&intent)])?;
        Ok(())
    });
}

/// The area the app keeps clear, in pixels (left, top, right, bottom): system bars, cutout and the
/// soft keyboard, as `PaperActivity.safeInsets` last saw them. `None` when the call failed.
pub fn safe_insets(app: &AndroidApp) -> Option<[i32; 4]> {
    with_activity(app, "insets", |env, activity| {
        let array = env.call_method(activity, jni_str!("safeInsets"), jni_sig!(() -> int[]), &[])?.l()?;
        let array = env.cast_local::<jni::objects::JIntArray>(array)?;
        let mut sides = [0; 4];
        array.get_region(env, 0, &mut sides)?;
        Ok(sides)
    })
}

/// Whether the soft keyboard has written into `GameActivity`'s text buffer yet. Before it has, the
/// buffer's native text is a null pointer that `AndroidApp::text_input_state` must not read.
pub fn keyboard_wrote(app: &AndroidApp) -> bool {
    with_activity(app, "keyboard", |env, activity| env.call_method(activity, jni_str!("keyboardWrote"), jni_sig!(() -> bool), &[])?.z()).unwrap_or(false)
}

/// Takes final text the app read out of the keyboard buffer: `before` off its start, `after` off its
/// end, on the UI thread (`PaperActivity.consumeInput`), so letters typed meanwhile stay.
pub fn consume_input(app: &AndroidApp, before: &str, after: &str) {
    let _ = with_activity(app, "consume input", |env, activity| {
        let (before, after) = (env.new_string(before)?, env.new_string(after)?);
        env.call_method(
            activity,
            jni_str!("consumeInput"),
            jni_sig!((java.lang.String, java.lang.String) -> void),
            &[JValue::Object(&before), JValue::Object(&after)],
        )?;
        Ok(())
    });
}

/// Whether a [`consume_input`] has not reached the buffer yet; the buffer must not be read until it has.
pub fn input_pending(app: &AndroidApp) -> bool {
    with_activity(app, "input pending", |env, activity| env.call_method(activity, jni_str!("inputPending"), jni_sig!(() -> bool), &[])?.z()).unwrap_or(false)
}

/// Schemes a link may use to leave the app; anything else (files, intents, javascript) is refused.
const LINK_SCHEMES: [&str; 4] = ["https://", "http://", "mailto:", "tel:"];

/// Whether `url` may be handed to another app: http, https, mailto or tel, with no control characters.
fn is_link(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    LINK_SCHEMES.iter().any(|scheme| lower.starts_with(scheme)) && !url.chars().any(char::is_control)
}

/// The Rust text of a Java string; `None` for `null`.
fn string_of(env: &Env, value: JObject) -> jni::errors::Result<Option<String>> {
    if value.is_null() {
        return Ok(None);
    }
    let text = env.cast_local::<JString>(value)?;
    text.try_to_string(env).map(Some)
}
