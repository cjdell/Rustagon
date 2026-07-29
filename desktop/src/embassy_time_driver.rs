use std::time::SystemTime;

struct StdDriver;

impl embassy_time_driver::Driver for StdDriver {
    fn now(&self) -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64
    }

    fn schedule_wake(&self, _at: u64, _waker: &core::task::Waker) {
        // Desktop doesn't use the embassy executor, so we don't need to
        // actually schedule anything.  Tasks are driven by `futures::executor::block_on`
        // which runs the sleep via the `sleep()` helper in `app::utils`.
    }
}

embassy_time_driver::time_driver_impl!(static DRIVER: StdDriver = StdDriver);
