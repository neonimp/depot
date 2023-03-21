use neoncore::const_fn::ascii_to_u64_be;

pub mod depot_handle;
mod helpers;
mod types;

/// cbindgen:ignore
pub const MAGIC: u64 = ascii_to_u64_be(b"DEPOTARC");
