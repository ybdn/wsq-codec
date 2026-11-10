//! Splitting independent work over scoped threads.

use std::sync::{Arc, Mutex};
use std::thread::{Builder, Scope, ScopedJoinHandle};

#[cfg(test)]
thread_local! {
    /// Makes [`spawn`] behave as if the OS refused to create a thread.
    pub(crate) static FAIL_SPAWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A job started by [`spawn`]: running on a thread, or already done on the
/// calling thread because no thread could be created.
pub(crate) enum Task<'scope, T> {
    Thread(ScopedJoinHandle<'scope, T>),
    Done(T),
}

impl<T> Task<'_, T> {
    /// Waits for the job and returns its result. Panics if the job panicked.
    pub(crate) fn join(self) -> T {
        match self {
            Task::Thread(handle) => handle.join().expect("scoped job does not panic"),
            Task::Done(value) => value,
        }
    }
}

/// Runs `f` on a new scoped thread. If the thread cannot be created (no
/// thread support on the target, thread limits), runs `f` on the calling
/// thread instead: threads never change the output.
pub(crate) fn spawn<'scope, 'env, T, F>(scope: &'scope Scope<'scope, 'env>, f: F) -> Task<'scope, T>
where
    F: FnOnce() -> T + Send + 'scope,
    T: Send + 'scope,
{
    match try_spawn(scope, f) {
        Ok(handle) => Task::Thread(handle),
        Err(f) => Task::Done(f()),
    }
}

/// Runs `f` on a new scoped thread, or gives it back if the thread cannot
/// be created.
pub(crate) fn try_spawn<'scope, 'env, T, F>(
    scope: &'scope Scope<'scope, 'env>,
    f: F,
) -> Result<ScopedJoinHandle<'scope, T>, F>
where
    F: FnOnce() -> T + Send + 'scope,
    T: Send + 'scope,
{
    // A failed spawn drops the closure, so keep it reachable to give it back.
    let slot = Arc::new(Mutex::new(Some(f)));
    let job = Arc::clone(&slot);
    #[cfg(test)]
    let refused = FAIL_SPAWN.with(|flag| flag.get());
    #[cfg(not(test))]
    let refused = false;
    // Tests that refuse allocations do so on the threads started here too.
    #[cfg(test)]
    let injection = crate::mem::inject::current();
    let spawned = if refused {
        Err(std::io::Error::other("thread creation refused"))
    } else {
        Builder::new().spawn_scoped(scope, move || {
            #[cfg(test)]
            crate::mem::inject::set(injection);
            let f = job.lock().ok().and_then(|mut f| f.take());
            f.expect("job runs once")()
        })
    };
    spawned.map_err(|_| {
        let f = slot.lock().ok().and_then(|mut f| f.take());
        f.expect("a job that did not start is still here")
    })
}

/// Fewest items worth a thread of their own.
pub(crate) const MIN_ITEMS: usize = 1 << 18;

/// How many threads to give `items` items, given up to `threads`.
pub(crate) fn parts(threads: usize, items: usize) -> usize {
    threads.min(items / MIN_ITEMS).max(1)
}

/// Calls `f(first, group)` on consecutive groups of `rows`, `first` being
/// the index of the group's first row, on up to `threads` threads.
/// `items`, the work all the rows stand for, decides how many threads are
/// worth it.
pub(crate) fn row_groups<R: Send>(
    rows: &mut [R],
    items: usize,
    threads: usize,
    f: impl Fn(usize, &mut [R]) + Sync,
) {
    let parts = parts(threads, items);
    if parts == 1 {
        return f(0, rows);
    }
    let size = rows.len().div_ceil(parts).max(1);
    let f = &f;
    std::thread::scope(|scope| {
        for (i, group) in rows.chunks_mut(size).enumerate() {
            drop(spawn(scope, move || f(i * size, group)));
        }
    });
}

/// Returns `f(i)` for every `i` in `order`, a permutation of `0..n`, on
/// up to `parts` threads, as a vector indexed by `i`. The threads take the
/// items one at a time in the given order, so listing the largest first
/// balances uneven items. Each result depends on its item only, so the
/// output does not depend on the threads.
pub(crate) fn map_dynamic<R: Send>(
    order: &[usize],
    parts: usize,
    f: impl Fn(usize) -> R + Sync,
) -> Vec<R> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let worker = || {
        let mut done = Vec::new();
        loop {
            let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let Some(&i) = order.get(k) else { break };
            done.push((i, f(i)));
        }
        done
    };
    let mut out: Vec<Option<R>> = order.iter().map(|_| None).collect();
    std::thread::scope(|scope| {
        let tasks: Vec<_> = (1..parts).map(|_| spawn(scope, worker)).collect();
        let mut merge = |done: Vec<(usize, R)>| {
            for (i, r) in done {
                out[i] = Some(r);
            }
        };
        merge(worker());
        for task in tasks {
            merge(task.join());
        }
    });
    out.into_iter()
        .map(|r| r.expect("every item is mapped"))
        .collect()
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
        let a = spawn(scope, a);
        let b = spawn(scope, b);
        let c = c();
        (a.join(), b.join(), c)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DecodeOptions, EncodeOptions, decode_with, encode};

    /// Runs `f` as if no thread could be created.
    fn without_threads<R>(f: impl FnOnce() -> R) -> R {
        FAIL_SPAWN.with(|flag| flag.set(true));
        let out = f();
        FAIL_SPAWN.with(|flag| flag.set(false));
        out
    }

    #[test]
    fn spawn_falls_back_to_the_calling_thread() {
        let caller = std::thread::current().id();
        let id = without_threads(|| {
            std::thread::scope(|s| spawn(s, || std::thread::current().id()).join())
        });
        assert_eq!(id, caller);
        let id = std::thread::scope(|s| spawn(s, || std::thread::current().id()).join());
        assert_ne!(id, caller);
    }

    #[test]
    fn helpers_survive_refused_threads() {
        assert_eq!(without_threads(|| run3(true, || 1, || 2, || 3)), (1, 2, 3));
        let order = [2, 0, 3, 1];
        let squares = [0, 1, 4, 9];
        assert_eq!(map_dynamic(&order, 3, |i| i * i), squares);
        assert_eq!(
            without_threads(|| map_dynamic(&order, 3, |i| i * i)),
            squares
        );
        let src: Vec<u32> = (0..MIN_ITEMS as u32 * 3).collect();
        let mut dst = vec![0; src.len()];
        without_threads(|| {
            let mut rows: Vec<&mut [u32]> = dst.chunks_mut(1000).collect();
            row_groups(&mut rows, src.len(), 4, |first, rows| {
                for (i, row) in rows.iter_mut().enumerate() {
                    for (j, d) in row.iter_mut().enumerate() {
                        *d = src[(first + i) * 1000 + j] + 1;
                    }
                }
            })
        });
        assert!(dst.iter().zip(&src).all(|(d, s)| *d == s + 1));
    }

    #[test]
    fn codec_output_is_identical_when_no_thread_can_be_created() {
        // Integer-only pattern, large enough for every parallel section.
        let (w, h) = (1031usize, 777usize);
        let pixels: Vec<u8> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                (((x * 7 + y * 3) % 61) * 4 + (x * y) % 13) as u8
            })
            .collect();
        let wsq = encode(&pixels, w, h, &EncodeOptions::default()).unwrap();
        let image = crate::decode(&wsq).unwrap();
        let (enc, dec) = without_threads(|| {
            (
                encode(&pixels, w, h, &EncodeOptions::default().with_threads(4)).unwrap(),
                decode_with(&wsq, &DecodeOptions::default().with_threads(4)).unwrap(),
            )
        });
        assert_eq!(enc, wsq);
        assert_eq!(dec, image);
    }
}
