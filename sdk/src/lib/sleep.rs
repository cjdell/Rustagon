use crate::lib::protocol::{extern_check_timer, extern_register_timer};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

struct Sleep {
  timer_id: i32,
  registered: bool,
  duration_ms: u32,
}

impl Sleep {
  fn new(ms: u32) -> Self {
    Self {
      timer_id: -1,
      registered: false,
      duration_ms: ms,
    }
  }
}

impl Future for Sleep {
  type Output = ();

  fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    // Register timer on first poll
    if !self.registered {
      self.timer_id = unsafe { extern_register_timer(self.duration_ms) };
      self.registered = true;
    }

    // Check if timer has expired
    let expired = unsafe { extern_check_timer(self.timer_id) };

    if expired == 1 {
      Poll::Ready(())
    } else {
      // Wake the waker so we get polled again
      cx.waker().wake_by_ref();
      Poll::Pending
    }
  }
}

pub async fn sleep(ms: u32) {
  Sleep::new(ms).await
}
