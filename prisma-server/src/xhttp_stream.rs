/// Adapter that bridges HTTP request/response body streams into AsyncRead + AsyncWrite.
/// Used by XHTTP transport modes (packet-up, stream-up, stream-one).
pub type XhttpStream = crate::channel_stream::ChannelStream;
