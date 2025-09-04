fn main() {
    // Đường dẫn đến thư mục Lib\x64 của npcap
    println!("cargo:rustc-link-search=native=C:\\Users\\badboyhalo1801\\Desktop\\npcap\\Lib\\x64");
    println!("cargo:rustc-link-lib=wpcap");
    println!("cargo:rustc-link-lib=packet"); // đôi khi cần thêm Packet.dll
}
