use crate::lib::helper::receive_host_ipc_message;
use crate::lib::protocol::HostIpcMessage;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::pubsub::{PubSubBehavior, PubSubChannel, Subscriber};

extern crate alloc;
extern crate core;

type BoxFuture = Pin<Box<dyn Future<Output = ()>>>;

type HostIpcChannel = PubSubChannel<NoopRawMutex, (u32, HostIpcMessage), 10, 3, 1>;
type HostIpcSubscriber = Subscriber<'static, NoopRawMutex, (u32, HostIpcMessage), 10, 3, 1>;

#[thread_local]
static TASK_QUEUE: RefCell<Option<TaskQueue>> = RefCell::new(None);

#[thread_local]
pub static HOST_IPC_CHANNEL: HostIpcChannel = HostIpcChannel::new();

/// Simple fixed-capacity FIFO queue (ring buffer) for spawned tasks.
struct TaskQueue {
  buf: Vec<Option<BoxFuture>>,
  head: usize,
  len: usize,
}

impl TaskQueue {
  fn new(capacity: usize) -> Self {
    let mut buf = Vec::with_capacity(capacity);
    buf.resize_with(capacity, || None);
    Self { buf, head: 0, len: 0 }
  }

  fn len(&self) -> usize {
    self.len
  }

  fn is_empty(&self) -> bool {
    self.len == 0
  }

  fn push_back(&mut self, item: BoxFuture) {
    if self.len == self.buf.len() {
      let new_capacity = self.buf.len() * 2;
      let mut new_buf = Vec::with_capacity(new_capacity);
      new_buf.resize_with(new_capacity, || None);
      for i in 0..self.len {
        let idx = (self.head + i) % self.buf.len();
        new_buf[i] = self.buf[idx].take();
      }
      self.buf = new_buf;
      self.head = 0;
    }
    let idx = (self.head + self.len) % self.buf.len();
    self.buf[idx] = Some(item);
    self.len += 1;
  }

  fn pop_front(&mut self) -> Option<BoxFuture> {
    if self.len == 0 {
      return None;
    }
    let item = self.buf[self.head].take();
    self.head = (self.head + 1) % self.buf.len();
    self.len -= 1;
    item
  }
}

/// Spawn an async task into the runtime
pub fn spawn<F>(future: F)
where
  F: Future<Output = ()> + 'static,
{
  let mut queue = TASK_QUEUE.borrow_mut();
  if queue.is_none() {
    *queue = Some(TaskQueue::new(10));
  }
  queue.as_mut().unwrap().push_back(Box::pin(future));
}

/// Execute one round of polling all pending tasks
/// Returns true if there are no more tasks to run
#[unsafe(no_mangle)]
pub extern "C" fn tick(host_msg_id: u32, host_msg_size: u32) -> bool {
  // println!("tick: {host_msg_id} {host_msg_size}");

  if host_msg_size != 0 {
    let host_msg = receive_host_ipc_message(host_msg_id, host_msg_size);

    HOST_IPC_CHANNEL.publish_immediate((host_msg_id, host_msg));
  }

  let mut queue_borrow = TASK_QUEUE.borrow_mut();

  let queue = match queue_borrow.as_mut() {
    Some(q) => q,
    None => return true, // No queue = no tasks
  };

  let count = queue.len();

  for _ in 0..count {
    if let Some(mut task) = queue.pop_front() {
      let waker = create_waker();
      let mut context = Context::from_waker(&waker);

      match task.as_mut().poll(&mut context) {
        Poll::Ready(()) => {
          // Task completed, don't re-queue
        }
        Poll::Pending => {
          // Task not done, put it back in the queue
          queue.push_back(task);
        }
      }
    }
  }

  queue.is_empty()
}

/// Create a no-op waker (we don't need wake notifications in this simple model)
fn create_waker() -> Waker {
  unsafe fn clone(_: *const ()) -> RawWaker {
    RawWaker::new(core::ptr::null(), &VTABLE)
  }

  unsafe fn wake(_: *const ()) {}
  unsafe fn wake_by_ref(_: *const ()) {}
  unsafe fn drop(_: *const ()) {}

  static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

  unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
}

/// Yield control back to the runtime
pub fn yield_now() -> YieldNow {
  YieldNow { yielded: false }
}

pub struct YieldNow {
  yielded: bool,
}

impl Future for YieldNow {
  type Output = ();

  fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
    if self.yielded {
      Poll::Ready(())
    } else {
      self.yielded = true;
      Poll::Pending
    }
  }
}

pub async fn get_next_host_message() -> (u32, HostIpcMessage) {
  let mut subscriber = match HOST_IPC_CHANNEL.subscriber() {
    Ok(subscriber) => subscriber,
    Err(err) => print_and_panic!("try_poll_channel: Could not get subscriber: {err:?}"),
  };

  subscriber.next_message_pure().await
}
