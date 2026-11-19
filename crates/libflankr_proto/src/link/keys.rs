//! Static keys: libhardline's, re-exported under their old paths.
//!
//! [`StaticKey`] is the same trait as before (`public` and `dh`), and is
//! implemented for [`StaticKeypair`] and for `Arc<K>` as before.
//! libhardline also implements it for each of its vault keys.

pub use libhardline::{StaticKey, StaticKeypair, generate_keypair, public_key};
