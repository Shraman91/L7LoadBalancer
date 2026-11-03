use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

const NUM_SHARDS: usize = 64;

#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    last_update: Instant,
}

struct Shard {
    entries: parking_lot::RwLock<HashMap<IpAddr, TokenBucket>>,
}

impl Shard {
    fn new() -> Self {
        Self {
            entries: parking_lot::RwLock::new(HashMap::new()),
        }
    }
}

pub struct RateLimiter {
    shards: Vec<Shard>,
    rate: f64,
    burst: f64,
    max_entries_per_shard: usize,
    total_entries: AtomicUsize,
    idle_ttl: Duration,
}

impl RateLimiter {
    pub fn from_config(config: &crate::config::RateLimitConfig) -> Self {
        Self::new(config.requests_per_second, config.burst, config.max_entries)
    }

    pub fn new(requests_per_second: f64, burst: u32, max_entries: usize) -> Self {
        let max_entries = max_entries.max(NUM_SHARDS);
        let max_entries_per_shard = max_entries.div_ceil(NUM_SHARDS);

        let mut shards = Vec::with_capacity(NUM_SHARDS);
        for _ in 0..NUM_SHARDS {
            shards.push(Shard::new());
        }

        Self {
            shards,
            rate: requests_per_second.max(0.1),
            burst: (burst.max(1)) as f64,
            max_entries_per_shard,
            total_entries: AtomicUsize::new(0),
            idle_ttl: Duration::from_secs(60),
        }
    }

    fn get_shard_index(&self, ip: &IpAddr) -> usize {
        let mut hasher = DefaultHasher::new();
        ip.hash(&mut hasher);
        (hasher.finish() as usize) % NUM_SHARDS
    }

    /// Checks if a request from `ip` is allowed.
    /// Returns `Ok(())` if allowed, or `Err(retry_after_seconds)` if rejected.
    pub fn check(&self, ip: IpAddr) -> Result<(), u64> {
        let shard_idx = self.get_shard_index(&ip);
        let shard = &self.shards[shard_idx];
        let now = Instant::now();

        let mut entries = shard.entries.write();

        if let Some(bucket) = entries.get_mut(&ip) {
            let elapsed = now.duration_since(bucket.last_update).as_secs_f64();
            bucket.tokens = (bucket.tokens + elapsed * self.rate).min(self.burst);
            bucket.last_update = now;

            if bucket.tokens >= 1.0 {
                bucket.tokens -= 1.0;
                Ok(())
            } else {
                let needed = 1.0 - bucket.tokens;
                let retry_after = (needed / self.rate).ceil() as u64;
                Err(retry_after.max(1))
            }
        } else {
            // New entry: check capacity bounding
            if entries.len() >= self.max_entries_per_shard {
                // Evict idle entries
                let ttl = self.idle_ttl;
                let before = entries.len();
                entries.retain(|_, b| now.duration_since(b.last_update) < ttl);
                let evicted = before - entries.len();
                self.total_entries.fetch_sub(evicted, Ordering::Relaxed);

                // If still full, evict the oldest entry
                if entries.len() >= self.max_entries_per_shard {
                    if let Some((&oldest_ip, _)) = entries.iter().min_by_key(|(_, b)| b.last_update)
                    {
                        entries.remove(&oldest_ip);
                        self.total_entries.fetch_sub(1, Ordering::Relaxed);
                    }
                }
            }

            // Start with burst - 1 tokens
            entries.insert(
                ip,
                TokenBucket {
                    tokens: self.burst - 1.0,
                    last_update: now,
                },
            );
            self.total_entries.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    /// Prune entries that have been idle for longer than `idle_ttl`.
    pub fn prune_idle(&self) -> usize {
        self.prune_idle_older_than(self.idle_ttl)
    }

    /// Prune entries that have been idle for longer than `ttl`.
    pub fn prune_idle_older_than(&self, ttl: Duration) -> usize {
        let now = Instant::now();
        let mut total_evicted = 0;

        for shard in &self.shards {
            let mut entries = shard.entries.write();
            let before = entries.len();
            entries.retain(|_, b| now.duration_since(b.last_update) < ttl);
            let evicted = before - entries.len();
            total_evicted += evicted;
        }

        self.total_entries
            .fetch_sub(total_evicted, Ordering::Relaxed);
        total_evicted
    }

    pub fn total_entries(&self) -> usize {
        self.total_entries.load(Ordering::Relaxed)
    }

    /// Spawn background cleaner task that sweeps idle entries periodically.
    pub fn spawn_cleanup_task(
        self: &Arc<Self>,
        interval: Duration,
        cancel_token: CancellationToken,
    ) {
        let limiter = Arc::clone(self);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => break,
                    _ = ticker.tick() => {
                        let pruned = limiter.prune_idle();
                        if pruned > 0 {
                            tracing::debug!(pruned, remaining = limiter.total_entries(), "rate limiter pruned idle entries");
                        }
                    }
                }
            }
        });
    }
}
