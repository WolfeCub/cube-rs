//! Safe wrappers for the libc calls std lacks.

/// Nanoseconds on `CLOCK_BOOTTIME`, the sensor event clock.
pub fn boottime() -> u64 {
    clock(libc::CLOCK_BOOTTIME)
}

/// Nanoseconds on `CLOCK_MONOTONIC`.
pub fn monotonic() -> u64 {
    clock(libc::CLOCK_MONOTONIC)
}

fn clock(id: libc::clockid_t) -> u64 {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: only writes `t`.
    unsafe { libc::clock_gettime(id, &mut t) };
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}

/// `unix_secs` in the phone's time zone.
pub fn local_time(unix_secs: u64) -> Option<libc::tm> {
    let t = unix_secs as libc::time_t;
    // SAFETY: all zeros is a valid `tm`, and localtime_r only writes it.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let ok = !unsafe { libc::localtime_r(&t, &mut tm) }.is_null();
    ok.then_some(tm)
}
