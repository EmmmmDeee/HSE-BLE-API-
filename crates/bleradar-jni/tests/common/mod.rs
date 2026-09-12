//! A `JNIEnv` function table for tests that have no JVM.
//!
//! Only the slots `bleradar_jni::env` reads are populated, at the same
//! indices, over a heap `MockString` (UTF-16 units) standing in for `jstring`.
//! Strings live in a thread-local arena so the safe API never dereferences a
//! raw pointer: `read` finds a reference by pointer identity.

#![allow(dead_code)]

use core::cell::RefCell;
use core::ffi::c_void;

use bleradar_jni::env::{
    JStringRef, JniEnvPtr, SLOT_EXCEPTION_CHECK, SLOT_GET_STRING_LENGTH, SLOT_GET_STRING_REGION,
    SLOT_NEW_STRING, TABLE_LEN_JNI_21,
};

struct MockString {
    units: Vec<u16>,
}

thread_local! {
    // The boxes are the point: a `jstring` is a pointer to the `MockString`,
    // which must not move when the arena grows, so `Vec<MockString>` (what
    // `clippy::vec_box` suggests) would hand out dangling references.
    #[allow(clippy::vec_box)]
    static ARENA: RefCell<Vec<Box<MockString>>> = const { RefCell::new(Vec::new()) };
}

fn register(units: Vec<u16>) -> JStringRef {
    let boxed = Box::new(MockString { units });
    let reference: JStringRef = core::ptr::from_ref::<MockString>(&boxed).cast_mut().cast();
    ARENA.with(|arena| arena.borrow_mut().push(boxed));
    reference
}

unsafe extern "system" fn get_string_length(_env: JniEnvPtr, string: JStringRef) -> i32 {
    // SAFETY: the VM side of the mock; `string` was produced by `register`.
    let units = unsafe { &(*string.cast::<MockString>()).units };
    i32::try_from(units.len()).expect("mock string fits jsize")
}

unsafe extern "system" fn get_string_region(
    _env: JniEnvPtr,
    string: JStringRef,
    start: i32,
    len: i32,
    buf: *mut u16,
) {
    // SAFETY: the VM side of the mock; `string` was produced by `register`
    // and the caller sized `buf` for `len` units.
    unsafe {
        let units = &(*string.cast::<MockString>()).units;
        let start = usize::try_from(start).expect("non-negative start");
        let len = usize::try_from(len).expect("non-negative len");
        core::ptr::copy_nonoverlapping(units.as_ptr().add(start), buf, len);
    }
}

unsafe extern "system" fn new_string(_env: JniEnvPtr, units: *const u16, len: i32) -> JStringRef {
    // SAFETY: the VM side of the mock; the caller passes `len` valid units.
    let slice = unsafe { core::slice::from_raw_parts(units, usize::try_from(len).expect("len")) };
    register(slice.to_vec())
}

unsafe extern "system" fn exception_check(_env: JniEnvPtr) -> u8 {
    0
}

/// A fake VM: a function table whose populated slots are the ones the crate
/// reads, plus the double indirection JNI uses (`JNIEnv*` → table pointer →
/// table).
pub struct MockEnv {
    _table: Box<[*const c_void; TABLE_LEN_JNI_21]>,
    table_ptr: *const c_void,
}

impl MockEnv {
    pub fn new() -> Self {
        let mut table = Box::new([core::ptr::null::<c_void>(); TABLE_LEN_JNI_21]);
        table[SLOT_NEW_STRING] = new_string as *const c_void;
        table[SLOT_GET_STRING_LENGTH] = get_string_length as *const c_void;
        table[SLOT_GET_STRING_REGION] = get_string_region as *const c_void;
        table[SLOT_EXCEPTION_CHECK] = exception_check as *const c_void;
        let table_ptr = core::ptr::from_ref::<[*const c_void; TABLE_LEN_JNI_21]>(&table).cast();
        Self {
            _table: table,
            table_ptr,
        }
    }

    /// The `JNIEnv*` to hand to an export.
    pub fn env(&self) -> JniEnvPtr {
        core::ptr::from_ref::<*const c_void>(&self.table_ptr)
            .cast_mut()
            .cast()
    }

    /// A `jstring` holding `text`.
    pub fn string(&self, text: &str) -> JStringRef {
        register(text.encode_utf16().collect())
    }

    /// A `jstring` holding arbitrary UTF-16 units (Java strings may carry
    /// unpaired surrogates; `&str` cannot express them).
    pub fn string_units(&self, units: &[u16]) -> JStringRef {
        register(units.to_vec())
    }

    /// The text behind a `jstring` this mock produced (by `string` or by an
    /// export's `NewString`), or `None` for null or an unknown reference.
    pub fn read(&self, string: JStringRef) -> Option<String> {
        if string.is_null() {
            return None;
        }
        ARENA.with(|arena| {
            arena
                .borrow()
                .iter()
                .rev()
                .find(|boxed| {
                    core::ptr::from_ref::<MockString>(boxed)
                        .cast_mut()
                        .cast::<c_void>()
                        == string
                })
                .and_then(|boxed| String::from_utf16(&boxed.units).ok())
        })
    }

    /// Frees every string this thread's mock produced; outstanding references
    /// become invalid, so call it only between independent test iterations.
    pub fn reset(&self) {
        ARENA.with(|arena| arena.borrow_mut().clear());
    }
}

impl Default for MockEnv {
    fn default() -> Self {
        Self::new()
    }
}
