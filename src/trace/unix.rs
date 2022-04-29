use std::net::IpAddr;
use std::time::{Duration, Instant};
use socket2::{Domain, Protocol, Socket, Type};
use std::mem::MaybeUninit;
use std::net::{SocketAddr, UdpSocket};
use std::collections::HashSet;
use pnet_packet::Packet;
use pnet_packet::icmp::IcmpTypes;
use super::node::{NodeType, Node};
use super::BASE_DST_PORT;

pub fn trace_route(src_ip: IpAddr, dst_ip: IpAddr, max_hop: u8, receive_timeout: Duration) -> Result<Vec<Node>, String> {
    let mut result: Vec<Node> = vec![];
    let udp_socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(e) => {
            return Err(format!("{}", e));
        },
    };
    let icmp_socket: Socket = 
    if src_ip.is_ipv4() {
        Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4)).unwrap()
    }else if src_ip.is_ipv6(){
        Socket::new(Domain::IPV6, Type::RAW, Some(Protocol::ICMPV6)).unwrap()
    }else{
        return Err(String::from("invalid source address"));
    };
    icmp_socket.set_read_timeout(Some(receive_timeout)).unwrap();
    let mut ip_set: HashSet<IpAddr> = HashSet::new();
    for ttl in 1..max_hop {
        match udp_socket.set_ttl(ttl as u32) {
            Ok(_) => (),
            Err(e) => {
                return Err(format!("{}", e));
            },
        }
        let udp_buf = [0u8; 0];
        let mut buf: Vec<u8> = vec![0; 512];
        let mut recv_buf = unsafe { &mut *(buf.as_mut_slice() as *mut [u8] as *mut [MaybeUninit<u8>]) };
        let dst: SocketAddr = SocketAddr::new(dst_ip, BASE_DST_PORT + ttl as u16);
        let send_time = Instant::now();
        match udp_socket.send_to(&udp_buf, dst) {
            Ok(_) => (),
            Err(e) => {
                return Err(format!("{}", e));
            },
        }
        match icmp_socket.recv_from(&mut recv_buf) {
            Ok((bytes_len, addr)) => {
                let src_addr: IpAddr = addr.as_socket().unwrap_or(SocketAddr::new(src_ip, 0)).ip();
                if ip_set.contains(&src_addr) {
                    continue;
                }
                let recv_time = Instant::now().duration_since(send_time);
                let recv_buf = unsafe { *(recv_buf as *mut [MaybeUninit<u8>] as *mut [u8; 512]) };
                if let Some(packet) = pnet_packet::ipv4::Ipv4Packet::new(&recv_buf[0..bytes_len]){
                    let icmp_packet = pnet_packet::icmp::IcmpPacket::new(packet.payload());
                    if let Some(icmp) = icmp_packet {
                        let ip_addr: IpAddr = IpAddr::V4(packet.get_source());
                        //let host_name: String = dns_lookup::lookup_addr(&ip_addr).unwrap_or(ip_addr.to_string());
                        match icmp.get_icmp_type() {
                            IcmpTypes::TimeExceeded => {
                                result.push(Node {
                                    ip_addr: ip_addr,
                                    host_name: String::new(),
                                    hop: ttl,
                                    node_type: if ttl == 1 {NodeType::DefaultGateway}else{NodeType::Relay},
                                    rtt: recv_time,
                                });
                                ip_set.insert(ip_addr);
                            },
                            IcmpTypes::DestinationUnreachable => {
                                result.push(Node {
                                    ip_addr: ip_addr,
                                    host_name: String::new(),
                                    hop: ttl,
                                    node_type: NodeType::Destination,
                                    rtt: recv_time,
                                });
                                break;
                            },
                            _ => {},
                        }
                    }
                }
            },
            Err(_) => {},
        }
    }
    for node in &mut result {
        let host_name: String = dns_lookup::lookup_addr(&node.ip_addr).unwrap_or(node.ip_addr.to_string());
        node.host_name = host_name;
    }
    Ok(result)
} 