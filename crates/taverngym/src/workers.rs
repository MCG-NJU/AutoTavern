//! Worker threads that run games (the batched environment's pool).
//!
//! Combats and effect chains on huge late warbands go deeper than a thread's default 2 MiB
//! stack: inside the Python extension that is a bare SIGSEGV with an empty log. Only the
//! pages a thread touches are committed, so the larger reserve costs no memory.

/// Stack of a game worker thread.
pub const STACK: usize = 64 << 20;

/// `s.spawn(f)` with the worker stack.
pub fn spawn<'scope, 'env, T: Send + 'scope>(
    s: &'scope std::thread::Scope<'scope, 'env>,
    f: impl FnOnce() -> T + Send + 'scope,
) -> std::thread::ScopedJoinHandle<'scope, T> {
    std::thread::Builder::new()
        .stack_size(STACK)
        .spawn_scoped(s, move || {
            crate::guard::thread_started(STACK);
            f()
        })
        .expect("spawn a game worker thread")
}
