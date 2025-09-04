mod listening;

use clap::{Arg, Command};
use listening::{CaptureOptions, list_interfaces}; // Removed unused imports
use std::process;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set console window size (Windows only)
    #[cfg(windows)]
    set_console_size(150, 30);
    
    // Parse command line arguments
    let matches = Command::new("PacketCaptureTool")
        .version("1.0")
        .author("Your Name")
        .about("A comprehensive network packet capture and analysis tool")
        .after_help(
            "_________                ___.                                  \n\
             ╲_   ___ ╲   ____ _______╲_ │__    ____ _______  __ __  ______ \n\
             ╱    ╲  ╲╱ _╱ __ ╲╲_  __ ╲│ __ ╲ _╱ __ ╲╲_  __ ╲│  │  ╲╱  ___╱ \n\
             ╲     ╲____╲  ___╱ │  │ ╲╱│ ╲_╲ ╲╲  ___╱ │  │ ╲╱│  │  ╱╲___ ╲  \n\
             ╲______  ╱ ╲___  >│__│   │___  ╱ ╲___  >│__│   │____╱╱____  > \n\
                    ╲╱      ╲╱            ╲╱      ╲╱                   ╲╱  \n\
                                                                           "
        )
        .arg(
            Arg::new("interface")
                .short('i')
                .long("interface")
                .value_name("INTERFACE")
                .help("Network interface to use (default: auto-detect)")
        )
        .arg(
            Arg::new("filter")
                .short('f')
                .long("filter")
                .value_name("FILTER")
                .help("BPF filter expression (e.g., 'udp', 'tcp port 80')")
        )
        .arg(
            Arg::new("promiscuous")
                .short('p')
                .long("promiscuous")
                .help("Enable promiscuous mode")
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("FILE")
                .help("Save packets to file (PCAP format)")
        )
        .arg(
            Arg::new("count")
                .short('c')
                .long("count")
                .value_name("NUM")
                .help("Stop after capturing NUM packets")
        )
        .arg(
            Arg::new("list")
                .short('l')
                .long("list")
                .help("List available interfaces and exit")
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Verbose output")
        )
        .get_matches();

    // List interfaces if requested
    if matches.contains_id("list") {
        if let Err(e) = list_interfaces() {
            eprintln!("Error listing interfaces: {}", e);
        }
        process::exit(0);
    }

    // Prepare capture options
    let options = CaptureOptions {
        interface: matches.get_one::<String>("interface").cloned(),
        filter: matches.get_one::<String>("filter").cloned(),
        promiscuous: matches.contains_id("promiscuous"),
        output_file: matches.get_one::<String>("output").cloned(),
        packet_limit: matches.get_one::<String>("count")
            .and_then(|s| s.parse::<u32>().ok()),
        verbose: matches.contains_id("verbose"),
    };

    // Start capture
    match listening::start_capture(options) {
        Ok(stats) => {
            // Display final statistics
            println!("\n=== Capture Summary ===");
            println!("Total packets captured: {}", stats.packet_count);
            println!("\n=== Protocol Statistics ===");
            for (protocol, count) in &stats.protocol_stats {
                println!("{}: {}", protocol, count);
            }
        }
        Err(e) => {
            eprintln!("Error during capture: {}", e);
            process::exit(1);
        }
    }

    Ok(())
}

// Set console window size (Windows only)
#[cfg(windows)]
fn set_console_size(width: u16, height: u16) {
    use winapi::um::processenv::GetStdHandle;
    use winapi::um::winbase::STD_OUTPUT_HANDLE;
 // Fixed import
    use winapi::um::wincon::{SetConsoleWindowInfo, COORD, SMALL_RECT};

    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        
        // Set screen buffer size
        let size = COORD { 
            X: width as i16, // Fixed: cast u16 to i16
            Y: 1000 
        };
        winapi::um::wincon::SetConsoleScreenBufferSize(handle, size);
        
        // Set window size
        let rect = SMALL_RECT {
            Left: 0,
            Top: 0,
            Right: (width - 1) as i16, // Fixed: cast u16 to i16
            Bottom: (height - 1) as i16, // Fixed: cast u16 to i16
        };
        SetConsoleWindowInfo(handle, 1, &rect);
    }
}

// For non-Windows platforms, do nothing
#[cfg(not(windows))]
fn set_console_size(_width: u16, _height: u16) {
    // Not implemented for non-Windows platforms
}