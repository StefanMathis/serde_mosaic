#![cfg_attr(docsrs, doc = include_str!("../README.md"))]
#![cfg_attr(not(docsrs), doc = include_str!("../README_local.md"))]
#![deny(missing_docs)]

pub mod attributes;
pub mod database_manager;
pub mod format;

pub use attributes::*;
pub use database_manager::*;
pub use format::*;

pub use serde;
