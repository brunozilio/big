use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};

pub const MOST_AT_ONCE: usize = 8;

pub fn ordered<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let workers =
        std::thread::available_parallelism().map_or(2, NonZeroUsize::get).clamp(1, MOST_AT_ONCE).min(items.len());
    if workers <= 1 {
        return items.iter().map(work).collect();
    }
    let next = AtomicUsize::new(0);
    let mut found: Vec<Option<R>> = items.iter().map(|_| None).collect();
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(at) else { break };
                        done.push((at, work(item)));
                    }
                    done
                })
            })
            .collect();
        for handle in handles {
            let done = handle.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic));
            for (at, value) in done {
                if let Some(slot) = found.get_mut(at) {
                    *slot = Some(value);
                }
            }
        }
    });
    found.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_is_worked_once_and_the_order_is_kept() {
        let items: Vec<u64> = (0..100).collect();
        let worked = ordered(&items, |item| {
            std::thread::sleep(std::time::Duration::from_millis(100 - item));
            item * 2
        });
        assert_eq!(worked, items.iter().map(|item| item * 2).collect::<Vec<_>>());
        assert!(ordered(&[] as &[u64], |item| *item).is_empty());
        assert_eq!(ordered(&[7_u64], |item| *item + 1), [8]);
    }
}
