//! The WebAssembly interface the npm package calls: the host copies a .docx
//! into memory from `alloc`, calls `validate`, reads the JSON report it points
//! to, then frees both.

/// Memory for `n` bytes, owned by the caller until `dealloc`.
#[unsafe(no_mangle)]
pub extern "C" fn alloc(n: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(n);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `p` and `n` come from `alloc(n)` (or `validate`'s result) and are freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dealloc(p: *mut u8, n: usize) {
    drop(unsafe { Vec::from_raw_parts(p, 0, n) });
}

/// Validates `n` bytes at `p` and returns the report as JSON (see
/// `Report::to_json`): its address in the high 32 bits, its length in the low.
///
/// # Safety
/// `p` points to `n` initialized bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate(p: *const u8, n: usize, max_errors: u32) -> u64 {
    let bytes = unsafe { std::slice::from_raw_parts(p, n) };
    let json = crate::check(bytes).to_json(max_errors as usize).into_bytes().into_boxed_slice();
    let len = json.len();
    let ptr = Box::into_raw(json) as *mut u8;
    ((ptr as u64) << 32) | len as u64
}
