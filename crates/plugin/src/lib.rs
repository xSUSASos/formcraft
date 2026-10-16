//! WebAssembly plugin host (ADR 0003): loads Add-In components (`plugin.toml` + `.wasm`),
//! registers their commands and custom features, and runs them in a sandbox — WASI without
//! files, network or environment, a memory limit, and a time limit per call (epoch
//! interruption). A plugin that traps or times out is unloaded: its commands disappear, its
//! features fall back to their frozen bodies, the application keeps running.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod host;
pub mod install;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use formcraft_engine::regen::CustomFeatures;
use formcraft_engine::{CommandSpec, EngineError, Placement, Session, Source};
use formcraft_kernel::{Kernel, Shape};
pub use formcraft_plugin_api::{API_VERSION, Manifest, ManifestError};
use serde_json::{Value, json};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder};

use crate::host::{Bindings, HostState};

/// Limits of one plugin (defaults from ADR 0003).
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub memory: usize,
    /// Wall time of one call (init, command, regenerate).
    pub call: Duration,
    pub init: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { memory: 256 << 20, call: Duration::from_secs(2), init: Duration::from_secs(10) }
    }
}

/// Epoch tick: deadlines are counted in ticks.
const TICK: Duration = Duration::from_millis(10);

fn ticks(d: Duration) -> u64 {
    (d.as_millis() / TICK.as_millis()).max(1) as u64
}

/// What a plugin registered.
#[derive(Debug, Clone, Default)]
pub struct Registered {
    pub commands: Vec<String>,
    /// Feature kinds; the host command for each is `<plugin>.<kind>`.
    pub features: Vec<String>,
}

pub struct Loaded {
    pub manifest: Manifest,
    pub dir: PathBuf,
    registered: Mutex<Registered>,
    store: Mutex<Store<HostState>>,
    bindings: Bindings,
    dead: AtomicBool,
    /// Why the plugin was unloaded.
    pub failure: Mutex<Option<String>>,
    limits: Limits,
}

impl Loaded {
    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    pub fn registered(&self) -> Registered {
        self.registered.lock().map(|r| r.clone()).unwrap_or_default()
    }

    pub fn is_dead(&self) -> bool {
        self.dead.load(Ordering::Relaxed)
    }

    pub fn log(&self) -> Vec<String> {
        match self.store.try_lock() {
            Ok(s) => s.data().log.clone(),
            Err(_) => vec!["(the plugin is busy)".into()],
        }
    }

    fn kill(&self, why: String) {
        log::error!("plugin `{}` unloaded: {why}", self.manifest.id);
        self.dead.store(true, Ordering::Relaxed);
        if let Ok(mut f) = self.failure.lock() {
            *f = Some(why);
        }
    }

    /// Run `f` on the plugin's store with a deadline; a trap or timeout unloads the plugin.
    fn call<R>(
        &self,
        timeout: Duration,
        f: impl FnOnce(&Bindings, &mut Store<HostState>) -> wasmtime::Result<R>,
    ) -> Result<R, String> {
        if self.is_dead() {
            return Err(format!("plugin `{}` was unloaded", self.manifest.id));
        }
        // A plugin calling back into itself (a command that rebuilds its own feature) would
        // deadlock; refuse instead.
        let mut store =
            self.store.try_lock().map_err(|_| format!("plugin `{}` is busy (re-entrant call)", self.manifest.id))?;
        store.set_epoch_deadline(ticks(timeout));
        match f(&self.bindings, &mut store) {
            Ok(r) => Ok(r),
            Err(e) => {
                let why = if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::Interrupt) {
                    format!("timed out after {} ms", timeout.as_millis())
                } else {
                    format!("crashed: {e}")
                };
                drop(store);
                self.kill(why.clone());
                Err(format!("plugin `{}` {why}", self.manifest.id))
            }
        }
    }
}

/// All plugins of a session; also the session's builder of custom features.
pub struct Plugins {
    engine: Engine,
    linker: Linker<HostState>,
    kernel: Arc<dyn Kernel>,
    plugins: Mutex<Vec<Arc<Loaded>>>,
    stop: Arc<AtomicBool>,
    pub limits: Limits,
}

impl Drop for Plugins {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PluginError {
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("{0}")]
    Load(String),
}

impl Plugins {
    pub fn new(kernel: Arc<dyn Kernel>) -> Result<Arc<Plugins>, PluginError> {
        let mut config = Config::new();
        config.wasm_component_model(true).epoch_interruption(true);
        let engine = Engine::new(&config).map_err(|e| PluginError::Load(e.to_string()))?;
        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(|e| PluginError::Load(e.to_string()))?;
        host::add_to_linker(&mut linker).map_err(|e| PluginError::Load(e.to_string()))?;
        let stop = Arc::new(AtomicBool::new(false));
        // the epoch ticker that enforces call deadlines
        let (e, s) = (engine.clone(), stop.clone());
        std::thread::Builder::new()
            .name("plugin-epoch".into())
            .spawn(move || {
                while !s.load(Ordering::Relaxed) {
                    std::thread::sleep(TICK);
                    e.increment_epoch();
                }
            })
            .map_err(|e| PluginError::Load(e.to_string()))?;
        Ok(Arc::new(Plugins {
            engine,
            linker,
            kernel,
            plugins: Mutex::new(Vec::new()),
            stop,
            limits: Limits::default(),
        }))
    }

    pub fn list(&self) -> Vec<Arc<Loaded>> {
        self.plugins.lock().map(|p| p.clone()).unwrap_or_default()
    }

    fn get(&self, id: &str) -> Option<Arc<Loaded>> {
        self.list().into_iter().find(|p| p.manifest.id == id)
    }

    /// Load the plugin in `dir` (with `plugin.toml`) into `s`: instantiate, call `init`,
    /// register its commands and features. Errors leave the session unchanged.
    pub fn load(self: &Arc<Self>, dir: &Path, s: &mut Session) -> Result<Arc<Loaded>, PluginError> {
        let text = std::fs::read_to_string(dir.join("plugin.toml"))
            .map_err(|e| PluginError::Load(format!("{}: {e}", dir.join("plugin.toml").display())))?;
        let manifest = Manifest::parse(&text)?;
        if self.get(&manifest.id).is_some_and(|p| !p.is_dead()) {
            return Err(PluginError::Load(format!("plugin `{}` is already loaded", manifest.id)));
        }
        let bytes = std::fs::read(dir.join(&manifest.wasm))
            .map_err(|e| PluginError::Load(format!("{}: {e}", manifest.wasm)))?;
        let component = Component::new(&self.engine, &bytes)
            .map_err(|e| PluginError::Load(format!("{}: not a valid component: {e}", manifest.wasm)))?;
        let limits: StoreLimits = StoreLimitsBuilder::new().memory_size(self.limits.memory).instances(4).build();
        let files = manifest
            .permissions
            .files
            .iter()
            .map(|f| {
                let p = PathBuf::from(f);
                if p.is_absolute() { p } else { dir.join(p) }
            })
            .collect();
        let state = HostState {
            table: ResourceTable::new(),
            wasi: wasmtime_wasi::WasiCtx::builder().build(),
            limits,
            session: None,
            kernel: self.kernel.clone(),
            files,
            log: Vec::new(),
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_epoch_deadline(ticks(self.limits.init));
        let bindings = Bindings::instantiate(&mut store, &component, &self.linker)
            .map_err(|e| PluginError::Load(format!("`{}` failed to start: {e}", manifest.id)))?;
        let loaded = Arc::new(Loaded {
            manifest: manifest.clone(),
            dir: dir.to_path_buf(),
            registered: Mutex::new(Registered::default()),
            store: Mutex::new(store),
            bindings,
            dead: AtomicBool::new(false),
            failure: Mutex::new(None),
            limits: self.limits,
        });
        let (commands, features) = loaded
            .call(self.limits.init, |b, st| b.call_init(st))
            .map_err(PluginError::Load)?
            .map_err(|e| PluginError::Load(format!("`{}` init: {e}", manifest.id)))?;
        let mut reg = Registered::default();
        let id = manifest.id.clone();
        let mut specs = Vec::new();
        for c in commands {
            specs.push((
                self.command_spec(
                    &id,
                    &c.id,
                    &c.label,
                    &c.description,
                    &c.params_schema,
                    c.placement,
                    c.shortcut,
                    c.ai,
                )?,
                None,
            ));
            reg.commands.push(c.id);
        }
        for f in features {
            let cmd = format!("{id}.{}", f.kind);
            let spec =
                self.command_spec(&id, &cmd, &f.label, &f.description, &f.params_schema, f.placement, None, f.ai)?;
            specs.push((spec.undoable(), Some((f.kind.clone(), f.label.clone()))));
            reg.features.push(f.kind);
        }
        // register everything or nothing
        let source = Source::Plugin(id.clone());
        for (spec, feature) in specs {
            let cmd_id = spec.id.clone();
            let handler: formcraft_engine::Handler = match feature {
                // feature commands are run by the host: the plugin is idle when the feature is
                // rebuilt, so `regenerate` can never be re-entrant
                Some((kind, label)) => {
                    let plugin = id.clone();
                    Arc::new(move |s: &mut Session, p: &Value| {
                        let mut params = p.clone();
                        let op = formcraft_engine::params::body_op(s.document(), &params)?;
                        if let Some(o) = params.as_object_mut() {
                            o.remove("operation");
                            o.remove("target");
                        }
                        s.add_custom_feature(&plugin, &kind, &label, params, op)
                    })
                }
                None => {
                    let me = Arc::downgrade(&loaded);
                    let name = cmd_id.clone();
                    Arc::new(move |s: &mut Session, p: &Value| {
                        let l = me.upgrade().ok_or_else(|| EngineError::Failed("the plugin was removed".into()))?;
                        run_command(&l, s, &name, p)
                    })
                }
            };
            if let Err(e) = s.registry_mut().register(spec.source(source.clone()), handler) {
                s.registry_mut().unregister_source(&source);
                return Err(PluginError::Load(format!("`{id}`: {e}")));
            }
        }
        if let Ok(mut r) = loaded.registered.lock() {
            *r = reg;
        }
        if let Ok(mut list) = self.plugins.lock() {
            list.retain(|p| p.manifest.id != id);
            list.push(loaded.clone());
        }
        s.set_custom_features(Some(self.clone() as Arc<dyn CustomFeatures>));
        Ok(loaded)
    }

    #[allow(clippy::too_many_arguments)]
    fn command_spec(
        &self,
        plugin: &str,
        id: &str,
        label: &str,
        description: &str,
        schema: &str,
        placement: Option<host::types::Placement>,
        shortcut: Option<String>,
        ai: bool,
    ) -> Result<CommandSpec, PluginError> {
        if !id.starts_with(&format!("{plugin}.")) {
            return Err(PluginError::Load(format!("command `{id}` must start with `{plugin}.`")));
        }
        let params: Value = serde_json::from_str(schema)
            .map_err(|e| PluginError::Load(format!("`{id}`: parameter schema is not JSON: {e}")))?;
        if !params.is_object() {
            return Err(PluginError::Load(format!("`{id}`: parameter schema must be an object")));
        }
        let mut spec = CommandSpec::new(id, label, description).params(params).icon("plugin");
        spec.placement = placement.map(|p| Placement { tab: p.tab, group: p.group, pinned: p.pinned });
        spec.shortcut = shortcut;
        spec.ai = ai;
        Ok(spec)
    }

    /// Unload a plugin: its commands go away, its features use their frozen bodies.
    pub fn unload(&self, id: &str, s: &mut Session) -> bool {
        let found = self.get(id);
        if let Some(p) = &found {
            p.kill("unloaded by the user".into());
        }
        if let Ok(mut list) = self.plugins.lock() {
            list.retain(|p| p.manifest.id != id);
        }
        s.registry_mut().unregister_source(&Source::Plugin(id.to_string()));
        s.set_custom_features(s.custom_features().cloned());
        found.is_some()
    }

    /// Remove the commands of plugins that crashed or timed out (call after commands/regen).
    pub fn reap(&self, s: &mut Session) -> Vec<(String, String)> {
        let dead: Vec<Arc<Loaded>> = self.list().into_iter().filter(|p| p.is_dead()).collect();
        let mut out = Vec::new();
        for p in dead {
            let why = p.failure.lock().ok().and_then(|f| f.clone()).unwrap_or_default();
            if !s.registry_mut().unregister_source(&Source::Plugin(p.manifest.id.clone())).is_empty() {
                out.push((p.manifest.id.clone(), why));
            }
        }
        out
    }

    /// Load every plugin folder under `root` (one level). Returns (folder, result) per plugin.
    pub fn load_all(self: &Arc<Self>, root: &Path, s: &mut Session) -> Vec<(PathBuf, Result<String, String>)> {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join("plugin.toml").is_file())
            .collect();
        dirs.sort();
        dirs.into_iter()
            .map(|d| {
                let r = self.load(&d, s).map(|l| l.manifest.id.clone()).map_err(|e| e.to_string());
                (d, r)
            })
            .collect()
    }

    pub fn describe(&self) -> Value {
        Value::Array(
            self.list()
                .iter()
                .map(|p| {
                    json!({
                        "id": p.manifest.id, "name": p.manifest.name, "version": p.manifest.version,
                        "dir": p.dir.display().to_string(), "loaded": !p.is_dead(),
                        "failure": p.failure.lock().ok().and_then(|f| f.clone()),
                        "commands": p.registered().commands, "features": p.registered().features,
                    })
                })
                .collect(),
        )
    }
}

/// A plugin command: the session is lent to the store for the call (host.execute works on
/// it) and always returned, also when the plugin traps.
fn run_command(l: &Loaded, s: &mut Session, id: &str, p: &Value) -> formcraft_engine::Result<Value> {
    let lent = std::mem::take(s);
    let params = p.to_string();
    let mut back = None;
    let r = l.call(l.limits.call, |b, st| {
        st.data_mut().session = Some(lent);
        let r = b.call_run_command(&mut *st, id, &params);
        back = st.data_mut().session.take();
        r
    });
    match back {
        Some(sess) => *s = sess,
        None => log::error!("plugin `{}`: session lost; a fresh one was created", l.manifest.id),
    }
    match r {
        Err(e) => Err(EngineError::Failed(e)),
        Ok(Err(e)) => Err(EngineError::Failed(e)),
        Ok(Ok(text)) => {
            let v: Value = serde_json::from_str(&text).unwrap_or(Value::String(text));
            Ok(if v.is_object() { v } else { json!({"result": v}) })
        }
    }
}

impl CustomFeatures for Plugins {
    fn regenerate(&self, plugin: &str, kind: &str, params: &Value) -> Result<Shape, String> {
        let p = self.get(plugin).ok_or_else(|| format!("plugin `{plugin}` is not loaded"))?;
        let text = params.to_string();
        p.call(p.limits.call, |b, st| {
            let res = b.call_regenerate(&mut *st, kind, &text)?;
            Ok(match res {
                Ok(handle) => host::take_solid(st.data_mut(), handle).map_err(|e| e.to_string()),
                Err(e) => Err(e),
            })
        })?
    }
}

#[cfg(test)]
mod tests;
