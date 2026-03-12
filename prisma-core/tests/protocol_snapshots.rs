use prisma_core::protocol::codec::*;
use prisma_core::protocol::types::*;
use prisma_core::types::*;

use std::net::Ipv4Addr;

#[test]
fn test_client_hello_snapshot() {
    let msg = ClientHello {
        version: PROTOCOL_VERSION,
        client_ephemeral_pub: [0xAA; 32],
        timestamp: 1700000000,
        padding: vec![],
    };
    let encoded = encode_client_hello(&msg);
    insta::assert_yaml_snapshot!("client_hello_wire", encoded);
}

#[test]
fn test_server_hello_snapshot() {
    let msg = ServerHello {
        server_ephemeral_pub: [0xBB; 32],
        encrypted_challenge: vec![0x01, 0x02, 0x03, 0x04],
        padding: vec![],
    };
    let encoded = encode_server_hello(&msg);
    insta::assert_yaml_snapshot!("server_hello_wire", encoded);
}

#[test]
fn test_data_frame_connect_snapshot() {
    let frame = DataFrame {
        command: Command::Connect(ProxyDestination {
            address: ProxyAddress::Ipv4(Ipv4Addr::new(93, 184, 216, 34)),
            port: 443,
        }),
        flags: 0,
        stream_id: 1,
    };
    let encoded = encode_data_frame(&frame);
    insta::assert_yaml_snapshot!("data_frame_connect_wire", encoded);
}

#[test]
fn test_data_frame_domain_connect_snapshot() {
    let frame = DataFrame {
        command: Command::Connect(ProxyDestination {
            address: ProxyAddress::Domain("example.com".into()),
            port: 80,
        }),
        flags: 0,
        stream_id: 0,
    };
    let encoded = encode_data_frame(&frame);
    insta::assert_yaml_snapshot!("data_frame_domain_connect_wire", encoded);
}

#[test]
fn test_data_frame_data_snapshot() {
    let frame = DataFrame {
        command: Command::Data(b"GET / HTTP/1.1\r\n\r\n".to_vec()),
        flags: 0,
        stream_id: 1,
    };
    let encoded = encode_data_frame(&frame);
    insta::assert_yaml_snapshot!("data_frame_data_wire", encoded);
}

#[test]
fn test_server_accept_snapshot() {
    let msg = ServerAccept {
        status: AcceptStatus::Ok,
        session_id: uuid::Uuid::nil(),
    };
    let encoded = encode_server_accept(&msg);
    insta::assert_yaml_snapshot!("server_accept_wire", encoded);
}
