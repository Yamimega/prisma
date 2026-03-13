//! TUN device abstraction for system-wide proxying.
//!
//! In TUN mode, all traffic on the system is captured via a virtual network
//! interface and routed through the PrismaVeil tunnel.
//!
//! Architecture:
//! ```text
//! Applications → OS routing table → TUN device → Prisma TUN handler
//!     → TCP packets → smoltcp TCP stack → CMD_CONNECT through tunnel
//!     → UDP packets → PrismaUDP relay through tunnel
//! ```

/// Re-export the TUN config from prisma-core.
pub use prisma_core::config::client::TunConfig;

pub mod device;
pub mod handler;
pub mod tcp_stack;

/// IP packet header parsing utilities.
pub mod packet {
    use std::net::{Ipv4Addr, SocketAddr};

    /// Parsed info from an IPv4 packet header.
    #[derive(Debug, Clone)]
    pub struct Ipv4Info {
        pub src: Ipv4Addr,
        pub dst: Ipv4Addr,
        pub protocol: u8,
        pub payload_offset: usize,
        pub total_len: usize,
    }

    /// IP protocol numbers.
    pub const PROTO_TCP: u8 = 6;
    pub const PROTO_UDP: u8 = 17;

    /// Parse an IPv4 packet header.
    pub fn parse_ipv4(data: &[u8]) -> Option<Ipv4Info> {
        if data.len() < 20 {
            return None;
        }
        let version = data[0] >> 4;
        if version != 4 {
            return None;
        }
        let ihl = (data[0] & 0x0f) as usize;
        let header_len = ihl * 4;
        if data.len() < header_len {
            return None;
        }
        let total_len = u16::from_be_bytes([data[2], data[3]]) as usize;
        let protocol = data[9];
        let src = Ipv4Addr::new(data[12], data[13], data[14], data[15]);
        let dst = Ipv4Addr::new(data[16], data[17], data[18], data[19]);

        Some(Ipv4Info {
            src,
            dst,
            protocol,
            payload_offset: header_len,
            total_len,
        })
    }

    /// Extract TCP destination from an IPv4/TCP packet.
    pub fn tcp_dest(data: &[u8]) -> Option<SocketAddr> {
        let ip = parse_ipv4(data)?;
        if ip.protocol != PROTO_TCP {
            return None;
        }
        let tcp = &data[ip.payload_offset..];
        if tcp.len() < 4 {
            return None;
        }
        let dst_port = u16::from_be_bytes([tcp[2], tcp[3]]);
        Some(SocketAddr::new(ip.dst.into(), dst_port))
    }

    /// Extract UDP destination from an IPv4/UDP packet.
    pub fn udp_dest(data: &[u8]) -> Option<SocketAddr> {
        let ip = parse_ipv4(data)?;
        if ip.protocol != PROTO_UDP {
            return None;
        }
        let udp = &data[ip.payload_offset..];
        if udp.len() < 4 {
            return None;
        }
        let dst_port = u16::from_be_bytes([udp[2], udp[3]]);
        Some(SocketAddr::new(ip.dst.into(), dst_port))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_ipv4_valid() {
        // Minimal IPv4 header: version=4, IHL=5, total_len=40
        let mut pkt = vec![0u8; 40];
        pkt[0] = 0x45; // version=4, IHL=5
        pkt[2] = 0;
        pkt[3] = 40; // total_len = 40
        pkt[9] = 6; // TCP
        pkt[12..16].copy_from_slice(&[10, 0, 0, 1]); // src
        pkt[16..20].copy_from_slice(&[192, 168, 1, 1]); // dst

        let info = super::packet::parse_ipv4(&pkt).unwrap();
        assert_eq!(info.src, "10.0.0.1".parse::<std::net::Ipv4Addr>().unwrap());
        assert_eq!(
            info.dst,
            "192.168.1.1".parse::<std::net::Ipv4Addr>().unwrap()
        );
        assert_eq!(info.protocol, 6);
        assert_eq!(info.payload_offset, 20);
    }

    #[test]
    fn test_parse_ipv4_too_short() {
        assert!(super::packet::parse_ipv4(&[0u8; 10]).is_none());
    }

    #[test]
    fn test_tcp_dest() {
        let mut pkt = vec![0u8; 44]; // 20 IP + 24 TCP
        pkt[0] = 0x45;
        pkt[2] = 0;
        pkt[3] = 44;
        pkt[9] = 6; // TCP
        pkt[16..20].copy_from_slice(&[1, 2, 3, 4]); // dst IP
                                                    // TCP header at offset 20
        pkt[22] = 0x01; // dst port high byte
        pkt[23] = 0xBB; // dst port low byte = 443

        let dest = super::packet::tcp_dest(&pkt).unwrap();
        assert_eq!(dest.port(), 443);
        assert_eq!(dest.ip().to_string(), "1.2.3.4");
    }

    #[test]
    fn test_udp_dest() {
        let mut pkt = vec![0u8; 28]; // 20 IP + 8 UDP
        pkt[0] = 0x45;
        pkt[2] = 0;
        pkt[3] = 28;
        pkt[9] = 17; // UDP
        pkt[16..20].copy_from_slice(&[8, 8, 8, 8]); // dst IP
                                                    // UDP header at offset 20
        pkt[22] = 0x00;
        pkt[23] = 53; // dst port = 53

        let dest = super::packet::udp_dest(&pkt).unwrap();
        assert_eq!(dest.port(), 53);
        assert_eq!(dest.ip().to_string(), "8.8.8.8");
    }
}
