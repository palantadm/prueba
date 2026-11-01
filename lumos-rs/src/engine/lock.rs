//! Console lock: with a PIN set, "Lock" freezes the console for everyone (every screen, the assistant, REST imports)
//! until the PIN is entered. Emergency blackout stays available. The PIN is stored salted + hashed in `<data>/lock.json`.

use super::*;
use sha2::{Digest, Sha256};

const FILE: &str = "lock.json";
const MAX_FAILS: u32 = 5;
const COOL_DOWN: Duration = Duration::from_secs(30);

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Stored {
    salt: String,
    hash: String,
}

#[derive(Default)]
pub struct LockState {
    pub locked: bool,
    stored: Stored,
    fails: u32,
    until: Option<Instant>,
}

fn digest(salt: &str, pin: &str) -> String {
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(b"\0");
    h.update(pin.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

impl LockState {
    pub fn load(store: &Store) -> Self {
        let stored: Stored = std::fs::read_to_string(store.dir.join(FILE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        LockState { locked: false, stored, fails: 0, until: None }
    }
    pub fn pin_set(&self) -> bool {
        !self.stored.hash.is_empty()
    }
    fn check(&self, pin: &str) -> bool {
        self.pin_set() && digest(&self.stored.salt, pin) == self.stored.hash
    }
}

fn valid_pin(p: &str) -> bool {
    (4..=12).contains(&p.chars().count()) && p.chars().all(|c| c.is_ascii_digit())
}

impl Engine {
    /// Called for every command that comes from outside (UI, assistant, API) — not for the engine's own scheduled work.
    pub(super) fn lock_guard(&self, cmd: &Command) -> Result<(), String> {
        if !self.lock.locked {
            return Ok(());
        }
        match cmd {
            Command::Unlock { .. } | Command::Blackout { .. } | Command::SubscribeDmx { .. } => Ok(()),
            _ => Err("Mesa bloqueada: introduce el PIN para desbloquear".into()),
        }
    }

    pub(super) fn lock_command(&mut self, cmd: Command) -> Result<(), String> {
        match cmd {
            Command::Lock => {
                if !self.lock.pin_set() {
                    return Err("Primero define un PIN (Show → Bloquear mesa…)".into());
                }
                self.lock.locked = true;
                self.dirty = true;
                Ok(())
            }
            Command::Unlock { pin } => {
                if let Some(t) = self.lock.until {
                    if Instant::now() < t {
                        return Err(format!("Demasiados intentos: espera {} s", t.saturating_duration_since(Instant::now()).as_secs() + 1));
                    }
                    self.lock.until = None;
                }
                if self.lock.check(&pin) {
                    self.lock.locked = false;
                    self.lock.fails = 0;
                    self.dirty = true;
                    Ok(())
                } else {
                    self.lock.fails += 1;
                    if self.lock.fails >= MAX_FAILS {
                        self.lock.fails = 0;
                        self.lock.until = Some(Instant::now() + COOL_DOWN);
                        return Err(format!("PIN incorrecto. Demasiados intentos: espera {} s", COOL_DOWN.as_secs()));
                    }
                    Err(format!("PIN incorrecto ({} intentos restantes)", MAX_FAILS - self.lock.fails))
                }
            }
            Command::LockSet { pin, new_pin } => {
                if self.lock.locked {
                    return Err("La mesa está bloqueada".into());
                }
                if self.lock.pin_set() && !self.lock.check(&pin) {
                    return Err("El PIN actual no es correcto".into());
                }
                if new_pin.is_empty() {
                    self.lock.stored = Stored::default();
                    let _ = std::fs::remove_file(self.store.dir.join(FILE));
                } else {
                    if !valid_pin(&new_pin) {
                        return Err("El PIN debe tener entre 4 y 12 cifras".into());
                    }
                    let salt: String = format!("{:x}{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0), std::process::id());
                    self.lock.stored = Stored { hash: digest(&salt, &new_pin), salt };
                    let text = serde_json::to_string(&self.lock.stored).map_err(|e| e.to_string())?;
                    std::fs::write(self.store.dir.join(FILE), text).map_err(|e| e.to_string())?;
                }
                self.dirty = true;
                Ok(())
            }
            _ => Err("Comando de bloqueo desconocido".into()),
        }
    }

    pub(super) fn lock_rt(&self) -> LockRt {
        LockRt { locked: self.lock.locked, pin_set: self.lock.pin_set() }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::*;
    use super::*;

    #[test]
    fn pin_lock_blocks_outside_commands_until_unlocked() {
        let mut e = engine();
        assert!(e.handle(Command::Lock).unwrap_err().contains("define un PIN"));
        assert!(e.handle(Command::LockSet { pin: String::new(), new_pin: "12".into() }).unwrap_err().contains("4 y 12"));
        assert!(e.handle(Command::LockSet { pin: String::new(), new_pin: "12ab".into() }).is_err());
        e.handle(Command::LockSet { pin: String::new(), new_pin: "4321".into() }).unwrap();
        // stored hashed, not in the clear
        let file = std::fs::read_to_string(e.store.dir.join("lock.json")).unwrap();
        assert!(!file.contains("4321"));
        assert!(e.lock_guard(&Command::Go { cuelist: 1 }).is_ok(), "unlocked: everything passes");
        e.handle(Command::Lock).unwrap();
        assert!(e.build_rt().lock.locked);
        assert!(e.lock_guard(&Command::Go { cuelist: 1 }).unwrap_err().contains("bloqueada"));
        assert!(e.lock_guard(&Command::Blackout { on: true }).is_ok(), "emergency blackout stays available");
        assert!(e.lock_guard(&Command::Unlock { pin: "x".into() }).is_ok());
        assert!(e.handle(Command::LockSet { pin: "4321".into(), new_pin: "9999".into() }).unwrap_err().contains("bloqueada"));
        assert!(e.handle(Command::Unlock { pin: "0000".into() }).unwrap_err().contains("incorrecto"));
        assert!(e.lock.locked);
        e.handle(Command::Unlock { pin: "4321".into() }).unwrap();
        assert!(!e.lock.locked && e.lock_guard(&Command::Go { cuelist: 1 }).is_ok());
        // change and remove the PIN
        assert!(e.handle(Command::LockSet { pin: "1111".into(), new_pin: "5555".into() }).unwrap_err().contains("actual"));
        e.handle(Command::LockSet { pin: "4321".into(), new_pin: "5555".into() }).unwrap();
        assert!(LockState::load(&e.store).check("5555") && !LockState::load(&e.store).check("4321"), "persisted");
        e.handle(Command::LockSet { pin: "5555".into(), new_pin: String::new() }).unwrap();
        assert!(!e.lock.pin_set());
    }

    #[test]
    fn repeated_wrong_pins_trigger_a_cool_down() {
        let mut e = engine();
        e.handle(Command::LockSet { pin: String::new(), new_pin: "2468".into() }).unwrap();
        e.handle(Command::Lock).unwrap();
        for _ in 0..4 {
            assert!(e.handle(Command::Unlock { pin: "0000".into() }).unwrap_err().contains("restantes"));
        }
        assert!(e.handle(Command::Unlock { pin: "0000".into() }).unwrap_err().contains("Demasiados"));
        // even the right PIN waits during the cool-down
        assert!(e.handle(Command::Unlock { pin: "2468".into() }).unwrap_err().contains("espera"));
        assert!(e.lock.locked);
    }
}
