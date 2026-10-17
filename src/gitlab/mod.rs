//! Merge Requests über GitLab und glab: Remotes und Host-Zuordnung (E16),
//! glab-Runner, später GraphQL, Domänenlogik, Service und Checkout.

// Vorübergehend bis zur Verdrahtung (Task 10–12): Das Crate ist ein reines
// Binary, unverdrahteter Code bricht sonst `clippy -D warnings`.
#![cfg_attr(not(test), allow(dead_code))]

pub mod glab;
pub mod remote;
pub mod secret;
