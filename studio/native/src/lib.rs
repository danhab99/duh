//! The bridge between duh Studio (Dart) and duh's engine (`lib`).
//! Everything Dart can call lives in `api`; `frb_generated` is written by
//! flutter_rust_bridge_codegen (`just studio-codegen`) and committed.

pub mod api;
mod frb_generated;
