//! `malloc`, `realloc` and `free` for pl_mpeg on `wasm32-unknown-unknown`,
//! which has no C library (see `wasm-libc/stdlib.h`). Each block carries
//! its size in a 16-byte header so `free`/`realloc` can rebuild the
//! layout for Rust's allocator.

use std::alloc::{Layout, alloc, dealloc, realloc as re};
use std::ffi::c_void;

const HEADER: usize = 16;

fn layout(size: usize) -> Option<Layout> {
    Layout::from_size_align(size.checked_add(HEADER)?, HEADER).ok()
}

/// # Safety
/// C `malloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    let Some(l) = layout(size) else { return std::ptr::null_mut() };
    // SAFETY: `l` has a non-zero size; the header holds the user size.
    unsafe {
        let p = alloc(l);
        if p.is_null() {
            return p.cast();
        }
        (p as *mut usize).write(size);
        p.add(HEADER).cast()
    }
}

/// # Safety
/// C `free`: `ptr` is null or came from [`malloc`]/[`realloc`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: the block starts HEADER bytes earlier and records its size.
    unsafe {
        let p = (ptr as *mut u8).sub(HEADER);
        let size = (p as *const usize).read();
        dealloc(p, layout(size).expect("valid layout"));
    }
}

/// # Safety
/// C `realloc`: `ptr` is null or came from [`malloc`]/[`realloc`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    if ptr.is_null() {
        // SAFETY: as malloc.
        return unsafe { malloc(size) };
    }
    let Some(new) = layout(size) else { return std::ptr::null_mut() };
    // SAFETY: as free; the new layout keeps the alignment.
    unsafe {
        let p = (ptr as *mut u8).sub(HEADER);
        let old = (p as *const usize).read();
        let q = re(p, layout(old).expect("valid layout"), new.size());
        if q.is_null() {
            return q.cast();
        }
        (q as *mut usize).write(size);
        q.add(HEADER).cast()
    }
}
