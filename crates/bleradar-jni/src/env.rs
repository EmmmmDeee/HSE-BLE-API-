//! The single place this crate reads through `JNIEnv`.
//!
//! Every native receives a `JNIEnv*`: a pointer to the calling thread's
//! pointer to the VM's `JNINativeInterface` function table, whose layout the
//! JNI specification fixes (four reserved slots, then the functions in
//! specification order — the same table in HotSpot's and Android's `jni.h`).
//! Every other export in this crate is a total function over primitives and
//! never touches that table; only string traffic needs it, so this module
//! exposes exactly two operations — copying a `jstring` into an owned Rust
//! [`String`] and creating a `jstring` from a `&str` — through the UTF-16
//! entry points (`GetStringLength` / `GetStringRegion` / `NewString`), so the
//! VM's "modified UTF-8" is never decoded or encoded here.
//!
//! The slot indices below are verified against the JDK's `jni.h` and then
//! exercised against a real JVM by `cargo xtask verify-jni-live`;
//! `tests/common/mod.rs` builds a table with the same indices so the unit
//! tests and the export campaign cover this path without a JVM.

use core::ffi::c_void;
use core::mem::transmute;
use core::ptr;

/// `JNIEnv*` as every export receives it: a pointer to the calling thread's
/// pointer to the VM's `JNINativeInterface` function table.
pub type JniEnvPtr = *mut c_void;

/// A `jstring` local reference (a `jobject`). Never dereferenced here; only
/// handed back to the VM's own functions.
pub type JStringRef = *mut c_void;

type JSize = i32;
type JChar = u16;
type JBoolean = u8;

/// Table slot of `NewString(env, const jchar*, jsize) -> jstring`.
pub const SLOT_NEW_STRING: usize = 163;
/// Table slot of `GetStringLength(env, jstring) -> jsize` (UTF-16 units).
pub const SLOT_GET_STRING_LENGTH: usize = 164;
/// Table slot of `GetStringRegion(env, jstring, jsize start, jsize len, jchar* buf)`.
pub const SLOT_GET_STRING_REGION: usize = 220;
/// Table slot of `ExceptionCheck(env) -> jboolean`.
pub const SLOT_EXCEPTION_CHECK: usize = 228;
/// Members of the JNI 21 table (four reserved slots plus 231 functions). The
/// highest slot used above, 228, has existed since JNI 1.2, so Android's
/// JNI 1.6 table (233 members) covers it as well.
pub const TABLE_LEN_JNI_21: usize = 235;

type NewStringFn = unsafe extern "system" fn(JniEnvPtr, *const JChar, JSize) -> JStringRef;
type GetStringLengthFn = unsafe extern "system" fn(JniEnvPtr, JStringRef) -> JSize;
type GetStringRegionFn = unsafe extern "system" fn(JniEnvPtr, JStringRef, JSize, JSize, *mut JChar);
type ExceptionCheckFn = unsafe extern "system" fn(JniEnvPtr) -> JBoolean;

/// Reads the function pointer stored at `slot`.
///
/// Caller's contract: `env` is the non-null `JNIEnv*` the VM passed to the
/// native currently executing on this thread, and `slot` is one of the
/// `SLOT_*` constants above.
unsafe fn table_slot(env: JniEnvPtr, slot: usize) -> *const c_void {
    // SAFETY: by the caller's contract `env` points at a valid pointer to the
    // VM's function table, and every `SLOT_*` constant lies inside the
    // table's specification-mandated minimum length.
    unsafe {
        let table = *env.cast::<*const *const c_void>();
        *table.add(slot)
    }
}

/// Copies `string` into an owned [`String`].
///
/// Returns `None` for a null `env` or `string`, a length the VM reports as
/// negative, an exception left pending by the copy, or UTF-16 that is not
/// valid Unicode (an unpaired surrogate); every export treats `None` as
/// invalid input and answers with its documented sentinel.
///
/// # Safety
///
/// `env` must be null or the `JNIEnv*` the VM passed to the native currently
/// executing on this thread (or a table built to the same layout, as the
/// test mock is), and `string` must be null or a `jstring` reference valid
/// for that call.
#[must_use]
pub unsafe fn read_string(env: JniEnvPtr, string: JStringRef) -> Option<String> {
    if env.is_null() || string.is_null() {
        return None;
    }
    // SAFETY: `env` is non-null and, by this function's contract, the VM's
    // own `JNIEnv*` for this thread; the slots are the specification's fixed
    // indices and hold functions of exactly the signatures transmuted to;
    // `buffer` is sized from the VM's own length.
    unsafe {
        let get_length =
            transmute::<*const c_void, GetStringLengthFn>(table_slot(env, SLOT_GET_STRING_LENGTH));
        let get_region =
            transmute::<*const c_void, GetStringRegionFn>(table_slot(env, SLOT_GET_STRING_REGION));
        let exception_check =
            transmute::<*const c_void, ExceptionCheckFn>(table_slot(env, SLOT_EXCEPTION_CHECK));
        let length = get_length(env, string);
        let units = usize::try_from(length).ok()?;
        let mut buffer = vec![0u16; units];
        if length > 0 {
            get_region(env, string, 0, length, buffer.as_mut_ptr());
        }
        if exception_check(env) != 0 {
            return None;
        }
        String::from_utf16(&buffer).ok()
    }
}

/// Creates a new `jstring` holding `text`.
///
/// Returns null for a null `env`, a text longer than `jsize` can describe,
/// or a VM allocation failure (which leaves the VM's `OutOfMemoryError`
/// pending for the Java caller, exactly as a null return from `NewString`
/// means in JNI).
///
/// # Safety
///
/// `env` must be null or the `JNIEnv*` the VM passed to the native currently
/// executing on this thread (or a table built to the same layout, as the
/// test mock is).
#[must_use]
pub unsafe fn new_string(env: JniEnvPtr, text: &str) -> JStringRef {
    if env.is_null() {
        return ptr::null_mut();
    }
    let units: Vec<u16> = text.encode_utf16().collect();
    let Ok(length) = JSize::try_from(units.len()) else {
        return ptr::null_mut();
    };
    // SAFETY: as in `read_string`; `units` outlives the call and `length` is
    // exactly its length.
    unsafe {
        let new_string = transmute::<*const c_void, NewStringFn>(table_slot(env, SLOT_NEW_STRING));
        new_string(env, units.as_ptr(), length)
    }
}
