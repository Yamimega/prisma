use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Buf, BytesMut};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc;
use tonic::Streaming;

use prisma_core::proto::tunnel::TunnelData;

/// Adapter that bridges a tonic bidirectional gRPC stream into AsyncRead + AsyncWrite.
pub struct GrpcStream {
    read_rx: mpsc::Receiver<bytes::Bytes>,
    write_tx: mpsc::Sender<bytes::Bytes>,
    read_buf: BytesMut,
}

impl GrpcStream {
    pub fn new(
        inbound: Streaming<TunnelData>,
        response_tx: mpsc::Sender<Result<TunnelData, tonic::Status>>,
    ) -> Self {
        let (read_tx, read_rx) = mpsc::channel::<bytes::Bytes>(64);
        let (write_tx, write_rx) = mpsc::channel::<bytes::Bytes>(64);

        // Read loop: gRPC inbound → channel
        tokio::spawn(Self::read_loop(inbound, read_tx));
        // Write loop: channel → gRPC outbound
        tokio::spawn(Self::write_loop(write_rx, response_tx));

        Self {
            read_rx,
            write_tx,
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

    async fn write_loop(
        mut rx: mpsc::Receiver<bytes::Bytes>,
        tx: mpsc::Sender<Result<TunnelData, tonic::Status>>,
    ) {
        while let Some(data) = rx.recv().await {
            let msg = TunnelData {
                payload: data.to_vec(),
            };
            if tx.send(Ok(msg)).await.is_err() {
                break;
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
        match self.write_tx.try_send(bytes::Bytes::copy_from_slice(buf)) {
            Ok(()) => Poll::Ready(Ok(buf.len())),
            Err(mpsc::error::TrySendError::Full(_)) => {
                let tx = self.write_tx.clone();
                let data = bytes::Bytes::copy_from_slice(buf);
                let waker = cx.waker().clone();
                tokio::spawn(async move {
                    let _ = tx.send(data).await;
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
