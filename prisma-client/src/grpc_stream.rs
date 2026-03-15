use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Buf, BytesMut};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc;
use tonic::Streaming;

use prisma_core::proto::tunnel::TunnelData;

type ReserveFut<T> = Pin<
    Box<dyn Future<Output = Result<mpsc::OwnedPermit<T>, mpsc::error::SendError<()>>> + Send>,
>;

/// Adapter that bridges a tonic gRPC bidirectional stream into AsyncRead + AsyncWrite.
pub struct GrpcStream {
    read_rx: mpsc::Receiver<bytes::Bytes>,
    write_tx: mpsc::Sender<TunnelData>,
    read_buf: BytesMut,
    write_reserve: Option<ReserveFut<TunnelData>>,
}

impl GrpcStream {
    pub fn new(inbound: Streaming<TunnelData>, outbound_tx: mpsc::Sender<TunnelData>) -> Self {
        let (read_tx, read_rx) = mpsc::channel::<bytes::Bytes>(256);

        tokio::spawn(Self::read_loop(inbound, read_tx));

        Self {
            read_rx,
            write_tx: outbound_tx,
            read_buf: BytesMut::new(),
            write_reserve: None,
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
        let this = self.get_mut();

        let mut fut = this
            .write_reserve
            .take()
            .unwrap_or_else(|| Box::pin(this.write_tx.clone().reserve_owned()));

        match fut.as_mut().poll(cx) {
            Poll::Ready(Ok(permit)) => {
                permit.send(TunnelData {
                    payload: buf.to_vec(),
                });
                Poll::Ready(Ok(buf.len()))
            }
            Poll::Ready(Err(_)) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "gRPC stream closed",
            ))),
            Poll::Pending => {
                this.write_reserve = Some(fut);
                Poll::Pending
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
