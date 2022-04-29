use std::net::IpAddr;
use std::time::Duration;
use super::node::Node;

pub const BASE_DST_PORT: u16 = 33435;

#[derive(Clone, Debug)]
pub struct Tracer {
    /// Source IP Address
    pub src_ip: IpAddr,
    /// Destination IP Address
    pub dst_ip: IpAddr,
    /// Max hop
    pub max_hop: u8,
    /// Timeout setting for trace   
    pub trace_timeout: Duration,
    /// Timeout setting for packet receive  
    pub receive_timeout: Duration,
    /// Packet send rate
    pub send_rate: Duration,
    /// Result of probes  
    pub trace_result: Vec<Node>,
}

impl Tracer {
    pub fn new(dst_ip: IpAddr) -> Result<Tracer, String> {
        match default_net::get_default_interface(){
            Ok(interface) => {
                let src_ip: IpAddr = 
                if interface.ipv4.len() > 0 {
                    IpAddr::V4(interface.ipv4[0].addr)
                }else{
                    if interface.ipv6.len() > 0 {
                        IpAddr::V6(interface.ipv6[0].addr)
                    }else{
                        return Err(String::from("Failed to get default interface"));
                    }
                };
                let tracer = Tracer {
                    src_ip: src_ip,
                    dst_ip: dst_ip,
                    max_hop: 64,
                    trace_timeout: Duration::from_millis(30000),
                    receive_timeout: Duration::from_millis(1000),
                    send_rate: Duration::from_millis(1000),
                    trace_result: vec![],
                };
                return Ok(tracer);
            },
            Err(e) => {
                return Err(format!("{}",e));
            },
        }
    }
    pub fn trace(&self) -> Result<Vec<Node>, String> {
        super::trace_route(self.src_ip, self.dst_ip, self.max_hop, self.receive_timeout)
    }
}