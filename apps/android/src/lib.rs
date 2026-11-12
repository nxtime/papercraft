//! PaperCraft for Android: the shared app on a `GameActivity` window. `android_main` is the entry
//! point the activity's native library exports.

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
mod platform;
