// TODO: Data model - add optional orderGroupId field
// TODO: Data router should pass messages to rabbitmq - key = orderGroupId(if exists)
// TODO: Data router - messages should be persistent(persistent queue + attribute on message) - configurable(?)
// TODO: Command service should listen on rabbitmq
// TODO: Command service should be able to listen on multiple queues
// TODO: Command service should have exclusive consumer on a queue
// TODO: Command service - make sure that acks are done after message is fully processed
// TODO: Command service - add locking on same orderGroupId
// TODO: Helm files
// TODO: Docker compose/test changes
// TODO: Docs

// TODO:(unrelated) - split helm template into multiple files based on services

// TODO: PK: Use tokio mutex(possible async operations inside)
// TODO: PK: Move to guard structure instead of closure? (release on drop)

fn lock_key<'k>(&self, key: &'k StorageKey) -> DocumentLock<'k, StorageKey> {
    DocumentLock::new(self.locked_keys.clone(), key)
}


use std::sync::{Arc, Mutex, Weak};

pub type LockStorage<T> = Arc<Mutex<Vec<(T, Weak<Mutex<()>>)>>>;

pub struct DocumentLock<'a, T: PartialEq + Clone> {
    locked_keys: LockStorage<T>,
    key_lock: Arc<Mutex<()>>,
    key: &'a T,
}

impl<'a, T: PartialEq + Clone> DocumentLock<'a, T> {
    pub fn new(locked_keys: LockStorage<T>, key: &T) -> DocumentLock<T> {
        let key_lock = DocumentLock::get_storage_key_lock(&locked_keys, &key);
        DocumentLock {
            locked_keys,
            key_lock,
            key,
        }
    }

    pub fn then<U, F: FnOnce() -> U>(&self, f: F) -> U {
        let _result = self.key_lock.lock();

        f()
    }

    fn get_storage_key_lock(locked_keys: &LockStorage<T>, key: &T) -> Arc<Mutex<()>> {
        let mut locked_keys = locked_keys.lock().unwrap();
        let lock_entry = (*locked_keys).iter_mut().find(|item| item.0 == *key);
        match lock_entry {
            Some(entry) => {
                let lock = match entry.1.upgrade() {
                    Some(arc) => arc,
                    None => Arc::new(Mutex::new(())),
                };
                entry.1 = Arc::downgrade(&lock);
                lock
            }
            None => {
                let lock = Arc::new(Mutex::new(()));
                locked_keys.push((key.clone(), Arc::downgrade(&lock)));
                lock
            }
        }
    }
}

impl<'a, T: PartialEq + Clone> Drop for DocumentLock<'a, T> {
    fn drop(&mut self) {
        self.key_lock = Arc::new(Mutex::new(()));
        let mut vec = self.locked_keys.lock().unwrap();
        (&mut *vec).retain(|v| &v.0 != self.key || v.1.upgrade().is_some());
    }
}
