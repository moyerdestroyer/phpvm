//! Shared test helpers (this crate is binary-only; tests live in submodules).

#[cfg(test)]
pub mod env_lock {
    use std::sync::Mutex;

    pub static LOCK: Mutex<()> = Mutex::new(());
}
