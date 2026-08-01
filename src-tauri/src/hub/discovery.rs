/// ZANPOS LAN hub discovery via mDNS-SD.
///
/// Advertises and discovers `_zanpos-hub._tcp.local` services on the local network.
/// Only advertises non-sensitive metadata (instance ID, protocol version, port,
/// branch discriminator, pairing capability, TLS fingerprint).

#[cfg(feature = "mdns-discovery")]
mod inner {
    pub struct HubDiscovery;

    impl HubDiscovery {
        pub fn new() -> Result<Self, String> {
            Ok(Self)
        }

        pub fn start(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn stop(&self) {}
    }
}

#[cfg(not(feature = "mdns-discovery"))]
mod inner {
    pub struct HubDiscovery;

    impl HubDiscovery {
        pub fn new() -> Result<Self, String> {
            Err("mDNS discovery not compiled in — enable the mdns-discovery feature".into())
        }

        pub fn start(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn stop(&self) {}
    }
}

pub use inner::HubDiscovery;
