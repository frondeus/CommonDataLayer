use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use tokio::sync::oneshot::{channel, Receiver, Sender};

use crate::abort_on_poison;

type LocksDB = Arc<Mutex<HashMap<String, LockQueue>>>;

pub struct ParallelTaskQueue {
    locks: LocksDB,
}

impl ParallelTaskQueue {
    pub async fn run_task(&self, key: String) -> LockGuard {
        let receiver = {
            let mut locks = self.locks.lock().unwrap();
            let lock_queue = LockQueue::new();
            let queue = (*locks).entry(key.clone()).or_insert(lock_queue);
            queue.add_task()
        };
        receiver.await.unwrap();

        LockGuard {
            db: self.locks.clone(),
            key,
        }
    }

    pub fn new() -> ParallelTaskQueue {
        ParallelTaskQueue {
            locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for ParallelTaskQueue {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LockQueue {
    is_blocked: bool,
    queue: VecDeque<Sender<()>>,
}

impl LockQueue {
    fn new() -> LockQueue {
        LockQueue {
            is_blocked: false,
            queue: VecDeque::new(),
        }
    }

    fn add_task(&mut self) -> Receiver<()> {
        let (tx, rx) = channel();
        match self.is_blocked {
            true => {
                self.queue.push_back(tx);
            }
            false => {
                self.is_blocked = true;
                tx.send(()).expect("Receiver dropped");
            }
        };
        rx
    }

    fn end_task(&mut self) {
        match self.queue.pop_front() {
            None => {
                self.is_blocked = false;
            }
            Some(tx) => {
                let result = tx.send(());
                if result.is_err() {
                    // receiver dropped, start next task
                    self.end_task()
                }
            }
        }
    }
}

pub struct LockGuard {
    db: LocksDB,
    key: String,
}
impl Drop for LockGuard {
    fn drop(&mut self) {
        let mut guard = self.db.lock().unwrap_or_else(abort_on_poison);
        let entry = (*guard)
            .get_mut(&self.key)
            .expect("LockGuard already dropped");
        entry.end_task();
        if !entry.is_blocked {
            (*guard).remove(&self.key);
        }
    }
}
