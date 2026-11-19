//! Port of `utils/cache.js`: an in-memory cache with per-entry expiry and a size cap
//! (oldest entry evicted first) whose loads are shared by concurrent callers.

use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures::StreamExt;
use futures::future::{BoxFuture, FutureExt, Shared};
use indexmap::IndexMap;

type Load<V, E> = Shared<BoxFuture<'static, Result<Option<V>, E>>>;

struct Entry<V, E> {
    expires: Instant,
    load: Load<V, E>,
}

pub struct TtlCache<K, V, E> {
    ttl: Duration,
    max_size: usize,
    map: Mutex<IndexMap<K, Entry<V, E>>>,
}

impl<K, V, E> TtlCache<K, V, E>
where
    K: Hash + Eq + Clone,
    V: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    pub fn new(ttl: Duration, max_size: usize) -> Self {
        Self {
            ttl,
            max_size,
            map: Mutex::new(IndexMap::new()),
        }
    }

    /// Returns the cached result for `key`, or runs `loader` and caches it. Concurrent
    /// callers share one in-flight load. `None` results and errors are not kept, so
    /// the next call retries.
    pub async fn get_or_load<F>(&self, key: K, loader: F) -> Result<Option<V>, E>
    where
        F: Future<Output = Result<Option<V>, E>> + Send + 'static,
    {
        let load = {
            let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
            match map.get(&key) {
                Some(entry) if Instant::now() <= entry.expires => entry.load.clone(),
                _ => {
                    let load = loader.boxed().shared();
                    map.shift_remove(&key);
                    map.insert(
                        key.clone(),
                        Entry {
                            expires: Instant::now() + self.ttl,
                            load: load.clone(),
                        },
                    );
                    if map.len() > self.max_size {
                        map.shift_remove_index(0);
                    }
                    load
                }
            }
        };

        let result = load.clone().await;
        if !matches!(result, Ok(Some(_))) {
            let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
            if map.get(&key).is_some_and(|entry| entry.load.ptr_eq(&load)) {
                map.shift_remove(&key);
            }
        }
        result
    }

    pub fn remove(&self, key: &K) {
        self.map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .shift_remove(key);
    }

    pub fn clear(&self) {
        self.map.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    pub fn len(&self) -> usize {
        self.map.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Runs `f` over `items` with at most `limit` calls in flight, preserving result order.
pub async fn map_limit<T, R, F, Fut>(
    items: impl IntoIterator<Item = T>,
    limit: usize,
    f: F,
) -> Vec<R>
where
    F: FnMut(T) -> Fut,
    Fut: Future<Output = R>,
{
    futures::stream::iter(items)
        .map(f)
        .buffered(limit.max(1))
        .collect()
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn counting_loader(
        calls: &Arc<AtomicUsize>,
        result: Result<Option<u32>, String>,
    ) -> impl Future<Output = Result<Option<u32>, String>> + Send + 'static {
        let calls = calls.clone();
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(10)).await;
            result
        }
    }

    #[tokio::test]
    async fn concurrent_callers_share_one_load() {
        let cache = TtlCache::new(Duration::from_secs(60), 10);
        let calls = Arc::new(AtomicUsize::new(0));
        let (a, b) = tokio::join!(
            cache.get_or_load("k", counting_loader(&calls, Ok(Some(1)))),
            cache.get_or_load("k", counting_loader(&calls, Ok(Some(2)))),
        );
        assert_eq!((a, b), (Ok(Some(1)), Ok(Some(1))));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            cache
                .get_or_load("k", counting_loader(&calls, Ok(Some(3))))
                .await,
            Ok(Some(1))
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn none_and_errors_are_not_cached() {
        let cache = TtlCache::new(Duration::from_secs(60), 10);
        let calls = Arc::new(AtomicUsize::new(0));
        assert_eq!(
            cache
                .get_or_load("k", counting_loader(&calls, Ok(None)))
                .await,
            Ok(None)
        );
        assert_eq!(
            cache
                .get_or_load("k", counting_loader(&calls, Err("x".into())))
                .await,
            Err("x".into())
        );
        assert_eq!(
            cache
                .get_or_load("k", counting_loader(&calls, Ok(Some(5))))
                .await,
            Ok(Some(5))
        );
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn entries_expire() {
        let cache = TtlCache::new(Duration::from_millis(20), 10);
        let calls = Arc::new(AtomicUsize::new(0));
        cache
            .get_or_load("k", counting_loader(&calls, Ok(Some(1))))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(40)).await;
        cache
            .get_or_load("k", counting_loader(&calls, Ok(Some(2))))
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn oldest_entry_is_evicted_at_the_size_cap() {
        let cache = TtlCache::new(Duration::from_secs(60), 2);
        let calls = Arc::new(AtomicUsize::new(0));
        for key in ["a", "b", "c"] {
            cache
                .get_or_load(key, counting_loader(&calls, Ok(Some(1))))
                .await
                .unwrap();
        }
        assert_eq!(cache.len(), 2);
        cache
            .get_or_load("a", counting_loader(&calls, Ok(Some(1))))
            .await
            .unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            4,
            "a was evicted and loaded again"
        );
    }

    #[tokio::test]
    async fn map_limit_keeps_order_and_limit() {
        let in_flight = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let results = map_limit(0..10u64, 3, |i| {
            let (in_flight, peak) = (in_flight.clone(), peak.clone());
            async move {
                let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(10 - i)).await;
                in_flight.fetch_sub(1, Ordering::SeqCst);
                i * 2
            }
        })
        .await;
        assert_eq!(results, (0..10).map(|i| i * 2).collect::<Vec<_>>());
        assert!(peak.load(Ordering::SeqCst) <= 3);
    }
}
