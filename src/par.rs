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
    // A failed spawn drops the closure, so keep it reachable to run it here.
    let slot = Arc::new(Mutex::new(Some(f)));
    let job = Arc::clone(&slot);
    #[cfg(test)]
    let refused = FAIL_SPAWN.with(|flag| flag.get());
    #[cfg(not(test))]
    let refused = false;
    let spawned = if refused {
        Err(std::io::Error::other("thread creation refused"))
    } else {
        Builder::new().spawn_scoped(scope, move || {
            let f = job.lock().ok().and_then(|mut f| f.take());
            f.expect("job runs once")()
        })
    };
    match spawned {
        Ok(handle) => Task::Thread(handle),
        Err(_) => {
            let f = slot.lock().ok().and_then(|mut f| f.take());
            Task::Done(f.expect("a job that did not start is still here")())
        }
    }
}

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
            // Dropping the task is fine: the scope joins it.
            drop(spawn(scope, move || f(src, dst)));
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
    fn run3_and_zip_chunks_survive_refused_threads() {
        assert_eq!(without_threads(|| run3(true, || 1, || 2, || 3)), (1, 2, 3));
        let src: Vec<u32> = (0..MIN_ITEMS as u32 * 3).collect();
        let mut dst = vec![0; src.len()];
        without_threads(|| {
            zip_chunks(&src, &mut dst, 4, |s, d| {
                d.iter_mut().zip(s).for_each(|(d, s)| *d = s + 1)
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
