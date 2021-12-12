use std::marker::PhantomData;

use futures::{stream::FusedStream, Stream, StreamExt};
use tokio::sync::OwnedRwLockWriteGuard;

use crate::ctx;

/// This is a wrapper for `RwLock<ctx::State>` that releases the lock at the end of a stream.
pub struct StreamLock<T> {
  lock: Option<OwnedRwLockWriteGuard<ctx::State>>,
  _pd: PhantomData<T>,
}
impl<T> StreamLock<T> {
  /// Accepts a stream and a [`OwnedRwLockWriteGuard`] to the [`ctx::State`]
  pub fn chain<S: Stream<Item = T>>(
    stream: S,
    lock: OwnedRwLockWriteGuard<ctx::State>,
  ) -> futures::stream::Chain<S, Self> {
    stream.chain(Self {
      lock: Some(lock),
      _pd: PhantomData,
    })
  }
}
impl<S> Unpin for StreamLock<S> {}

impl<S> FusedStream for StreamLock<S> {
  fn is_terminated(&self) -> bool {
    true
  }
}

impl<T> Stream for StreamLock<T> {
  type Item = T;

  fn poll_next(
    mut self: std::pin::Pin<&mut Self>,
    _cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Option<Self::Item>> {
    self.lock.take();
    std::task::Poll::Ready(None)
  }
}
