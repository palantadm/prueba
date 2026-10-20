//! Raw USB DMX interfaces (uDMX by anyma.ch / Objective Development).
//! Compiled only with `--features usb` so the default build needs no libusb.

use super::Sink;

#[cfg(not(feature = "usb"))]
pub fn new_usb(_driver: &str, _serial: Option<&str>) -> Box<dyn Sink> {
    use super::Stat;
    use crate::model::OutState;

    struct Unsupported(Stat);
    impl Sink for Unsupported {
        fn send(&mut self, _universe: u16, _data: &[u8; 512]) {}
        fn stat(&self) -> Stat {
            self.0.clone()
        }
    }
    Box::new(Unsupported(Stat::new(OutState::Error, "built without usb support")))
}

#[cfg(feature = "usb")]
pub fn new_usb(driver: &str, serial: Option<&str>) -> Box<dyn Sink> {
    udmx::UdmxSink::new(driver, serial)
}

#[cfg(feature = "usb")]
mod udmx {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use parking_lot::Mutex;

    use super::Sink;
    use crate::io::worker::{SharedStat, Slot, MIN_FRAME_GAP};
    use crate::io::Stat;
    use crate::model::OutState;

    const VID: u16 = 0x16C0;
    const PID: u16 = 0x05DC;
    /// uDMX vendor request: set a range of channels (wValue = count, wIndex = first channel).
    const CMD_SET_CHANNEL_RANGE: u8 = 2;

    pub struct UdmxSink {
        slot: Arc<Slot>,
        stat: SharedStat,
        frames: Arc<AtomicU32>,
    }

    impl UdmxSink {
        pub fn new(driver: &str, serial: Option<&str>) -> Box<dyn Sink> {
            let slot = Slot::new();
            let stat: SharedStat = Arc::new(Mutex::new(Stat::new(OutState::Off, "connecting…")));
            let frames = Arc::new(AtomicU32::new(0));
            if !driver.eq_ignore_ascii_case("udmx") {
                *stat.lock() = Stat::new(OutState::Error, format!("unknown USB driver '{driver}'"));
                return Box::new(UdmxSink { slot, stat, frames });
            }
            let serial = serial.map(str::to_string).filter(|s| !s.is_empty());
            let (s2, st2, f2) = (slot.clone(), stat.clone(), frames.clone());
            let _ = std::thread::Builder::new()
                .name("dmx-udmx".into())
                .spawn(move || run(serial, s2, st2, f2));
            Box::new(UdmxSink { slot, stat, frames })
        }
    }

    impl Drop for UdmxSink {
        fn drop(&mut self) {
            self.slot.close();
        }
    }

    impl Sink for UdmxSink {
        fn send(&mut self, _u: u16, data: &[u8; 512]) {
            self.slot.put(data);
        }
        fn stat(&self) -> Stat {
            let mut s = self.stat.lock().clone();
            s.frames = self.frames.load(Ordering::Relaxed);
            s
        }
    }

    fn open(serial: Option<&str>) -> Result<rusb::DeviceHandle<rusb::GlobalContext>, String> {
        for dev in rusb::devices().map_err(|e| e.to_string())?.iter() {
            let Ok(desc) = dev.device_descriptor() else { continue };
            if desc.vendor_id() != VID || desc.product_id() != PID {
                continue;
            }
            let h = dev.open().map_err(|e| e.to_string())?;
            if let Some(want) = serial {
                match h.read_serial_number_string_ascii(&desc) {
                    Ok(s) if s == want => {}
                    _ => continue,
                }
            }
            return Ok(h);
        }
        Err("uDMX device not found".into())
    }

    fn run(serial: Option<String>, slot: Arc<Slot>, stat: SharedStat, frames: Arc<AtomicU32>) {
        let mut backoff = Duration::from_millis(500);
        while !slot.stopped() {
            let handle = match open(serial.as_deref()) {
                Ok(h) => {
                    backoff = Duration::from_millis(500);
                    *stat.lock() = Stat::new(OutState::Ok, "uDMX connected");
                    h
                }
                Err(e) => {
                    *stat.lock() = Stat::new(OutState::Error, e);
                    if !slot.sleep(backoff) {
                        return;
                    }
                    backoff = (backoff * 2).min(Duration::from_secs(5));
                    continue;
                }
            };
            loop {
                if slot.stopped() {
                    return;
                }
                slot.wait(MIN_FRAME_GAP);
                let Some(frame) = slot.latest() else { continue };
                let rt = rusb::request_type(
                    rusb::Direction::Out,
                    rusb::RequestType::Vendor,
                    rusb::Recipient::Device,
                );
                match handle.write_control(rt, CMD_SET_CHANNEL_RANGE, 512, 0, &frame, Duration::from_millis(100)) {
                    Ok(_) => {
                        frames.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        *stat.lock() = Stat::new(OutState::Error, e.to_string());
                        break;
                    }
                }
            }
        }
    }
}
