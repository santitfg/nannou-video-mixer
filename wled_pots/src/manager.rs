use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::client::WledClient;
use crate::discovery::{DeviceDiscovery, DiscoveredDevice};
use crate::poller::{DevicePoller, DeviceReading};

struct TrackedDevice {
    name: String,
    mac: Option<String>,
    poller: DevicePoller,
}

/// Ties discovery and per-device pollers together behind one shared,
/// runtime-adjustable poll interval.
pub struct WledManager {
    discovery: Box<dyn DeviceDiscovery>,
    poll_interval: Arc<Mutex<Duration>>,
    devices: Mutex<HashMap<SocketAddr, TrackedDevice>>,
}

impl WledManager {
    pub fn new(discovery: Box<dyn DeviceDiscovery>, poll_interval: Duration) -> Self {
        WledManager {
            discovery,
            poll_interval: Arc::new(Mutex::new(poll_interval)),
            devices: Mutex::new(HashMap::new()),
        }
    }

    /// Runs discovery and starts a poller for any newly-found device.
    /// Devices already tracked (by address) are left untouched.
    pub fn discover_and_track(&self, timeout: Duration) {
        for found in self.discovery.discover(timeout) {
            self.track(found.addr, found.name, found.mac);
        }
    }

    /// Tracks a device without discovery, e.g. a manually-entered IP for
    /// when mDNS discovery is unreliable or unavailable.
    pub fn add_manual(&self, addr: SocketAddr) {
        self.track(addr, addr.to_string(), None);
    }

    /// Tracks a device found by a discovery run elsewhere (e.g. on a
    /// background thread, so the caller's thread never blocks on mDNS).
    pub fn add_discovered(&self, found: DiscoveredDevice) {
        self.track(found.addr, found.name, found.mac);
    }

    /// The current poll interval.
    pub fn poll_interval(&self) -> Duration {
        *self.poll_interval.lock().unwrap()
    }

    fn track(&self, addr: SocketAddr, name: String, mac: Option<String>) {
        let mut devices = self.devices.lock().unwrap();
        if devices.contains_key(&addr) {
            return;
        }
        let client = WledClient::new(addr);
        let poller = DevicePoller::spawn(client, self.poll_interval.clone());
        devices.insert(addr, TrackedDevice { name, mac, poller });
    }

    pub fn remove(&self, addr: SocketAddr) {
        self.devices.lock().unwrap().remove(&addr);
    }

    /// Updates the poll interval for every currently tracked device, and
    /// any tracked afterwards. Takes effect on each poller's next tick.
    pub fn set_poll_interval(&self, interval: Duration) {
        *self.poll_interval.lock().unwrap() = interval;
    }

    pub fn devices(&self) -> Vec<SocketAddr> {
        self.devices.lock().unwrap().keys().copied().collect()
    }

    pub fn device_name(&self, addr: SocketAddr) -> Option<String> {
        self.devices.lock().unwrap().get(&addr).map(|d| d.name.clone())
    }

    pub fn device_mac(&self, addr: SocketAddr) -> Option<String> {
        self.devices.lock().unwrap().get(&addr).and_then(|d| d.mac.clone())
    }

    pub fn reading(&self, addr: SocketAddr) -> Option<DeviceReading> {
        self.devices.lock().unwrap().get(&addr).map(|d| d.poller.reading())
    }
}
