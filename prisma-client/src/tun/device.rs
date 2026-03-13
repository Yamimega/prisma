//! Platform-agnostic TUN device trait and implementations.
//!
//! Platform support:
//! - **Windows**: Uses the Wintun driver via the `wintun` crate. Wintun is a
//!   lightweight, high-performance TUN driver that doesn't require a separate
//!   installation step on Windows 10+.
//! - **Linux**: Uses `/dev/net/tun` via raw ioctl. Requires `CAP_NET_ADMIN`.
//! - **macOS**: Uses the `utun` kernel interface via raw sockets. Requires root.

use anyhow::Result;

/// A TUN device that can read and write IP packets.
pub trait TunDevice: Send + 'static {
    /// Read a single IP packet from the TUN device.
    /// Returns the number of bytes read.
    fn recv(&self, buf: &mut [u8]) -> Result<usize>;

    /// Write a single IP packet to the TUN device.
    fn send(&self, buf: &[u8]) -> Result<usize>;

    /// Get the name of the TUN device.
    fn name(&self) -> &str;

    /// Get the MTU of the TUN device.
    fn mtu(&self) -> u16;
}

/// Create a TUN device with the given configuration.
pub fn create_tun_device(
    device_name: &str,
    mtu: u16,
    _include_routes: &[String],
    _exclude_routes: &[String],
) -> Result<Box<dyn TunDevice>> {
    #[cfg(target_os = "windows")]
    {
        create_windows_tun(device_name, mtu)
    }

    #[cfg(target_os = "linux")]
    {
        create_linux_tun(device_name, mtu)
    }

    #[cfg(target_os = "macos")]
    {
        create_macos_tun(device_name, mtu)
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        let _ = (device_name, mtu);
        Err(anyhow::anyhow!(
            "TUN mode is not supported on this platform. \
             Supported: Windows, Linux, macOS."
        ))
    }
}

// =============================================================================
// Windows implementation (Wintun driver)
// =============================================================================

#[cfg(target_os = "windows")]
fn create_windows_tun(device_name: &str, mtu: u16) -> Result<Box<dyn TunDevice>> {
    use std::sync::Arc;

    let wintun = unsafe { wintun::load() }.map_err(|e| {
        anyhow::anyhow!(
            "Failed to load Wintun driver: {}. Download wintun.dll from https://www.wintun.net/",
            e
        )
    })?;

    let adapter = match wintun::Adapter::open(&wintun, device_name) {
        Ok(a) => a,
        Err(_) => wintun::Adapter::create(&wintun, device_name, "PrismaVeil", None)
            .map_err(|e| anyhow::anyhow!("Failed to create Wintun adapter: {}", e))?,
    };

    // Ring buffer capacity (must be power of 2, between 128KB and 64MB).
    // 4MB is a reasonable default for high-throughput proxying.
    let capacity = 0x400000; // 4MB
    let session = adapter
        .start_session(capacity)
        .map_err(|e| anyhow::anyhow!("Failed to start Wintun session: {}", e))?;

    tracing::info!(device = %device_name, mtu = mtu, "Wintun TUN device created");

    Ok(Box::new(WindowsTunDevice {
        session: Arc::new(session),
        name: device_name.to_string(),
        mtu,
    }))
}

#[cfg(target_os = "windows")]
struct WindowsTunDevice {
    session: std::sync::Arc<wintun::Session>,
    name: String,
    mtu: u16,
}

#[cfg(target_os = "windows")]
impl TunDevice for WindowsTunDevice {
    fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        // Block until a packet is available (up to 1 second timeout).
        match self.session.receive_blocking() {
            Ok(packet) => {
                let bytes = packet.bytes();
                let n = bytes.len().min(buf.len());
                buf[..n].copy_from_slice(&bytes[..n]);
                Ok(n)
            }
            Err(e) => Err(anyhow::anyhow!("Wintun recv error: {}", e)),
        }
    }

    fn send(&self, buf: &[u8]) -> Result<usize> {
        let mut packet = self
            .session
            .allocate_send_packet(buf.len() as u16)
            .map_err(|e| anyhow::anyhow!("Wintun allocate error: {}", e))?;
        packet.bytes_mut().copy_from_slice(buf);
        self.session.send_packet(packet);
        Ok(buf.len())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn mtu(&self) -> u16 {
        self.mtu
    }
}

// =============================================================================
// Linux implementation (raw ioctl on /dev/net/tun)
// =============================================================================

#[cfg(target_os = "linux")]
fn create_linux_tun(device_name: &str, mtu: u16) -> Result<Box<dyn TunDevice>> {
    use std::os::fd::AsRawFd;

    // Open /dev/net/tun
    let fd = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/net/tun")
        .map_err(|e| {
            anyhow::anyhow!("Failed to open /dev/net/tun: {}. Ensure CAP_NET_ADMIN.", e)
        })?;

    // Set up TUN device via ioctl
    let mut ifr = [0u8; 40]; // struct ifreq
    let name_bytes = device_name.as_bytes();
    let copy_len = name_bytes.len().min(15);
    ifr[..copy_len].copy_from_slice(&name_bytes[..copy_len]);

    // IFF_TUN = 0x0001, IFF_NO_PI = 0x1000
    ifr[16] = 0x01;
    ifr[17] = 0x10;

    // TUNSETIFF = 0x400454CA
    unsafe {
        let ret = libc::ioctl(fd.as_raw_fd(), 0x400454CA, ifr.as_ptr());
        if ret < 0 {
            return Err(anyhow::anyhow!(
                "TUNSETIFF ioctl failed: {}",
                std::io::Error::last_os_error()
            ));
        }
    }

    // Get actual device name
    let name_end = ifr.iter().position(|&b| b == 0).unwrap_or(16).min(16);
    let actual_name = String::from_utf8_lossy(&ifr[..name_end]).to_string();

    tracing::info!(device = %actual_name, mtu = mtu, "Linux TUN device created");

    Ok(Box::new(LinuxTunDevice {
        fd,
        name: actual_name,
        mtu,
    }))
}

#[cfg(target_os = "linux")]
struct LinuxTunDevice {
    fd: std::fs::File,
    name: String,
    mtu: u16,
}

#[cfg(target_os = "linux")]
impl TunDevice for LinuxTunDevice {
    fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        use std::io::Read;
        let mut fd = &self.fd;
        let n = fd.read(buf)?;
        Ok(n)
    }

    fn send(&self, buf: &[u8]) -> Result<usize> {
        use std::io::Write;
        let mut fd = &self.fd;
        let n = fd.write(buf)?;
        Ok(n)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn mtu(&self) -> u16 {
        self.mtu
    }
}

// =============================================================================
// macOS implementation (utun via sys/kern_control.h)
// =============================================================================

#[cfg(target_os = "macos")]
fn create_macos_tun(device_name: &str, mtu: u16) -> Result<Box<dyn TunDevice>> {
    use std::os::fd::FromRawFd;

    // macOS uses the utun kernel control interface.
    // Parse utun index from device name (e.g., "utun5" → 5).
    let utun_index: u32 = if device_name.starts_with("utun") {
        device_name[4..].parse().unwrap_or(0) // utun0 if no number specified
    } else {
        0
    };

    // Create a PF_SYSTEM socket
    let fd = unsafe {
        libc::socket(
            libc::PF_SYSTEM,
            libc::SOCK_DGRAM,
            2, // SYSPROTO_CONTROL
        )
    };
    if fd < 0 {
        return Err(anyhow::anyhow!(
            "Failed to create PF_SYSTEM socket: {}",
            std::io::Error::last_os_error()
        ));
    }

    // Look up the control ID for com.apple.net.utun_control
    #[repr(C)]
    struct CtlInfo {
        ctl_id: u32,
        ctl_name: [u8; 96],
    }

    let mut info = CtlInfo {
        ctl_id: 0,
        ctl_name: [0u8; 96],
    };
    let control_name = b"com.apple.net.utun_control";
    info.ctl_name[..control_name.len()].copy_from_slice(control_name);

    // CTLIOCGINFO = 0xC0644E03
    let ret = unsafe { libc::ioctl(fd, 0xC0644E03u64 as libc::c_ulong, &mut info) };
    if ret < 0 {
        unsafe { libc::close(fd) };
        return Err(anyhow::anyhow!(
            "CTLIOCGINFO ioctl failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    // Connect to the utun control with the desired unit number.
    // sc_unit = utun_index + 1 (utun0 = unit 1, utun5 = unit 6)
    #[repr(C)]
    struct SockaddrCtl {
        sc_len: u8,
        sc_family: u8,
        ss_sysaddr: u16,
        sc_id: u32,
        sc_unit: u32,
        sc_reserved: [u32; 5],
    }

    let addr = SockaddrCtl {
        sc_len: std::mem::size_of::<SockaddrCtl>() as u8,
        sc_family: libc::AF_SYSTEM as u8,
        ss_sysaddr: 2, // AF_SYS_CONTROL
        sc_id: info.ctl_id,
        sc_unit: utun_index + 1,
        sc_reserved: [0; 5],
    };

    let ret = unsafe {
        libc::connect(
            fd,
            &addr as *const SockaddrCtl as *const libc::sockaddr,
            std::mem::size_of::<SockaddrCtl>() as u32,
        )
    };
    if ret < 0 {
        unsafe { libc::close(fd) };
        return Err(anyhow::anyhow!(
            "Failed to connect utun socket: {}. Try running with sudo.",
            std::io::Error::last_os_error()
        ));
    }

    let file = unsafe { std::fs::File::from_raw_fd(fd) };
    let actual_name = format!("utun{}", utun_index);

    tracing::info!(device = %actual_name, mtu = mtu, "macOS utun device created");

    Ok(Box::new(MacOsTunDevice {
        fd: file,
        name: actual_name,
        mtu,
    }))
}

#[cfg(target_os = "macos")]
struct MacOsTunDevice {
    fd: std::fs::File,
    name: String,
    mtu: u16,
}

#[cfg(target_os = "macos")]
impl TunDevice for MacOsTunDevice {
    fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        use std::io::Read;
        // macOS utun prepends a 4-byte protocol header (AF_INET = 2 for IPv4).
        // We need to read past it.
        let mut tmp = vec![0u8; buf.len() + 4];
        let mut fd = &self.fd;
        let n = fd.read(&mut tmp)?;
        if n <= 4 {
            return Ok(0);
        }
        let payload_len = n - 4;
        buf[..payload_len].copy_from_slice(&tmp[4..n]);
        Ok(payload_len)
    }

    fn send(&self, buf: &[u8]) -> Result<usize> {
        use std::io::Write;
        // Prepend the 4-byte protocol header (AF_INET = 2 for IPv4).
        let mut packet = Vec::with_capacity(buf.len() + 4);
        // Protocol family: AF_INET (2) in network byte order on macOS
        packet.extend_from_slice(&[0, 0, 0, 2]);
        packet.extend_from_slice(buf);
        let mut fd = &self.fd;
        fd.write_all(&packet)?;
        Ok(buf.len())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn mtu(&self) -> u16 {
        self.mtu
    }
}
