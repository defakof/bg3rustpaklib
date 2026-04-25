//! C-compatible FFI surface for linking bg3rustpaklib into C++ projects.
//!
//! Enabled with `--features ffi`. Build as a staticlib for C++:
//!   cargo rustc --release --features ffi --crate-type=staticlib
//!
//! The corresponding C header is at `include/bg3rustpaklib.h`.

use crate::loca::{LocaFormat, LocaUtils};
use crate::Package;
use std::borrow::Cow;
use std::ffi::{c_char, c_void, CStr, CString};
use std::fmt::Write as FmtWrite;
use std::io::Write as IoWrite;
use std::panic;

// ---------------------------------------------------------------------------
// Bg3Bytes - byte buffer returned to C++
// ---------------------------------------------------------------------------

/// Byte buffer returned by the Rust library.
/// Always free with `bg3_free_bytes()`; never free `data` directly.
#[repr(C)]
pub struct Bg3Bytes {
    /// Pointer to the first byte in the returned buffer.
    pub data: *const u8,
    /// Number of bytes available at `data`.
    pub len: usize,
}

fn vec_into_bg3bytes(v: Vec<u8>) -> *mut Bg3Bytes {
    let len = v.len();
    let boxed: Box<[u8]> = v.into_boxed_slice();
    let data = Box::into_raw(boxed) as *mut u8;
    Box::into_raw(Box::new(Bg3Bytes {
        data: data as *const u8,
        len,
    }))
}

/// Free a `Bg3Bytes` buffer returned by any `bg3*` function.
///
/// # Safety
///
/// `bytes` must be null or a pointer returned by this library. Passing any other
/// pointer, or passing the same pointer more than once, is undefined behavior.
#[no_mangle]
pub unsafe extern "C" fn bg3_free_bytes(bytes: *mut Bg3Bytes) {
    if bytes.is_null() {
        return;
    }
    // SAFETY: caller guarantees bytes was allocated by vec_into_bg3bytes.
    let b = unsafe { Box::from_raw(bytes) };
    if !b.data.is_null() {
        // SAFETY: data/len come from the boxed slice allocated in vec_into_bg3bytes.
        let _ = unsafe { Box::from_raw(std::slice::from_raw_parts_mut(b.data as *mut u8, b.len)) };
    }
}

// ---------------------------------------------------------------------------
// Bg3Pak - opaque package handle
// ---------------------------------------------------------------------------

/// Opaque handle to an open PAK package.
pub struct Bg3Pak(Package);

/// Open a PAK file. Returns NULL on error. Thread-safe.
#[no_mangle]
pub extern "C" fn bg3pak_open(path: *const c_char) -> *mut Bg3Pak {
    if path.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: path is checked for null and must be a valid null-terminated C string.
    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    match panic::catch_unwind(|| Package::open(path_str)) {
        Ok(Ok(pkg)) => Box::into_raw(Box::new(Bg3Pak(pkg))),
        _ => std::ptr::null_mut(),
    }
}

/// Close and free a PAK handle opened with `bg3pak_open()`.
///
/// # Safety
///
/// `pak` must be null or a pointer returned by `bg3pak_open`. Passing any other
/// pointer, or passing the same pointer more than once, is undefined behavior.
#[no_mangle]
pub unsafe extern "C" fn bg3pak_close(pak: *mut Bg3Pak) {
    if !pak.is_null() {
        // SAFETY: caller guarantees pak was returned by bg3pak_open.
        unsafe { drop(Box::from_raw(pak)) };
    }
}

/// Returns true if the PAK contains at least one entry whose path starts with
/// `"Localization/{language}/"` (case-insensitive, backslash-normalised).
///
/// # Safety
///
/// `pak` must be null or a valid pointer returned by `bg3pak_open`.
/// `language` must be null or a valid null-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bg3pak_has_localization(
    pak: *const Bg3Pak,
    language: *const c_char,
) -> bool {
    if pak.is_null() || language.is_null() {
        return false;
    }
    // SAFETY: language is checked for null and must be a valid null-terminated C string.
    let lang = match unsafe { CStr::from_ptr(language) }.to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };
    let prefix = format!("localization/{}/", lang.to_ascii_lowercase());
    // SAFETY: caller guarantees pak is a valid Bg3Pak pointer.
    let pak = unsafe { &*pak };
    pak.0.files().iter().any(|f| {
        f.name()
            .replace('\\', "/")
            .to_ascii_lowercase()
            .starts_with(&prefix)
    })
}

/// Call `callback(name, userdata)` for every file entry in the PAK.
/// `name` is a null-terminated UTF-8 path (e.g. `"Localization/Russian/strings.loca"`).
/// Do not call `bg3pak_*` functions from inside the callback.
///
/// # Safety
///
/// `pak` must be null or a valid pointer returned by `bg3pak_open`. If supplied,
/// `callback` must be callable for the duration of this function and must not
/// retain the temporary `name` pointer after returning.
#[no_mangle]
pub unsafe extern "C" fn bg3pak_for_each_file(
    pak: *const Bg3Pak,
    callback: Option<unsafe extern "C" fn(*const c_char, *mut c_void)>,
    userdata: *mut c_void,
) {
    if pak.is_null() {
        return;
    }
    let cb = match callback {
        Some(f) => f,
        None => return,
    };
    // SAFETY: caller guarantees pak is a valid Bg3Pak pointer.
    let pak = unsafe { &*pak };
    for file in pak.0.files() {
        if let Ok(cname) = CString::new(file.name()) {
            // SAFETY: callback contract allows calling with a temporary C string pointer.
            unsafe { cb(cname.as_ptr(), userdata) };
        }
    }
}

/// Read a file from the PAK by its internal path (backslash == slash, case-insensitive).
/// Returns NULL if not found or on error. Free the result with `bg3_free_bytes()`.
///
/// # Safety
///
/// `pak` must be null or a valid pointer returned by `bg3pak_open`.
/// `path` must be null or a valid null-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bg3pak_read_file(
    pak: *const Bg3Pak,
    path: *const c_char,
) -> *mut Bg3Bytes {
    if pak.is_null() || path.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: path is checked for null and must be a valid null-terminated C string.
    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    // SAFETY: caller guarantees pak is a valid Bg3Pak pointer.
    let pak = unsafe { &*pak };
    let file = pak.0.get(path_str).or_else(|| {
        let wanted = path_str.replace('\\', "/").to_ascii_lowercase();
        pak.0
            .files()
            .iter()
            .find(|f| f.name().replace('\\', "/").to_ascii_lowercase() == wanted)
    });
    let file = match file {
        Some(f) => f,
        None => return std::ptr::null_mut(),
    };
    match panic::catch_unwind(panic::AssertUnwindSafe(|| pak.0.read_file(file))) {
        Ok(Ok(data)) => vec_into_bg3bytes(data),
        _ => std::ptr::null_mut(),
    }
}

// ---------------------------------------------------------------------------
// Loca parsing to Qt-compatible compressed JSON
// ---------------------------------------------------------------------------

/// Compress bytes in Qt's `qCompress` wire format:
/// 4-byte big-endian uncompressed length followed by zlib-deflated data.
/// This output is directly readable by `qUncompress()` on the C++ side.
fn qt_compress(data: &[u8]) -> Vec<u8> {
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    let _ = enc.write_all(data);
    let compressed = enc.finish().unwrap_or_default();
    let mut out = Vec::with_capacity(4 + compressed.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&compressed);
    out
}

/// Escape a string value for JSON (content between double-quotes).
fn json_escape(s: &str) -> Cow<'_, str> {
    // Fast path: no characters need escaping.
    if s.bytes().all(|b| b != b'"' && b != b'\\' && b >= 0x20) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    Cow::Owned(out)
}

/// Parse raw binary `.loca` bytes and return a Qt-compatible zlib-compressed
/// JSON object `{"uuid":"text", ...}`.
///
/// The 4-byte big-endian length prefix matches Qt's `qCompress` format so the
/// result can be passed directly to `qUncompress()`. Returns NULL on error or
/// if the resource is empty. Free with `bg3_free_bytes()`.
///
/// # Safety
///
/// `data` must be null or point to `len` readable bytes for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn bg3loca_to_json_compressed(data: *const u8, len: usize) -> *mut Bg3Bytes {
    if data.is_null() || len == 0 {
        return std::ptr::null_mut();
    }
    // SAFETY: caller guarantees data points to len readable bytes.
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let resource = match panic::catch_unwind(|| LocaUtils::load_from_bytes(bytes, LocaFormat::Loca))
    {
        Ok(Ok(r)) => r,
        _ => return std::ptr::null_mut(),
    };
    if resource.is_empty() {
        return std::ptr::null_mut();
    }

    let mut json = String::with_capacity(resource.len() * 80);
    json.push('{');
    for (i, entry) in resource.entries.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push('"');
        json.push_str(&json_escape(&entry.key));
        json.push_str("\":\"");
        json.push_str(&json_escape(&entry.text));
        json.push('"');
    }
    json.push('}');

    vec_into_bg3bytes(qt_compress(json.as_bytes()))
}
