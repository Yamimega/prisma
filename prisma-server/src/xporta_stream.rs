/// Server-side XPorta stream adapter.
/// Bridges the reassembled upload data and download channel into AsyncRead + AsyncWrite,
/// so the PrismaVeil protocol handler can use it like any other transport.
pub type XPortaServerStream = crate::channel_stream::ChannelStream;
