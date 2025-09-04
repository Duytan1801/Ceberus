use pcap::{Device, Capture};
use etherparse::SlicedPacket;
use std::collections::HashMap;

pub struct CaptureOptions {
    pub interface: Option<String>,
    pub filter: Option<String>,
    pub promiscuous: bool,
    pub output_file: Option<String>,
    pub packet_limit: Option<u32>,
    pub verbose: bool,
}

pub struct CaptureStats {
    pub packet_count: u32,
    pub protocol_stats: HashMap<String, u32>,
}

pub fn list_interfaces() -> Result<(), Box<dyn std::error::Error>> {
    println!("Available network interfaces:");
    let devices = Device::list()?;
    for device in devices {
        println!("- {}: {}", device.name, device.desc.as_deref().unwrap_or("No description"));
    }
    Ok(())
}

pub fn auto_detect_wifi_device() -> Result<Device, Box<dyn std::error::Error>> {
    let devices = Device::list()?;
    
    // Try to find a Wi-Fi device
    let wifi_device = devices.into_iter()
        .find(|dev| {
            let desc = dev.desc.as_deref().unwrap_or("").to_lowercase();
            desc.contains("wireless") || 
            desc.contains("wi-fi") || 
            desc.contains("802.11") ||
            dev.name.contains("NativeWiFi") ||
            dev.name.contains("Wireless")
        })
        .or_else(|| {
            // Fallback: get the first non-loopback, non-virtual adapter
            Device::list()
                .ok()?
                .into_iter()
                .find(|dev| !dev.name.contains("Loopback") && 
                            !dev.desc.as_deref().unwrap_or("").contains("Miniport") &&
                            !dev.desc.as_deref().unwrap_or("").contains("Virtual"))
        })
        .ok_or("No suitable network device found")?;

    Ok(wifi_device)
}

pub fn start_capture(options: CaptureOptions) -> Result<CaptureStats, Box<dyn std::error::Error>> {
    // Get network interface
    let device = if let Some(interface_name) = options.interface {
        Device::list()?
            .into_iter()
            .find(|dev| dev.name == interface_name)
            .ok_or(format!("Interface '{}' not found", interface_name))?
    } else {
        // Auto-detect Wi-Fi device
        auto_detect_wifi_device()?
    };

    println!("Using device: {} ({:?})", device.name, device.desc);

    // Configure capture
    let mut cap_builder = Capture::from_device(device)?;
    
    if options.promiscuous {
        cap_builder = cap_builder.promisc(true);
    }
    
    cap_builder = cap_builder.timeout(1000);
    
    let mut cap = cap_builder.open()?;

    // Apply filter if specified
    if let Some(filter_expr) = options.filter {
        cap.filter(&filter_expr, true)?;
        println!("Applied filter: {}", filter_expr);
    }

    // Prepare output file if specified
    let mut savefile = if let Some(output_file) = options.output_file {
        Some(cap.savefile(&output_file)?)
    } else {
        None
    };

    println!("Starting packet capture... Press Ctrl+C to stop.");

    let mut packet_count = 0;
    let mut protocol_stats: HashMap<String, u32> = HashMap::new();

    loop {
        // Check if we've reached the packet limit
        if let Some(limit) = options.packet_limit {
            if packet_count >= limit {
                break;
            }
        }

        match cap.next_packet() {
            Ok(packet) => {
                packet_count += 1;
                
                // Save to file if requested
                if let Some(savefile) = &mut savefile {
                    savefile.write(&packet);
                }

                // Parse packet with etherparse
                match SlicedPacket::from_ethernet(&packet.data) {
                    Ok(sliced_packet) => {
                        let (protocol_name, src_addr, dst_addr, src_port, dst_port) = 
                            parse_packet_with_etherparse(&sliced_packet);
                        
                        // Update protocol statistics
                        *protocol_stats.entry(protocol_name.clone()).or_insert(0) += 1;
                        
                        // Display packet information
                        if options.verbose {
                            println!("Packet #{}, Length: {} bytes", packet_count, packet.header.len);
                            println!("  Protocol: {}", protocol_name);
                            println!("  Source: {}:{}", src_addr, src_port.unwrap_or_default());
                            println!("  Destination: {}:{}", dst_addr, dst_port.unwrap_or_default());
                            print_protocol_details(&sliced_packet);
                            println!("----------------------------------------");
                        } else if packet_count % 10 == 0 {
                            // Show brief status update every 10 packets
                            println!("Captured {} packets...", packet_count);
                        }
                    }
                    Err(e) => {
                        if options.verbose {
                            println!("Packet #{}: Error parsing packet - {}", packet_count, e);
                        }
                    }
                }
            }
            Err(pcap::Error::TimeoutExpired) => {
                // Timeout is expected, just continue
                continue;
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    }

    Ok(CaptureStats {
        packet_count,
        protocol_stats,
    })
}

fn parse_packet_with_etherparse(sliced_packet: &SlicedPacket) -> (String, String, String, Option<u16>, Option<u16>) {
    let mut protocol_name = "Unknown".to_string();
    let mut src_addr = "N/A".to_string();
    let mut dst_addr = "N/A".to_string();
    let mut src_port = None;
    let mut dst_port = None;

    // Determine network layer protocol
    if let Some(net) = &sliced_packet.net {
        match net {
            etherparse::NetSlice::Ipv4(ipv4) => {
                src_addr = ipv4.header().source_addr().to_string();
                dst_addr = ipv4.header().destination_addr().to_string();
                
                // Check transport layer protocol
                if let Some(trans) = &sliced_packet.transport {
                    match trans {
                        etherparse::TransportSlice::Tcp(tcp) => {
                            protocol_name = "TCP".to_string();
                            src_port = Some(tcp.source_port());
                            dst_port = Some(tcp.destination_port());
                        }
                        etherparse::TransportSlice::Udp(udp) => {
                            protocol_name = "UDP".to_string();
                            src_port = Some(udp.source_port());
                            dst_port = Some(udp.destination_port());
                        }
                        etherparse::TransportSlice::Icmpv4(icmp) => {
                            protocol_name = format!("ICMPv4 ({:?})", icmp.icmp_type());
                        }
                        etherparse::TransportSlice::Icmpv6(icmp) => {
                            protocol_name = format!("ICMPv6 ({:?})", icmp.icmp_type());
                        }
                        // Removed the unreachable catch-all pattern
                    }
                } else {
                    protocol_name = "IPv4".to_string();
                }
            }
            etherparse::NetSlice::Ipv6(ipv6) => {
                src_addr = ipv6.header().source_addr().to_string();
                dst_addr = ipv6.header().destination_addr().to_string();
                
                if let Some(trans) = &sliced_packet.transport {
                    match trans {
                        etherparse::TransportSlice::Tcp(tcp) => {
                            protocol_name = "TCP/IPv6".to_string();
                            src_port = Some(tcp.source_port());
                            dst_port = Some(tcp.destination_port());
                        }
                        etherparse::TransportSlice::Udp(udp) => {
                            protocol_name = "UDP/IPv6".to_string();
                            src_port = Some(udp.source_port());
                            dst_port = Some(udp.destination_port());
                        }
                        _ => {
                            protocol_name = "IPv6 (Other Transport)".to_string();
                        }
                    }
                } else {
                    protocol_name = "IPv6".to_string();
                }
            }
            etherparse::NetSlice::Arp(_arp) => { // Fixed: added underscore to indicate unused variable
                protocol_name = "ARP".to_string();
            }
        }
    } else if sliced_packet.link.is_some() {
        // Link layer protocols
        protocol_name = match sliced_packet.vlan() {
            Some(_) => "VLAN".to_string(),
            None => "Ethernet".to_string(),
        };
    }
    
    (protocol_name, src_addr, dst_addr, src_port, dst_port)
}

fn print_protocol_details(sliced_packet: &SlicedPacket) {
    if let Some(trans) = &sliced_packet.transport {
        match trans {
            etherparse::TransportSlice::Tcp(tcp) => {
                let tcp_header = tcp.to_header();
                println!("  TCP Flags: FIN={}, SYN={}, RST={}, PSH={}, ACK={}, URG={}, ECE={}, CWR={}",
                    tcp_header.fin, tcp_header.syn, tcp_header.rst, tcp_header.psh,
                    tcp_header.ack, tcp_header.urg, tcp_header.ece, tcp_header.cwr);
                println!("  Sequence Number: {}", tcp.sequence_number());
                println!("  Acknowledgment Number: {}", tcp.acknowledgment_number());
                println!("  Window Size: {}", tcp.window_size());
            }
            etherparse::TransportSlice::Udp(udp) => {
                println!("  UDP Length: {}", udp.length());
                println!("  Checksum: 0x{:04x}", udp.checksum());
            }
            _ => {}
        }
    }
    
    // Print VLAN information if present
    if let Some(_vlan) = sliced_packet.vlan() { // Fixed: added underscore to indicate unused variable
        println!("  VLAN present");
    }
}