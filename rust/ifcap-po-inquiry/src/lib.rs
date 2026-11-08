//! Read-only Rust port of the VistA IFCAP OPTION `PRCH DISPLAY PO`, whose
//! action is `EN3^PRCHRPT`: one purchase-order display session, reproducing
//! the original's prompts, messages, field order and formatting over an
//! immutable [`Store`] of FileMan globals.
//!
//! [`inquire`] is the one inquiry operation. [`bridge`] serves the loopback
//! front-end and its MUMPS, RUST and COMPARE session endpoints.

#![forbid(unsafe_code)]

// `mumps` is private: nothing in it is wider than `pub(crate)`, so it forms no
// part of the public API.
pub mod bridge;
pub mod inquiry;
mod mumps;
pub mod store;

pub use inquiry::{InquiryError, InquiryOutput, STUB_CONTEXT, inquire};
pub use store::{Store, StoreError};
