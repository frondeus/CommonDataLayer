use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use tokio::sync::oneshot::{channel, Receiver, Sender};

// +TODO: Data model - add optional orderGroupId field
// +TODO: Data router should pass messages to rabbitmq - key = orderGroupId(if exists)
// TODO: Data router - messages should be persistent(persistent queue + attribute on message) - configurable(?)
//+ TODO: Command service should listen on rabbitmq
//+ TODO: Command service should be able to listen on multiple queues
//+ TODO: Command service should have exclusive consumer on a queue - or merge streams
//+ TODO: Command service - make sure that acks are done after message is fully processed
//+ TODO: Command service - add locking on same orderGroupId
//+ TODO: Command service - locking limits only parallelization, does not guarantee order(if multiple futures are queued)
// TODO: Helm files
// TODO: Docker compose/test changes
// TODO: Docs

//+ TODO: Use tokio mutex(possible async operations inside)
//+ TODO: Move to guard structure instead of closure? (release on drop)

// TODO: remove unwraps
// TODO: change name
// TODO: make generic(?)

type LocksDB = Arc<Mutex<HashMap<String, LockQueue>>>;

pub struct TaskQueue {
    locks: LocksDB,
}

impl TaskQueue {
    pub async fn add_task(&self, key: String) -> LockGuard {
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
    pub fn new() -> TaskQueue {
        TaskQueue {
            locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}
impl Default for TaskQueue {
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
        let mut guard = self.db.lock().unwrap();
        let entry = (*guard).get_mut(&self.key).unwrap();
        entry.end_task();
        if !entry.is_blocked {
            (*guard).remove(&self.key);
        }
    }
}
