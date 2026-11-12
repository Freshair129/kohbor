// The service and status wiring that consume this API land in S3-5i/S3-5l.
#![allow(dead_code)]

pub mod inventory;
pub mod prewarm;
pub mod probe;
pub mod protocol;
pub mod service;
