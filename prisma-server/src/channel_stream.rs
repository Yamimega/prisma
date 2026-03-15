use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Buf, Bytes, BytesMut};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc;

/// Generic adapter that bridges a pair of `mpsc` channels into `AsyncRead + AsyncWrite`.
///
/// Used by XHTTP, XPorta, and other transport modes where upload and download
/// data arrive on separate channels. The adapter buffers partial reads and
/// applies backpressure on writes via `try_send` + waker-based retry.
pub struct ChannelStream {
    read_rx: mpsc::Receiver<Bytes>,
    write_tx: mpsc::Sender<Bytes>,
    read_buf: BytesMut,
}

impl ChannelStream {
    pub fn new(read_rx: mpsc::Receiver<Bytes>, write_tx: mpsc::Sender<Bytes>) -> Self {
        Self {
            read_rx,
            write_tx,
            read_buf: BytesMut::new(),
        }
    }
}

impl AsyncRead for ChannelStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // Drain internal buffer first
        if !self.read_buf.is_empty() {
            let to_copy = self.read_buf.len().min(buf.remaining());
            buf.put_slice(&self.read_buf[..to_copy]);
            self.read_buf.advance(to_copy);
            return Poll::Ready(Ok(()));
        }

        // Try to receive more data from the channel
        match self.read_rx.poll_recv(cx) {
            Poll::Ready(Some(data)) => {
                let to_copy = data.len().min(buf.remaining());
                buf.put_slice(&data[..to_copy]);
                if to_copy < data.len() {
                    self.read_buf.extend_from_slice(&data[to_copy..]);
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => Poll::Ready(Ok(())), // EOF
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for ChannelStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.write_tx.try_send(Bytes::copy_from_slice(buf)) {
            Ok(()) => Poll::Ready(Ok(buf.len())),
            Err(mpsc::error::TrySendError::Full(_)) => {
                // Channel full. Spawn a waiter that wakes us when capacity is available.
                // The spawned task acquires then immediately drops a permit (freeing the
                // slot), then wakes us so our next try_send succeeds. Data is only ever
                // sent via try_send above -- never in the spawned task -- preventing duplication.
                let tx = self.write_tx.clone();
                let waker = cx.waker().clone();
                tokio::spawn(async move {
                    if let Ok(permit) = tx.reserve_owned().await {
                        drop(permit);
                    }
                    waker.wake();
                });
                Poll::Pending
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "channel closed",
            ))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
