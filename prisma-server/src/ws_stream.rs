use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::extract::ws::{Message, WebSocket};
use bytes::{Buf, BytesMut};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc;

type ReserveFut<T> = Pin<
    Box<dyn Future<Output = Result<mpsc::OwnedPermit<T>, mpsc::error::SendError<()>>> + Send>,
>;

/// Adapter that bridges an axum WebSocket (Stream/Sink) into AsyncRead + AsyncWrite.
///
/// A background task splits the WebSocket into read/write halves and
/// communicates via channels so the adapter is Send + Sync friendly.
pub struct WsStream {
    read_rx: mpsc::Receiver<bytes::Bytes>,
    write_tx: mpsc::Sender<bytes::Bytes>,
    read_buf: BytesMut,
    write_reserve: Option<ReserveFut<bytes::Bytes>>,
}

impl WsStream {
    pub fn new(socket: WebSocket) -> Self {
        let (ws_sink, ws_stream) = socket.split();
        let (read_tx, read_rx) = mpsc::channel::<bytes::Bytes>(256);
        let (write_tx, write_rx) = mpsc::channel::<bytes::Bytes>(256);

        // Spawn read loop: WS → channel
        tokio::spawn(Self::read_loop(ws_stream, read_tx));
        // Spawn write loop: channel → WS
        tokio::spawn(Self::write_loop(ws_sink, write_rx));

        Self {
            read_rx,
            write_tx,
            read_buf: BytesMut::new(),
            write_reserve: None,
        }
    }

    async fn read_loop(mut ws_stream: SplitStream<WebSocket>, tx: mpsc::Sender<bytes::Bytes>) {
        while let Some(Ok(msg)) = ws_stream.next().await {
            match msg {
                Message::Binary(data) => {
                    if tx.send(data).await.is_err() {
                        break;
                    }
                }
                Message::Close(_) => break,
                _ => {} // Ignore text, ping, pong
            }
        }
    }

    async fn write_loop(
        mut ws_sink: SplitSink<WebSocket, Message>,
        mut rx: mpsc::Receiver<bytes::Bytes>,
    ) {
        while let Some(data) = rx.recv().await {
            if ws_sink.send(Message::Binary(data)).await.is_err() {
                break;
            }
        }
        let _ = ws_sink.close().await;
    }
}

impl AsyncRead for WsStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // Drain buffered data first
        if !self.read_buf.is_empty() {
            let to_copy = self.read_buf.len().min(buf.remaining());
            buf.put_slice(&self.read_buf[..to_copy]);
            self.read_buf.advance(to_copy);
            return Poll::Ready(Ok(()));
        }

        // Try to receive from channel
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

impl AsyncWrite for WsStream {
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
                permit.send(bytes::Bytes::copy_from_slice(buf));
                Poll::Ready(Ok(buf.len()))
            }
            Poll::Ready(Err(_)) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "WebSocket closed",
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
