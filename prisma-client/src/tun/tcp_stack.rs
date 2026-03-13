//! smoltcp-based userspace TCP/IP stack for TUN mode.
//!
//! Processes raw IP packets from the TUN device and converts TCP connections
//! into byte streams that can be relayed through PrismaVeil tunnels.
//!
//! Architecture:
//! ```text
//! TUN device ←→ smoltcp Interface ←→ TCP sockets ←→ PrismaVeil tunnel
//! ```

use std::collections::{HashMap, VecDeque};
use std::net::{Ipv4Addr, SocketAddr};

use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet};
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::socket::tcp::{self, State as TcpState};
use smoltcp::time::Instant as SmolInstant;
use smoltcp::wire::{HardwareAddress, IpAddress, IpCidr, Ipv4Address};
use tracing::{debug, trace};

/// Get current time as a smoltcp Instant.
fn smol_now() -> SmolInstant {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    SmolInstant::from_millis(elapsed.as_millis() as i64)
}

/// Packet queue for communication between TUN device I/O and smoltcp.
#[derive(Default)]
pub struct PacketQueue {
    /// Packets received from TUN device, waiting to be ingested by smoltcp.
    rx_queue: VecDeque<Vec<u8>>,
    /// Packets produced by smoltcp, waiting to be written to TUN device.
    tx_queue: Vec<Vec<u8>>,
}

impl PacketQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a packet received from the TUN device.
    pub fn push_rx(&mut self, packet: Vec<u8>) {
        self.rx_queue.push_back(packet);
    }

    /// Pop all packets that smoltcp wants to send to the TUN device.
    pub fn drain_tx(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.tx_queue)
    }
}

/// A virtual network device that bridges smoltcp with our packet queue.
struct VirtualDevice<'a> {
    queue: &'a mut PacketQueue,
    mtu: usize,
}

impl<'a> Device for VirtualDevice<'a> {
    type RxToken<'b>
        = VirtualRxToken
    where
        Self: 'b;
    type TxToken<'b>
        = VirtualTxToken<'b>
    where
        Self: 'b;

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ip;
        caps.max_transmission_unit = self.mtu;
        caps
    }

    fn receive(
        &mut self,
        _timestamp: SmolInstant,
    ) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let packet = self.queue.rx_queue.pop_front()?;
        Some((
            VirtualRxToken { buffer: packet },
            VirtualTxToken {
                queue: &mut self.queue.tx_queue,
            },
        ))
    }

    fn transmit(&mut self, _timestamp: SmolInstant) -> Option<Self::TxToken<'_>> {
        Some(VirtualTxToken {
            queue: &mut self.queue.tx_queue,
        })
    }
}

struct VirtualRxToken {
    buffer: Vec<u8>,
}

impl RxToken for VirtualRxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.buffer)
    }
}

struct VirtualTxToken<'a> {
    queue: &'a mut Vec<Vec<u8>>,
}

impl<'a> TxToken for VirtualTxToken<'a> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buffer = vec![0u8; len];
        let result = f(&mut buffer);
        self.queue.push(buffer);
        result
    }
}

/// Tracks a TCP connection through smoltcp.
pub struct TcpConnection {
    pub handle: SocketHandle,
    pub dest: SocketAddr,
    pub domain: Option<String>,
    /// Data received from the remote side via tunnel, to be sent into smoltcp socket.
    pub from_tunnel: Vec<u8>,
    /// Data read from smoltcp socket, to be sent through the tunnel.
    pub to_tunnel: Vec<u8>,
    /// Whether the tunnel connection is established.
    pub tunnel_connected: bool,
}

/// The smoltcp-based TCP/IP stack.
pub struct TcpStack {
    iface: Interface,
    sockets: SocketSet<'static>,
    queue: PacketQueue,
    connections: HashMap<SocketHandle, TcpConnection>,
    mtu: usize,
    /// Local IP assigned to the TUN interface.
    #[allow(dead_code)]
    local_ip: Ipv4Addr,
}

impl TcpStack {
    /// Create a new TCP stack with the given local IP and MTU.
    pub fn new(local_ip: Ipv4Addr, mtu: u16) -> Self {
        let config = Config::new(HardwareAddress::Ip);
        let mut iface = Interface::new(config, &mut DummyDevice { mtu: mtu as usize }, smol_now());

        // Add the local IP address
        let octets = local_ip.octets();
        let ip_addr = IpCidr::new(
            IpAddress::Ipv4(Ipv4Address::new(octets[0], octets[1], octets[2], octets[3])),
            0,
        );
        iface.update_ip_addrs(|addrs| {
            addrs.push(ip_addr).ok();
        });

        // Set default gateway (any IP works since we're routing everything through TUN)
        iface
            .routes_mut()
            .add_default_ipv4_route(Ipv4Address::new(0, 0, 0, 1))
            .ok();

        // Preallocate socket storage for up to 64 concurrent connections
        let socket_storage: Vec<smoltcp::iface::SocketStorage<'static>> = (0..64)
            .map(|_| smoltcp::iface::SocketStorage::EMPTY)
            .collect();
        let sockets = SocketSet::new(socket_storage);

        Self {
            iface,
            sockets,
            queue: PacketQueue::new(),
            connections: HashMap::new(),
            mtu: mtu as usize,
            local_ip,
        }
    }

    /// Feed a raw IP packet from the TUN device into the stack.
    pub fn receive_packet(&mut self, packet: &[u8]) {
        self.queue.push_rx(packet.to_vec());
    }

    /// Poll the stack, processing any pending packets and advancing TCP state machines.
    /// Returns packets that need to be written back to the TUN device.
    pub fn poll(&mut self) -> Vec<Vec<u8>> {
        let timestamp = smol_now();
        let mut device = VirtualDevice {
            queue: &mut self.queue,
            mtu: self.mtu,
        };

        self.iface.poll(timestamp, &mut device, &mut self.sockets);

        self.queue.drain_tx()
    }

    /// Create a new listening TCP socket for an incoming connection.
    /// Returns the socket handle.
    pub fn accept_connection(&mut self, dest: SocketAddr, domain: Option<String>) -> SocketHandle {
        let rx_buf = tcp::SocketBuffer::new(vec![0u8; 65536]);
        let tx_buf = tcp::SocketBuffer::new(vec![0u8; 65536]);
        let socket = tcp::Socket::new(rx_buf, tx_buf);

        let handle = self.sockets.add(socket);

        // Put socket into listen state
        let socket = self.sockets.get_mut::<tcp::Socket>(handle);
        socket.listen(dest.port()).ok();

        self.connections.insert(
            handle,
            TcpConnection {
                handle,
                dest,
                domain,
                from_tunnel: Vec::new(),
                to_tunnel: Vec::new(),
                tunnel_connected: false,
            },
        );

        debug!(dest = %dest, "TCP socket listening");
        handle
    }

    /// Read data from a TCP socket (data received from the application via TUN).
    pub fn read_from_socket(&mut self, handle: SocketHandle, buf: &mut [u8]) -> usize {
        let socket = self.sockets.get_mut::<tcp::Socket>(handle);
        socket.recv_slice(buf).unwrap_or_default()
    }

    /// Write data to a TCP socket (data to be sent to the application via TUN).
    pub fn write_to_socket(&mut self, handle: SocketHandle, data: &[u8]) -> usize {
        let socket = self.sockets.get_mut::<tcp::Socket>(handle);
        socket.send_slice(data).unwrap_or_default()
    }

    /// Check if a TCP socket is in an established state.
    pub fn is_established(&self, handle: SocketHandle) -> bool {
        let socket = self.sockets.get::<tcp::Socket>(handle);
        socket.state() == TcpState::Established
    }

    /// Check if a TCP socket is closed or closing.
    pub fn is_closed(&self, handle: SocketHandle) -> bool {
        let socket = self.sockets.get::<tcp::Socket>(handle);
        matches!(socket.state(), TcpState::Closed | TcpState::TimeWait)
    }

    /// Check if a socket can send data.
    pub fn can_send(&self, handle: SocketHandle) -> bool {
        let socket = self.sockets.get::<tcp::Socket>(handle);
        socket.can_send()
    }

    /// Check if a socket can receive data.
    pub fn can_recv(&self, handle: SocketHandle) -> bool {
        let socket = self.sockets.get::<tcp::Socket>(handle);
        socket.can_recv()
    }

    /// Close a TCP socket.
    pub fn close_socket(&mut self, handle: SocketHandle) {
        let socket = self.sockets.get_mut::<tcp::Socket>(handle);
        socket.close();
        self.connections.remove(&handle);
    }

    /// Get all socket handles.
    pub fn connection_handles(&self) -> Vec<SocketHandle> {
        self.connections.keys().copied().collect()
    }

    /// Get connection info for a socket.
    pub fn get_connection(&self, handle: SocketHandle) -> Option<&TcpConnection> {
        self.connections.get(&handle)
    }

    /// Get mutable connection info for a socket.
    pub fn get_connection_mut(&mut self, handle: SocketHandle) -> Option<&mut TcpConnection> {
        self.connections.get_mut(&handle)
    }

    /// Remove closed connections.
    pub fn cleanup_closed(&mut self) -> Vec<SocketHandle> {
        let closed: Vec<SocketHandle> = self
            .connections
            .keys()
            .copied()
            .filter(|h| self.is_closed(*h))
            .collect();

        for handle in &closed {
            self.connections.remove(handle);
            self.sockets.remove(*handle);
            trace!("Cleaned up closed TCP socket");
        }

        closed
    }
}

/// Dummy device used only for Interface initialization (smoltcp requires a device for `new()`).
struct DummyDevice {
    mtu: usize,
}

impl Device for DummyDevice {
    type RxToken<'a>
        = VirtualRxToken
    where
        Self: 'a;
    type TxToken<'a>
        = DummyTxToken
    where
        Self: 'a;

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ip;
        caps.max_transmission_unit = self.mtu;
        caps
    }

    fn receive(
        &mut self,
        _timestamp: SmolInstant,
    ) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        None
    }

    fn transmit(&mut self, _timestamp: SmolInstant) -> Option<Self::TxToken<'_>> {
        Some(DummyTxToken)
    }
}

struct DummyTxToken;

impl TxToken for DummyTxToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buffer = vec![0u8; len];
        f(&mut buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tcp_stack_creation() {
        let stack = TcpStack::new(Ipv4Addr::new(10, 0, 0, 1), 1500);
        assert_eq!(stack.local_ip, Ipv4Addr::new(10, 0, 0, 1));
        assert_eq!(stack.mtu, 1500);
        assert!(stack.connections.is_empty());
    }

    #[test]
    fn test_packet_queue() {
        let mut queue = PacketQueue::new();
        assert!(queue.rx_queue.is_empty());

        queue.push_rx(vec![1, 2, 3]);
        assert_eq!(queue.rx_queue.len(), 1);

        queue.tx_queue.push(vec![4, 5, 6]);
        let tx = queue.drain_tx();
        assert_eq!(tx.len(), 1);
        assert_eq!(tx[0], vec![4, 5, 6]);
        assert!(queue.tx_queue.is_empty());
    }

    #[test]
    fn test_accept_connection() {
        let mut stack = TcpStack::new(Ipv4Addr::new(10, 0, 0, 1), 1500);
        let dest: SocketAddr = "1.2.3.4:443".parse().unwrap();
        let handle = stack.accept_connection(dest, Some("example.com".into()));

        assert!(stack.get_connection(handle).is_some());
        let conn = stack.get_connection(handle).unwrap();
        assert_eq!(conn.dest, dest);
        assert_eq!(conn.domain.as_deref(), Some("example.com"));
        assert!(!conn.tunnel_connected);
    }

    #[test]
    fn test_cleanup_closed() {
        let mut stack = TcpStack::new(Ipv4Addr::new(10, 0, 0, 1), 1500);
        let dest: SocketAddr = "1.2.3.4:443".parse().unwrap();
        let _handle = stack.accept_connection(dest, None);

        // New sockets start in Listen state, not Closed
        let closed = stack.cleanup_closed();
        assert!(closed.is_empty());
    }

    #[test]
    fn test_poll_empty() {
        let mut stack = TcpStack::new(Ipv4Addr::new(10, 0, 0, 1), 1500);
        let packets = stack.poll();
        assert!(packets.is_empty());
    }
}
