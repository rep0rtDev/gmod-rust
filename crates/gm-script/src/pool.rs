//! A fixed set of worker threads for the parallel phase: one job per actor per dispatch, no
//! thread spawn per tick. `run` borrows the jobs for its own duration (it waits for every job
//! before returning, also when one panics), which is what lets them hold `&mut Actor`.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};

type Job = Box<dyn FnOnce() + Send + 'static>;

pub struct Pool {
    tx: Sender<Job>,
    pub threads: usize,
}

struct Latch {
    left: Mutex<usize>,
    done: Condvar,
}

struct CountDown<'a>(&'a Latch);

impl Drop for CountDown<'_> {
    fn drop(&mut self) {
        let mut left = self.0.left.lock().unwrap();
        *left -= 1;
        if *left == 0 {
            self.0.done.notify_all();
        }
    }
}

impl Pool {
    pub fn new(threads: usize) -> Self {
        let threads = threads.max(1);
        let (tx, rx) = channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        for i in 0..threads {
            let rx: Arc<Mutex<Receiver<Job>>> = rx.clone();
            std::thread::Builder::new()
                .name(format!("gm-script-{i}"))
                .spawn(move || loop {
                    let job = rx.lock().unwrap().recv();
                    match job {
                        Ok(job) => job(),
                        Err(_) => return,
                    }
                })
                .expect("spawn a script worker");
        }
        Pool { tx, threads }
    }

    /// Runs the jobs on the workers and returns when all of them finished.
    pub fn run<'a>(&self, jobs: Vec<Box<dyn FnOnce() + Send + 'a>>) {
        if jobs.is_empty() {
            return;
        }
        let latch = Arc::new(Latch {
            left: Mutex::new(jobs.len()),
            done: Condvar::new(),
        });
        for job in jobs {
            // The job may borrow locals of the caller: `run` does not return before every job
            // has finished (the latch counts down in a drop guard, so a panicking job counts too),
            // and the worker never touches the job after it has run.
            let job: Box<dyn FnOnce() + Send + 'static> = unsafe {
                std::mem::transmute::<
                    Box<dyn FnOnce() + Send + 'a>,
                    Box<dyn FnOnce() + Send + 'static>,
                >(job)
            };
            let mine = latch.clone();
            let wrapped: Job = Box::new(move || {
                let _guard = CountDown(&mine);
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
            });
            if let Err(e) = self.tx.send(wrapped) {
                // no worker left: run the job here so the latch still completes
                e.0();
            }
        }
        let mut left = latch.left.lock().unwrap();
        while *left > 0 {
            left = latch.done.wait(left).unwrap();
        }
    }
}
