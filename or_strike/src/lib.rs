#![feature(const_trait_impl, const_precise_live_drops, default_field_values, hash_map_macro)]

#[macro_use]
extern crate rust_i18n;
i18n!("locales", fallback = ["zh_CN"]);

pub mod body;
pub mod cell;
pub mod io;
pub mod ui;
