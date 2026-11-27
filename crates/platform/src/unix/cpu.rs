#![allow(unsafe_code)]

use std::time::Duration;

fn spent(time: libc::timeval) -> Duration {
    let seconds = u64::try_from(time.tv_sec).unwrap_or(0);
    let micros = u64::try_from(time.tv_usec).unwrap_or(0);
    Duration::from_secs(seconds) + Duration::from_micros(micros)
}

pub fn cpu_time() -> Option<Duration> {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    let asked = unsafe { libc::getrusage(libc::RUSAGE_SELF, &raw mut usage) };
    (asked == 0).then(|| spent(usage.ru_utime) + spent(usage.ru_stime))
}

#[cfg(test)]
mod tests {
    #[test]
    fn work_done_by_this_process_counts_as_cpu_time() {
        let before = super::cpu_time().expect("before");
        let mut sum = 0u64;
        while super::cpu_time().expect("during") == before {
            sum = sum.wrapping_add(std::hint::black_box(sum) | 1);
        }
        std::hint::black_box(sum);
        assert!(super::cpu_time().expect("after") > before);
    }
}
