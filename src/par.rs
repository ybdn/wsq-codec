//! Splitting independent work over scoped threads.

/// Fewest items worth a thread of their own.
pub(crate) const MIN_ITEMS: usize = 1 << 18;

/// How many threads to give `items` items, given up to `threads`.
pub(crate) fn parts(threads: usize, items: usize) -> usize {
    threads.min(items / MIN_ITEMS).max(1)
}

/// Calls `f` on matching chunks of `src` and `dst`, which have the same
/// length, on up to `threads` threads.
pub(crate) fn zip_chunks<A: Sync, B: Send>(
    src: &[A],
    dst: &mut [B],
    threads: usize,
    f: impl Fn(&[A], &mut [B]) + Sync,
) {
    debug_assert_eq!(src.len(), dst.len());
    let parts = parts(threads, src.len());
    if parts == 1 {
        return f(src, dst);
    }
    let size = src.len().div_ceil(parts);
    let f = &f;
    std::thread::scope(|scope| {
        for (src, dst) in src.chunks(size).zip(dst.chunks_mut(size)) {
            scope.spawn(move || f(src, dst));
        }
    });
}

/// Runs three independent jobs, on three threads when `parallel` is set,
/// and returns their results in order.
pub(crate) fn run3<T: Send>(
    parallel: bool,
    a: impl FnOnce() -> T + Send,
    b: impl FnOnce() -> T + Send,
    c: impl FnOnce() -> T + Send,
) -> (T, T, T) {
    if !parallel {
        return (a(), b(), c());
    }
    std::thread::scope(|scope| {
        let a = scope.spawn(a);
        let b = scope.spawn(b);
        let c = c();
        (
            a.join().expect("entropy coding does not panic"),
            b.join().expect("entropy coding does not panic"),
            c,
        )
    })
}
