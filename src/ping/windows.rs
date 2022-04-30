use std::time::{Instant, Duration};
use std::net::{SocketAddr, IpAddr, UdpSocket};
use std::mem::MaybeUninit;
use std::thread;
use socket2::{Domain, Protocol, Socket, Type, SockAddr};
use pnet_packet::Packet;
use pnet_packet::icmp::IcmpTypes;
use winapi::shared::ws2def::{AF_INET, AF_INET6, IPPROTO_IP};
use winapi::um::winsock2::{SOCKET, SOCK_RAW, SOL_SOCKET, SO_RCVTIMEO};

use crate::node::{NodeType, Node};
use crate::packet;
use super::{Pinger, PingStatus, PingResult};
use crate::protocol::Protocol as ProbeProtocol;
use crate::trace::BASE_DST_PORT;
use crate::sys;

fn icmp_ping(pinger: Pinger) -> Result<PingResult, String> {
    let host_name: String = dns_lookup::lookup_addr(&pinger.dst_ip).unwrap_or(pinger.dst_ip.to_string());
    let mut results: Vec<Node> = vec![];
    let icmp_socket: Socket = 
    if pinger.src_ip.is_ipv4() {
        Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4)).unwrap()
    }else if pinger.src_ip.is_ipv6(){
        Socket::new(Domain::IPV6, Type::RAW, Some(Protocol::ICMPV6)).unwrap()
    }else{
        return Err(String::from("invalid source address"));
    };
    icmp_socket.set_read_timeout(Some(pinger.receive_timeout)).unwrap();
    icmp_socket.set_ttl(pinger.ttl as u32).unwrap();
    let socket_addr = SocketAddr::new(pinger.dst_ip, 0);
    let sock_addr = SockAddr::from(socket_addr);
    let mut icmp_packet: Vec<u8> = packet::build_icmpv4_echo_packet();
    let start_time = Instant::now();
    let mut probe_time = Duration::from_millis(0);
    for seq in 1..pinger.count + 1 {
        probe_time = Instant::now().duration_since(start_time);
        if probe_time > pinger.ping_timeout {
            let result: PingResult = PingResult {
                results: results,
                status: PingStatus::Timeout,
                probe_time: probe_time,
            };
            return Ok(result);
        }
        let mut buf: Vec<u8> = vec![0; 512];
        let mut recv_buf = unsafe { &mut *(buf.as_mut_slice() as *mut [u8] as *mut [MaybeUninit<u8>]) };
        let send_time = Instant::now();
        match icmp_socket.send_to(&mut icmp_packet, &sock_addr) {
            Ok(_) => {},
            Err(_) => {},
        }
        loop {
            match icmp_socket.recv_from(&mut recv_buf) {
                Ok((bytes_len, _addr)) => {
                    let recv_time = Instant::now().duration_since(send_time);
                    if recv_time > pinger.receive_timeout {
                        break;
                    }
                    let recv_buf = unsafe { *(recv_buf as *mut [MaybeUninit<u8>] as *mut [u8; 512]) };
                    if let Some(packet) = pnet_packet::ipv4::Ipv4Packet::new(&recv_buf[0..bytes_len]){
                        let icmp_packet = pnet_packet::icmp::IcmpPacket::new(packet.payload());
                        if let Some(icmp) = icmp_packet {
                            let ip_addr: IpAddr = IpAddr::V4(packet.get_source());
                            match icmp.get_icmp_type() {
                                IcmpTypes::EchoReply => {
                                    results.push(Node {
                                        seq: seq,
                                        ip_addr: ip_addr,
                                        host_name: host_name.clone(),
                                        hop: Some(pinger.ttl - packet.get_ttl()),
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
        thread::sleep(pinger.send_rate);
    }
    let result: PingResult = PingResult {
        results: results,
        status: PingStatus::Done,
        probe_time: probe_time,
    };
    Ok(result)
}

fn tcp_ping(pinger: Pinger) -> Result<PingResult, String> {
    let host_name: String = dns_lookup::lookup_addr(&pinger.dst_ip).unwrap_or(pinger.dst_ip.to_string());
    let mut results: Vec<Node> = vec![];
    let socket_addr: SocketAddr = SocketAddr::new(pinger.dst_ip, pinger.dst_port);
    let sock_addr = SockAddr::from(socket_addr);
    let mut probe_time = Duration::from_millis(0);
    let start_time = Instant::now();
    for seq in 1..pinger.count + 1 {
        probe_time = Instant::now().duration_since(start_time);
        if probe_time > pinger.ping_timeout {
            let result: PingResult = PingResult {
                results: results,
                status: PingStatus::Timeout,
                probe_time: probe_time,
            };
            return Ok(result);
        }
        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();
        let connect_start_time = Instant::now();
        match socket.connect_timeout(&sock_addr, pinger.receive_timeout) {
            Ok(_) => {
                let connect_end_time = Instant::now().duration_since(connect_start_time);
                results.push(Node {
                    seq: seq,
                    ip_addr: pinger.dst_ip,
                    host_name: host_name.clone(),
                    hop: None,
                    node_type: NodeType::Destination,
                    rtt: connect_end_time,
                });
            },
            Err(e) => {
                println!("{}", e);
            },
        }
        thread::sleep(pinger.send_rate);
    }
    let result: PingResult = PingResult {
        results: results,
        status: PingStatus::Done,
        probe_time: probe_time,
    };
    Ok(result)
}

fn udp_ping(pinger: Pinger) -> Result<PingResult, String> {
    let host_name: String = dns_lookup::lookup_addr(&pinger.dst_ip).unwrap_or(pinger.dst_ip.to_string());
    let mut results: Vec<Node> = vec![];
    let udp_socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(e) => {
            return Err(format!("{}", e));
        },
    };
    let socket: SOCKET = 
    if pinger.src_ip.is_ipv4() {
        sys::create_socket(AF_INET, SOCK_RAW, IPPROTO_IP).unwrap()
    }else if pinger.src_ip.is_ipv6(){
        sys::create_socket(AF_INET6, SOCK_RAW, IPPROTO_IP).unwrap()
    }else{
        return Err(String::from("invalid source address"));
    };
    let socket_addr: SocketAddr = SocketAddr::new(pinger.src_ip, 0);
    //let socket_addr: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0);
    let sock_addr = SockAddr::from(socket_addr);
    sys::bind(socket, &sock_addr).unwrap();
    //sys::set_nonblocking(socket, true).unwrap();
    sys::set_promiscuous(socket, true).unwrap();
    sys::set_timeout_opt(socket, SOL_SOCKET, SO_RCVTIMEO, Some(pinger.receive_timeout)).unwrap();
    let start_time = Instant::now();
    let mut trace_time = Duration::from_millis(0);
    for seq in 1..pinger.count + 1 {
        trace_time = Instant::now().duration_since(start_time);
        if trace_time > pinger.ping_timeout {
            let result: PingResult = PingResult {
                results: results,
                status: PingStatus::Timeout,
                probe_time: trace_time,
            };
            return Ok(result);
        }
        match udp_socket.set_ttl(64) {
            Ok(_) => (),
            Err(e) => {
                return Err(format!("{}", e));
            },
        }
        let udp_buf = [0u8; 0];
        let dst: SocketAddr = SocketAddr::new(pinger.dst_ip, BASE_DST_PORT);
        let send_time = Instant::now();
        let mut buf: Vec<u8> = vec![0; 512];
        let recv_buf = unsafe { &mut *(buf.as_mut_slice() as *mut [u8] as *mut [MaybeUninit<u8>]) };
        match udp_socket.send_to(&udp_buf, dst) {
            Ok(_) => (),
            Err(e) => {
                return Err(format!("{}", e));
            },
        }
        loop {
            if Instant::now().duration_since(send_time) > pinger.receive_timeout {
                break;
            }
            match sys::recv_from(socket, recv_buf, 0) {
                Ok((bytes_len, addr)) => {
                    let src_addr: IpAddr = addr.as_socket().unwrap_or(SocketAddr::new(pinger.src_ip, 0)).ip();
                    if pinger.src_ip == src_addr {
                        continue;
                    }
                    let recv_time = Instant::now().duration_since(send_time);
                    let recv_buf = unsafe { *(recv_buf as *mut [MaybeUninit<u8>] as *mut [u8; 512]) };
                    if let Some(packet) = pnet_packet::ipv4::Ipv4Packet::new(&recv_buf[0..bytes_len]){
                        let icmp_packet = pnet_packet::icmp::IcmpPacket::new(packet.payload());
                        if let Some(icmp) = icmp_packet {
                            let ip_addr: IpAddr = IpAddr::V4(packet.get_source());
                            match icmp.get_icmp_type() {
                                IcmpTypes::DestinationUnreachable => {
                                    results.push(Node {
                                        seq: seq,
                                        ip_addr: ip_addr,
                                        host_name: host_name.clone(),
                                        hop: Some(pinger.ttl - packet.get_ttl()),
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
        thread::sleep(pinger.send_rate);
    }
    let result: PingResult = PingResult {
        results: results,
        status: PingStatus::Done,
        probe_time: trace_time,
    };
    Ok(result)
}

pub(crate) fn ping(pinger: Pinger) -> Result<PingResult, String> {
    match pinger.protocol {
        ProbeProtocol::Icmpv4 => {
            icmp_ping(pinger)
        },
        ProbeProtocol::Icmpv6 => {
            icmp_ping(pinger)
        },
        ProbeProtocol::Tcp => {
            tcp_ping(pinger)
        },
        ProbeProtocol::Udp => {
            udp_ping(pinger)
        },
    }
}

