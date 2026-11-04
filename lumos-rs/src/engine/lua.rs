//! Lua scripting. A script runs to completion inside the engine (no waiting): it reads the live state, sends
//! console lines and commands, and can schedule later runs with `lumos.after` / `lumos.every`.
//!
//! Sandbox: only the table, string, math, utf8 and coroutine libraries; no files, no OS, no `require`/`dofile`,
//! text chunks only for `load`; at most 250 ms and 32 MB per run.

use super::*;
use mlua::{HookTriggers, Lua, LuaOptions, LuaSerdeExt, MultiValue, StdLib, Value, VmState};
use std::cell::RefCell;

const MAX_RUN: Duration = Duration::from_millis(250);
const MAX_MEM: usize = 32 * 1024 * 1024;
const MAX_CODE: usize = 64 * 1024;
const MAX_TIMERS: usize = 64;
const MAX_LOG: usize = 60;

struct Timer {
    at: f64,
    every: Option<f64>,
    code: String,
    label: String,
}

#[derive(Default)]
pub struct LuaState {
    log: VecDeque<LuaLog>,
    timers: Vec<Timer>,
    store: BTreeMap<String, serde_json::Value>,
    depth: u8,
    pending_start: bool,
    last_periodic: HashMap<u32, f64>,
}

impl LuaState {
    pub fn new() -> Self {
        LuaState { pending_start: true, ..Default::default() }
    }
    pub fn reset(&mut self) {
        self.timers.clear();
        self.last_periodic.clear();
        self.pending_start = true;
    }
}

fn lua_err(e: impl std::fmt::Display) -> mlua::Error {
    mlua::Error::external(e.to_string())
}

fn osc_args(args: MultiValue) -> Result<Vec<rosc::OscType>, mlua::Error> {
    args.into_iter()
        .map(|v| match v {
            Value::Integer(i) => Ok(rosc::OscType::Int(i as i32)),
            Value::Number(n) => Ok(rosc::OscType::Float(n as f32)),
            Value::Boolean(b) => Ok(rosc::OscType::Bool(b)),
            Value::String(s) => Ok(rosc::OscType::String(s.to_string_lossy())),
            other => Err(lua_err(format!("OSC: argumento no válido ({})", other.type_name()))),
        })
        .collect()
}

fn fmt_value(v: &Value) -> String {
    match v {
        Value::Nil => "nil".into(),
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
        Value::String(s) => s.to_string_lossy(),
        other => format!("<{}>", other.type_name()),
    }
}

impl Engine {
    fn lua_note(&mut self, script: &str, ok: bool, text: String) {
        let t = self.now() as f32;
        if self.lua.log.len() >= MAX_LOG {
            self.lua.log.pop_front();
        }
        self.lua.log.push_back(LuaLog { t, script: script.to_string(), ok, text: text.chars().take(400).collect() });
        self.dirty = true;
    }

    fn lua_sandbox() -> Result<Lua, String> {
        let lua = Lua::new_with(StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8 | StdLib::COROUTINE, LuaOptions::new()).map_err(|e| e.to_string())?;
        lua.set_memory_limit(MAX_MEM).map_err(|e| e.to_string())?;
        lua.load(
            r#"
            dofile = nil; loadfile = nil; require = nil; collectgarbage = nil
            local l = load
            load = function(s, n, _, e) if type(s) ~= "string" then error("load: solo texto") end return l(s, n, "t", e) end
        "#,
        )
        .exec()
        .map_err(|e| e.to_string())?;
        Ok(lua)
    }

    /// Run `code` now. `label` names the script in the log. Everything it printed is logged; the error, if any, returned.
    pub(super) fn lua_run_code(&mut self, label: &str, code: &str) -> Result<(), String> {
        if code.len() > MAX_CODE {
            return Err("El script es demasiado largo (máximo 64 KB)".into());
        }
        if self.lua.depth >= 4 {
            return Err("Demasiados scripts anidados (un script llama a un macro que llama a otro script…)".into());
        }
        self.lua.depth += 1;
        let printed: RefCell<Vec<String>> = RefCell::new(Vec::new());
        let new_timers: RefCell<Vec<Timer>> = RefCell::new(Vec::new());
        let clear_timers = std::cell::Cell::new(false);
        let result = self.lua_exec(label, code, &printed, &new_timers, &clear_timers);
        self.lua.depth -= 1;
        if clear_timers.get() {
            self.lua.timers.clear();
        }
        for t in new_timers.into_inner() {
            if self.lua.timers.len() < MAX_TIMERS {
                self.lua.timers.push(t);
            }
        }
        for line in printed.into_inner() {
            self.lua_note(label, true, line);
        }
        match result {
            Ok(()) => Ok(()),
            Err(e) => {
                let msg = e.lines().next().unwrap_or("error").to_string();
                self.lua_note(label, false, msg.clone());
                Err(msg)
            }
        }
    }

    fn lua_exec(&mut self, label: &str, code: &str, printed: &RefCell<Vec<String>>, new_timers: &RefCell<Vec<Timer>>, clear_timers: &std::cell::Cell<bool>) -> Result<(), String> {
        let lua = Self::lua_sandbox()?;
        let started = Instant::now();
        lua.set_hook(HookTriggers::new().every_nth_instruction(10_000), move |_, _| {
            if started.elapsed() > MAX_RUN {
                Err(mlua::Error::runtime("El script tardó más de 250 ms y se detuvo (¿bucle infinito?)"))
            } else {
                Ok(VmState::Continue)
            }
        });
        let now = self.now();
        let eng_cell = RefCell::new(&mut *self);
        let eng = &eng_cell;
        let (printed, new_timers, clear_timers) = (printed, new_timers, clear_timers);
        let res: mlua::Result<()> = lua.scope(|scope| {
            let g = lua.globals();
            let t = lua.create_table()?;
            t.set("version", 1)?;
            g.set(
                "print",
                scope.create_function(move |_, args: MultiValue| {
                    let line = args.iter().map(fmt_value).collect::<Vec<_>>().join("\t");
                    printed.borrow_mut().push(line);
                    Ok(())
                })?,
            )?;
            t.set("log", g.get::<mlua::Function>("print")?)?;
            t.set("time", scope.create_function(move |_, ()| Ok(eng.borrow().now()))?)?;
            t.set(
                "date",
                scope.create_function(move |lua, ()| {
                    use chrono::{Datelike, Local, Timelike};
                    let d = Local::now();
                    let tb = lua.create_table()?;
                    for (k, v) in [("year", d.year() as i64), ("month", d.month() as i64), ("day", d.day() as i64), ("hour", d.hour() as i64), ("min", d.minute() as i64), ("sec", d.second() as i64), ("wday", d.weekday().number_from_monday() as i64)] {
                        tb.set(k, v)?;
                    }
                    Ok(tb)
                })?,
            )?;
            t.set(
                "cmd",
                scope.create_function(move |_, text: String| {
                    Ok(match eng.borrow_mut().run_cmdline_new(&text) {
                        Ok(()) => (true, None),
                        Err(e) => (false, Some(e)),
                    })
                })?,
            )?;
            t.set(
                "send",
                scope.create_function(move |lua, v: Value| {
                    let c: Command = match lua.from_value(v) {
                        Ok(c) => c,
                        Err(e) => return Ok((false, Some(format!("Comando no válido: {}", e.to_string().lines().next().unwrap_or("")).chars().take(200).collect::<String>()))),
                    };
                    Ok(match eng.borrow_mut().handle(c) {
                        Ok(()) => (true, None),
                        Err(e) => (false, Some(e)),
                    })
                })?,
            )?;
            t.set(
                "fixtures",
                scope.create_function(move |lua, ()| {
                    let e = eng.borrow();
                    let out = lua.create_table()?;
                    for (i, f) in e.show.fixtures.iter().enumerate() {
                        let r = lua.create_table()?;
                        r.set("id", f.id)?;
                        r.set("name", f.name.clone())?;
                        r.set("profile", f.profile.clone())?;
                        r.set("universe", f.universe)?;
                        r.set("address", f.address)?;
                        out.set(i + 1, r)?;
                    }
                    Ok(out)
                })?,
            )?;
            t.set(
                "attr",
                scope.create_function(move |_, (fixture, attr): (u32, String)| {
                    let e = eng.borrow();
                    let v = e.programmer.get(&fixture).and_then(|m| m.get(&attr)).copied().or_else(|| e.last_merged.get(&fixture).and_then(|m| m.get(&attr)).copied());
                    Ok(v)
                })?,
            )?;
            t.set("grandmaster", scope.create_function(move |_, ()| Ok(eng.borrow().grand_master))?)?;
            t.set("blackout", scope.create_function(move |_, ()| Ok(eng.borrow().blackout))?)?;
            t.set("exec_level", scope.create_function(move |_, id: u32| Ok(eng.borrow().exec_state_of(id).level))?)?;
            t.set(
                "cue",
                scope.create_function(move |_, id: u32| {
                    let e = eng.borrow();
                    Ok(e.runs.get(&id).filter(|r| r.state != PlayState::Idle).and_then(|r| r.current))
                })?,
            )?;
            t.set(
                "get",
                scope.create_function(move |lua, key: String| {
                    let e = eng.borrow();
                    match e.lua.store.get(&key) {
                        Some(v) => Ok(lua.to_value(v)?),
                        None => Ok(Value::Nil),
                    }
                })?,
            )?;
            t.set(
                "set",
                scope.create_function(move |lua, (key, v): (String, Value)| {
                    let mut e = eng.borrow_mut();
                    if matches!(v, Value::Nil) {
                        e.lua.store.remove(&key);
                    } else {
                        let j: serde_json::Value = lua.from_value(v).map_err(|e| lua_err(format!("Solo números, textos, booleanos y tablas simples: {e}")))?;
                        if e.lua.store.len() >= 256 && !e.lua.store.contains_key(&key) {
                            return Err(lua_err("Demasiadas claves guardadas (máximo 256)"));
                        }
                        e.lua.store.insert(key.chars().take(64).collect(), j);
                    }
                    Ok(())
                })?,
            )?;
            t.set(
                "osc",
                scope.create_function(move |_, (host, port, address, args): (String, u16, String, MultiValue)| {
                    let a = osc_args(args)?;
                    Ok(eng.borrow().io.osc_send_args(&host, port, &address, a))
                })?,
            )?;
            let add_timer = |every: bool| {
                move |_: &Lua, (secs, what): (f64, Value)| -> mlua::Result<()> {
                    if !(secs.is_finite() && secs >= if every { 0.1 } else { 0.0 }) {
                        return Err(lua_err(if every { "every: el periodo mínimo es 0,1 s" } else { "after: los segundos no pueden ser negativos" }));
                    }
                    let code = match what {
                        Value::String(s) => s.to_string_lossy(),
                        Value::Integer(id) => eng.borrow().show.lua.iter().find(|s| s.id as i64 == id).map(|s| s.code.clone()).ok_or_else(|| lua_err(format!("No existe el script {id}")))?,
                        _ => return Err(lua_err("after/every: pasa un texto de código Lua o el número de un script guardado")),
                    };
                    new_timers.borrow_mut().push(Timer { at: now + secs, every: every.then_some(secs), code, label: label.to_string() });
                    Ok(())
                }
            };
            t.set("after", scope.create_function(add_timer(false))?)?;
            t.set("every", scope.create_function(add_timer(true))?)?;
            t.set("cancel", scope.create_function(move |_, ()| {
                clear_timers.set(true);
                Ok(())
            })?)?;
            g.set("lumos", t)?;
            lua.load(code).set_name(label).exec()
        });
        res.map_err(|e| match e {
            mlua::Error::CallbackError { cause, .. } => cause.to_string(),
            mlua::Error::RuntimeError(m) => m,
            other => other.to_string(),
        })
    }

    pub(super) fn lua_command(&mut self, c: Command) -> Result<(), String> {
        match c {
            Command::LuaSave { mut script } => {
                script.name = script.name.trim().chars().take(60).collect();
                if script.name.is_empty() {
                    script.name = "Script".into();
                }
                if script.code.len() > MAX_CODE {
                    return Err("El script es demasiado largo (máximo 64 KB)".into());
                }
                if !script.every_s.is_finite() || script.every_s < 0.0 {
                    script.every_s = 0.0;
                }
                if script.every_s > 0.0 {
                    script.every_s = script.every_s.max(0.2);
                }
                if script.id == 0 {
                    script.id = next_id(self.show.lua.iter().map(|s| s.id));
                }
                self.lua.last_periodic.remove(&script.id);
                match self.show.lua.iter_mut().find(|s| s.id == script.id) {
                    Some(s) => *s = script,
                    None => self.show.lua.push(script),
                }
                self.touch();
            }
            Command::LuaRemove { id } => {
                self.show.lua.retain(|s| s.id != id);
                self.lua.last_periodic.remove(&id);
                self.touch();
            }
            Command::LuaRun { id } => {
                let s = self.show.lua.iter().find(|s| s.id == id).cloned().ok_or_else(|| format!("El script {id} no existe"))?;
                self.lua_run_code(&s.name, &s.code)?;
            }
            Command::LuaEval { code } => self.lua_run_code("(prueba)", &code)?,
            Command::LuaStop => {
                self.lua.timers.clear();
                self.dirty = true;
            }
            _ => {}
        }
        Ok(())
    }

    /// Timers, periodic scripts and "on start" scripts. Called every tick.
    pub(super) fn lua_tick(&mut self, now: f64) {
        if self.lua.pending_start {
            self.lua.pending_start = false;
            let starts: Vec<LuaScript> = self.show.lua.iter().filter(|s| s.on_start).cloned().collect();
            for s in starts {
                let _ = self.lua_run_code(&s.name, &s.code);
            }
        }
        let due: Vec<usize> = self.lua.timers.iter().enumerate().filter(|(_, t)| now >= t.at).map(|(i, _)| i).collect();
        let mut run: Vec<(String, String)> = Vec::new();
        for i in due.into_iter().rev() {
            let t = &mut self.lua.timers[i];
            run.push((t.label.clone(), t.code.clone()));
            match t.every {
                Some(p) => t.at = now + p,
                None => {
                    self.lua.timers.remove(i);
                }
            }
        }
        for (label, code) in run.into_iter().rev() {
            let _ = self.lua_run_code(&label, &code);
        }
        let periodic: Vec<LuaScript> = self.show.lua.iter().filter(|s| s.enabled && s.every_s > 0.0).cloned().collect();
        for s in periodic {
            let last = self.lua.last_periodic.get(&s.id).copied();
            if last.map_or(true, |l| now - l >= s.every_s as f64) {
                self.lua.last_periodic.insert(s.id, now);
                let _ = self.lua_run_code(&s.name, &s.code);
            }
        }
    }

    pub(super) fn lua_rt(&self) -> LuaRt {
        LuaRt { log: self.lua.log.iter().cloned().collect(), timers: self.lua.timers.len() as u32 }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::*;
    use super::*;

    fn run(e: &mut Engine, code: &str) -> Result<(), String> {
        e.handle(Command::LuaEval { code: code.into() })
    }
    fn last(e: &Engine) -> String {
        e.lua.log.back().map(|l| l.text.clone()).unwrap_or_default()
    }

    #[test]
    fn scripts_print_send_console_lines_and_commands_and_read_the_state() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 3);
        run(&mut e, "print('hola', 1, 2.5, true); assert(lumos.cmd('blackout on'))").unwrap();
        assert!(e.blackout);
        assert!(e.lua.log.iter().any(|l| l.ok && l.text == "hola\t1\t2.5\ttrue"));
        run(&mut e, "assert(lumos.send({type='grand_master', level=0.5}))").unwrap();
        assert!((e.grand_master - 0.5).abs() < 1e-6);
        // bad console line / bad command report instead of raising
        run(&mut e, "local ok, err = lumos.cmd('quién sabe qué'); assert(not ok and err); print(err)").unwrap();
        run(&mut e, "local ok, err = lumos.send({type='no_existe'}); assert(not ok and err)").unwrap();
        // reading
        run(&mut e, "local f = lumos.fixtures(); assert(#f == 3 and f[1].id == 1 and f[3].address == 3)").unwrap();
        run(&mut e, "lumos.cmd('fixture 1 at 40'); local v = lumos.attr(1, 'dimmer'); assert(math.abs(v - 0.4) < 0.01, tostring(v))").unwrap();
        run(&mut e, "assert(lumos.attr(9, 'dimmer') == nil); assert(lumos.grandmaster() == 0.5); assert(lumos.blackout() == true); assert(lumos.cue(1) == nil)").unwrap();
        run(&mut e, "local d = lumos.date(); assert(d.year >= 2024 and d.wday >= 1); assert(lumos.time() >= 0)").unwrap();
    }

    #[test]
    fn the_sandbox_has_no_files_no_os_no_bytecode_and_stops_runaway_scripts() {
        let mut e = engine();
        for code in ["assert(os == nil and io == nil and debug == nil and package == nil)", "assert(require == nil and dofile == nil and loadfile == nil)"] {
            run(&mut e, code).unwrap();
        }
        assert!(run(&mut e, "local f = string.dump(function() end); load(f)()").is_err(), "binary chunks are refused");
        let t = Instant::now();
        let err = run(&mut e, "while true do end").unwrap_err();
        assert!(err.contains("250 ms"), "{err}");
        assert!(t.elapsed() < Duration::from_secs(3));
        let err = run(&mut e, "local t = {} for i = 1, 1e9 do t[i] = string.rep('x', 1000) .. i end").unwrap_err();
        assert!(!err.is_empty(), "memory or time limit");
        // the engine keeps working afterwards
        run(&mut e, "lumos.cmd('blackout on')").unwrap();
        assert!(e.blackout);
        assert!(run(&mut e, "x = ").unwrap_err().to_lowercase().contains("syntax") || last(&e).len() > 0);
        assert!(run(&mut e, &"a".repeat(70_000)).is_err());
    }

    #[test]
    fn timers_after_every_cancel_and_the_store() {
        let mut e = engine();
        run(&mut e, "lumos.after(1, \"lumos.cmd('blackout on')\"); lumos.every(2, \"lumos.set('n', (lumos.get('n') or 0) + 1)\")").unwrap();
        assert_eq!(e.lua_rt().timers, 2);
        let t0 = e.now();
        e.lua_tick(t0 + 0.5);
        assert!(!e.blackout);
        e.lua_tick(t0 + 1.2);
        assert!(e.blackout, "after fired once");
        assert_eq!(e.lua_rt().timers, 1, "the one-shot is gone");
        e.lua_tick(t0 + 2.1);
        e.lua_tick(t0 + 4.3);
        run(&mut e, "assert(lumos.get('n') == 2)").unwrap();
        run(&mut e, "lumos.cancel()").unwrap();
        assert_eq!(e.lua_rt().timers, 0);
        // the store keeps numbers, text and small tables
        run(&mut e, "lumos.set('t', {a = 1, b = {2, 3}}); lumos.set('s', 'x')").unwrap();
        run(&mut e, "local t = lumos.get('t'); assert(t.a == 1 and t.b[2] == 3 and lumos.get('s') == 'x'); lumos.set('s', nil); assert(lumos.get('s') == nil)").unwrap();
        assert!(run(&mut e, "lumos.set('f', print)").is_err(), "functions cannot be stored");
        assert!(run(&mut e, "lumos.every(0.01, 'x=1')").is_err() && run(&mut e, "lumos.after(-1, 'x=1')").is_err());
        for _ in 0..70 {
            let _ = run(&mut e, "lumos.after(100, 'x=1')");
        }
        assert!(e.lua_rt().timers <= 64, "timers are capped");
    }

    #[test]
    fn stored_scripts_run_on_start_periodically_and_from_other_places_without_recursing_forever() {
        let mut e = engine();
        e.handle(Command::LuaSave { script: LuaScript { id: 0, name: "Inicio".into(), code: "lumos.set('start', 1)".into(), on_start: true, every_s: 0.0, enabled: false } }).unwrap();
        e.handle(Command::LuaSave { script: LuaScript { id: 0, name: "Latido".into(), code: "lumos.set('beat', (lumos.get('beat') or 0) + 1)".into(), on_start: false, every_s: 0.01, enabled: true } }).unwrap();
        assert_eq!(e.show.lua[1].every_s, 0.2, "the period has a floor");
        let t0 = e.now();
        e.lua_tick(t0);
        run(&mut e, "assert(lumos.get('start') == 1 and lumos.get('beat') == 1)").unwrap();
        e.lua_tick(t0 + 0.1);
        e.lua_tick(t0 + 0.3);
        run(&mut e, "assert(lumos.get('beat') == 2)").unwrap();
        e.lua_tick(t0 + 0.6);
        e.handle(Command::LuaRun { id: 1 }).unwrap();
        // from a console button and from a cue mark
        e.handle(Command::LuaSave { script: LuaScript { id: 0, name: "Apagón".into(), code: "lumos.cmd('blackout on')".into(), on_start: false, every_s: 0.0, enabled: false } }).unwrap();
        let id = e.show.lua[2].id;
        e.script_action(&ScriptAction::Lua { id }).unwrap();
        assert!(e.blackout);
        assert!(e.script_action(&ScriptAction::Lua { id: 99 }).is_err());
        assert!(e.handle(Command::LuaRun { id: 99 }).is_err());
        // a script that runs itself stops at a small depth instead of overflowing
        e.handle(Command::LuaSave { script: LuaScript { id: 0, name: "Bucle".into(), code: "local ok, err = lumos.send({type='lua_run', id=4}); if not ok then print(err) end".into(), on_start: false, every_s: 0.0, enabled: false } }).unwrap();
        e.handle(Command::LuaRun { id: 4 }).unwrap();
        assert!(e.lua.log.iter().any(|l| l.text.contains("anidados")), "{:?}", e.lua.log.iter().map(|l| &l.text).collect::<Vec<_>>());
        e.handle(Command::LuaRemove { id: 4 }).unwrap();
        assert_eq!(e.show.lua.len(), 3);
    }
}
