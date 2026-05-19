//! Well-known ADS index groups.
//!
//! Source: <https://github.com/birkenfeld/ads-rs/blob/master/src/index.rs>

// Unfortunately, not all those constants are documented.
#![allow(missing_docs)]

/// Get u32 handle to the name in the write data.  Index offset is 0.
/// Use with a `write_read` transaction.
pub const GET_SYMHANDLE_BYNAME: u32 = 0xF003;
/// Read/write data for a symbol by handle.
/// Use the handle as the index offset.
pub const RW_SYMVAL_BYHANDLE: u32 = 0xF005;

// undocumented; from AdsDef.h
pub const SYM_UPLOAD: u32 = 0xF00B;
pub const SYM_DT_UPLOAD: u32 = 0xF00E;
pub const SYM_UPLOAD_INFO2: u32 = 0xF00F;
pub const GET_TYPEINFO_BYNAME_EX: u32 = 0xF011;
