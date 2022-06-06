#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::missing_crate_level_docs)]
#![warn(rustdoc::missing_doc_code_examples)]
#![feature(vec_into_raw_parts)]
#![feature(generic_associated_types)]
#![doc = include_str!("../README.md")]

pub mod alloc;
pub mod code;
pub mod hook;
pub mod patcher;
pub mod wrapper;
