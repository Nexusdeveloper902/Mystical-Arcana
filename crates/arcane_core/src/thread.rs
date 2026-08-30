//! Thread pool + job system. Wraps `rayon` for fork-join parallelism and
//! `crossbeam-channel` for streaming background work.
//!
//! The engine uses two parallelism patterns:
//! 1. **Fork-join** — for batched processing where we wait for all results
//!    (e.g., procedural chunk generation, VFX simulation step). Uses
//!    `rayon`'s built-in pool.
//! 2. **Streaming background** — for long-lived workers that produce or
//!    consume data without blocking the main thread (e.g., chunk streaming,
//!    audio loading, asset cooking). Uses [`BackgroundWorker`].

use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// Wrapper around rayon's thread pool with a fixed (but configurable) thread
/// count. Used for fork-join gameplay batches.
#[derive(Debug)]
pub struct ThreadPool {
    inner: Arc<rayon::ThreadPool>,
}

impl ThreadPool {
    /// Builds a thread pool with the given number of worker threads.
    pub fn new(num_threads: usize) -> Self {
        Self {
            inner: Arc::new(
                rayon::ThreadPoolBuilder::new()
                    .num_threads(num_threads)
                    .thread_name(|i| format!("arcane-worker-{i}"))
                    .build()
                    .expect("rayon thread pool init"),
            ),
        }
    }

    /// Default pool — uses rayon defaults (typically one thread per logical CPU).
    pub fn default_pool() -> Self {
        Self {
            inner: Arc::new(
                rayon::ThreadPoolBuilder::new()
                    .thread_name(|i| format!("arcane-worker-{i}"))
                    .build()
                    .expect("rayon thread pool init"),
            ),
        }
    }

    /// Runs a closure on the pool, blocking until complete.
    pub fn install<OP, R>(&self, op: OP) -> R
    where
        OP: FnOnce() -> R + Send,
        R: Send,
    {
        self.inner.install(op)
    }

    /// Spawns a job that returns a value. Blocks immediately if the pool is
    /// saturated. Returns a [`JobHandle`] that can be joined later.
    pub fn spawn<F, R>(&self, f: F) -> JobHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = bounded(1);
        self.inner.spawn(move || {
            let r = f();
            let _ = tx.send(r);
        });
        JobHandle { rx }
    }

    /// Parallel-for over a slice. Each element is processed by a closure.
    /// `f` runs on rayon workers; calling thread participates as well.
    pub fn par_for<'a, T, F>(&self, slice: &'a mut [T], f: F)
    where
        T: Send,
        F: Fn(usize, &mut T) + Sync + 'a,
    {
        // We use rayon's par_iter_mut under the hood.
        use rayon::prelude::*;
        self.inner.install(|| {
            slice.par_iter_mut().enumerate().for_each(|(i, x)| f(i, x));
        });
    }
}

/// Handle to a spawned job. [`wait`] blocks until the job completes.
pub struct JobHandle<R> {
    rx: Receiver<R>,
}

impl<R> JobHandle<R> {
    /// Blocks until the job finishes and returns its result.
    pub fn wait(self) -> R {
        self.rx.recv().expect("job panicked")
    }

    /// Non-blocking check for completion.
    pub fn try_wait(&self) -> Option<R>
    where
        R: Clone,
    {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => panic!("job panicked before producing result"),
        }
    }
}

// === Background streaming worker ===

/// A long-lived background worker that pulls jobs from a queue. Suitable for
/// chunk streaming, asset loading, audio decoding — anything that produces
/// a stream of work without saturating the CPU.
pub struct BackgroundWorker<J, R>
where
    J: Send + 'static,
    R: Send + 'static,
{
    handle: Option<JoinHandle<()>>,
    // Stored as Option so the Drop impl can take it before joining the thread.
    // Without this, the channel stays open during join() and the worker never exits.
    tx: Option<Sender<J>>,
    rx: Receiver<R>,
}

impl<J, R> BackgroundWorker<J, R>
where
    J: Send + 'static,
    R: Send + 'static,
{
    /// Spawns a background worker. The closure loops, pulling jobs and
    /// pushing results back. The worker exits when the job channel is
    /// dropped (which happens when the `BackgroundWorker` is dropped).
    pub fn spawn<F>(name: &str, f: F) -> Self
    where
        F: Fn(J) -> R + Send + Sync + 'static,
    {
        let (job_tx, job_rx) = bounded::<J>(64);
        let (res_tx, res_rx) = bounded::<R>(64);
        let f = std::sync::Arc::new(f);
        let handle = thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                for job in job_rx {
                    let r = f(job);
                    let _ = res_tx.send(r);
                }
            })
            .expect("background worker spawn");
        Self { handle: Some(handle), tx: Some(job_tx), rx: res_rx }
    }

    /// Enqueue a job. Blocks if the job queue is full (back-pressure).
    pub fn submit(&self, job: J) {
        if let Some(tx) = &self.tx {
            tx.send(job).expect("worker died")
        }
    }

    /// Try to receive a completed result without blocking.
    pub fn try_recv(&self) -> Option<R> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => None,
        }
    }

    /// Block on the next completed result.
    pub fn recv(&self) -> R {
        self.rx.recv().expect("worker died")
    }
}

impl<J, R> Drop for BackgroundWorker<J, R>
where
    J: Send + 'static,
    R: Send + 'static,
{
    fn drop(&mut self) {
        // Drop the sender FIRST so the worker's job_rx returns None and the
        // loop exits. The worker will finish any in-flight job, then return.
        self.tx.take();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn thread_pool_default_runs_jobs() {
        let pool = ThreadPool::default_pool();
        let h1 = pool.spawn(|| 2 + 2);
        let h2 = pool.spawn(|| 3 * 4);
        assert_eq!(h1.wait(), 4);
        assert_eq!(h2.wait(), 12);
    }

    #[test]
    fn par_for_processes_all_elements() {
        let pool = ThreadPool::default_pool();
        let mut v = vec![0u32; 100];
        pool.par_for(&mut v, |i, x| *x = i as u32);
        for (i, v) in v.iter().enumerate() {
            assert_eq!(*v, i as u32);
        }
    }

    #[test]
    fn background_worker_streams_results() {
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        let worker: BackgroundWorker<u32, u32> = BackgroundWorker::spawn("test-worker", move |j| {
            c.fetch_add(1, Ordering::Relaxed);
            j * 2
        });

        for i in 0..10 {
            worker.submit(i);
        }
        // Wait for all results.
        let mut results = Vec::new();
        for _ in 0..10 {
            results.push(worker.recv());
        }
        results.sort();
        assert_eq!(results, vec![0, 2, 4, 6, 8, 10, 12, 14, 16, 18]);
        assert_eq!(counter.load(Ordering::Relaxed), 10);
    }

    #[test]
    fn background_worker_drops_cleanly() {
        let worker: BackgroundWorker<i32, String> =
            BackgroundWorker::spawn("drop-test", |j| format!("{}", j));
        worker.submit(42);
        let _ = worker.recv();
        // Drop should not panic.
        drop(worker);
    }
}
