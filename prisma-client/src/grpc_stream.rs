use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Buf, BytesMut};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc;
use tonic::Streaming;

use prisma_core::proto::tunnel::TunnelData;

/// Adapter that bridges a tonic gRPC bidirectional stream into AsyncRead + AsyncWrite.
pub struct GrpcStream {
    read_rx: mpsc::Receiver<bytes::Bytes>,
    write_tx: mpsc::Sender<TunnelData>,
    read_buf: BytesMut,
}

impl GrpcStream {
    pub fn new(inbound: Streaming<TunnelData>, outbound_tx: mpsc::Sender<TunnelData>) -> Self {
        let (read_tx, read_rx) = mpsc::channel::<bytes::Bytes>(64);

        tokio::spawn(Self::read_loop(inbound, read_tx));

        Self {
            read_rx,
            write_tx: outbound_tx,
            read_buf: BytesMut::new(),
        }
    }

    async fn read_loop(mut inbound: Streaming<TunnelData>, tx: mpsc::Sender<bytes::Bytes>) {
        loop {
            match inbound.message().await {
                Ok(Some(msg)) => {
                    if !msg.payload.is_empty()
                        && tx.send(bytes::Bytes::from(msg.payload)).await.is_err()
                    {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    }
}

impl AsyncRead for GrpcStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if !self.read_buf.is_empty() {
            let to_copy = self.read_buf.len().min(buf.remaining());
            buf.put_slice(&self.read_buf[..to_copy]);
            self.read_buf.advance(to_copy);
            return Poll::Ready(Ok(()));
        }

        match self.read_rx.poll_recv(cx) {
            Poll::Ready(Some(data)) => {
                let to_copy = data.len().min(buf.remaining());
                buf.put_slice(&data[..to_copy]);
                if to_copy < data.len() {
                    self.read_buf.extend_from_slice(&data[to_copy..]);
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => Poll::Ready(Ok(())),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for GrpcStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let msg = TunnelData {
            payload: buf.to_vec(),
        };
        match self.write_tx.try_send(msg) {
            Ok(()) => Poll::Ready(Ok(buf.len())),
            Err(mpsc::error::TrySendError::Full(_)) => {
                let tx = self.write_tx.clone();
                let msg = TunnelData {
                    payload: buf.to_vec(),
                };
                let waker = cx.waker().clone();
                tokio::spawn(async move {
                    let _ = tx.send(msg).await;
                    waker.wake();
                });
                Poll::Pending
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "gRPC stream closed",
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
