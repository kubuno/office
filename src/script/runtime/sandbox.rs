use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

use serde_json::{json, Value};

use crate::script::models::run::ConsoleEntry;

pub struct SandboxConfig {
    pub timeout_secs:    u64,
    pub memory_limit_mb: u32,
}

pub struct ExecutionResult {
    pub status:        String,
    pub return_value:  Option<Value>,
    pub console_output: Vec<ConsoleEntry>,
    pub duration_ms:   u64,
    pub error_message: Option<String>,
    pub error_stack:   Option<String>,
}

pub struct Sandbox {
    config: SandboxConfig,
}

impl Sandbox {
    pub fn new(config: SandboxConfig) -> Result<Self, anyhow::Error> {
        Ok(Self { config })
    }

    /// Execute JavaScript code in a sandboxed QuickJS runtime.
    /// Uses a dedicated OS thread to avoid blocking the async executor.
    pub async fn execute(
        &self,
        js_code: &str,
        _user_id: Uuid,
    ) -> ExecutionResult {
        let code = js_code.to_string();
        let timeout = Duration::from_secs(self.config.timeout_secs);

        let memory_limit_mb = self.config.memory_limit_mb;

        let result = tokio::time::timeout(
            timeout + Duration::from_millis(500),
            tokio::task::spawn_blocking(move || run_in_quickjs(&code, timeout, memory_limit_mb)),
        )
        .await;

        match result {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => ExecutionResult {
                status:         "error".to_string(),
                return_value:   None,
                console_output: vec![],
                duration_ms:    0,
                error_message:  Some(format!("Thread panicked: {e}")),
                error_stack:    None,
            },
            Err(_) => ExecutionResult {
                status:         "timeout".to_string(),
                return_value:   None,
                console_output: vec![],
                duration_ms:    self.config.timeout_secs * 1000,
                error_message:  Some(format!(
                    "Le script a dépassé le délai d'exécution de {} secondes",
                    self.config.timeout_secs
                )),
                error_stack:    None,
            },
        }
    }
}

fn run_in_quickjs(code: &str, timeout: Duration, memory_limit_mb: u32) -> ExecutionResult {
    use rquickjs::{Context, Runtime};
    use rquickjs::prelude::Rest;

    let start = Instant::now();

    // Build the console capture buffer
    let console_entries: Arc<Mutex<Vec<ConsoleEntry>>> = Arc::new(Mutex::new(Vec::new()));
    let console_clone = Arc::clone(&console_entries);

    let rt = match Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            return ExecutionResult {
                status:         "error".to_string(),
                return_value:   None,
                console_output: vec![],
                duration_ms:    start.elapsed().as_millis() as u64,
                error_message:  Some(format!("Impossible de créer le runtime QuickJS: {e}")),
                error_stack:    None,
            }
        }
    };

    // ── Limites de ressources (anti-DoS) ──────────────────────────────────────
    // 1. Mémoire : QuickJS interrompt l'allocation au-delà de la limite.
    let mem_bytes = (memory_limit_mb as usize).max(1).saturating_mul(1024 * 1024);
    rt.set_memory_limit(mem_bytes);
    // 2. Pile : empêche la récursion infinie de faire planter le thread.
    rt.set_max_stack_size(1024 * 1024);
    // 3. Temps mur : sans handler d'interruption, `tokio::time::timeout` n'annule
    //    JAMAIS le `spawn_blocking` — une boucle `while(true){}` épinglerait le
    //    thread à vie. Le handler est appelé régulièrement par le moteur et, dès
    //    que l'échéance est dépassée, lève une exception non capturable qui rend
    //    le contrôle à l'appelant.
    let deadline = start + timeout;
    rt.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));

    let ctx = match Context::full(&rt) {
        Ok(c) => c,
        Err(e) => {
            return ExecutionResult {
                status:         "error".to_string(),
                return_value:   None,
                console_output: vec![],
                duration_ms:    start.elapsed().as_millis() as u64,
                error_message:  Some(format!("Impossible de créer le contexte QuickJS: {e}")),
                error_stack:    None,
            }
        }
    };

    let result: Result<Option<Value>, String> = ctx.with(|ctx| {
        // ── Inject console ────────────────────────────────────────────────────
        let globals = ctx.globals();

        let console_log = {
            let buf = Arc::clone(&console_clone);
            rquickjs::Function::new(ctx.clone(), move |args: Rest<rquickjs::Value<'_>>| {
                let time_ms = 0u64; // simplified
                let values: Vec<Value> = args.0.iter()
                    .map(|v| js_value_to_json(v))
                    .collect();
                if let Ok(mut guard) = buf.lock() {
                    guard.push(ConsoleEntry { level: "log".to_string(), args: values, time_ms });
                }
                Ok::<(), rquickjs::Error>(())
            })
        }.map_err(|e| e.to_string())?;

        let console_warn = {
            let buf = Arc::clone(&console_clone);
            rquickjs::Function::new(ctx.clone(), move |args: Rest<rquickjs::Value<'_>>| {
                let values: Vec<Value> = args.0.iter().map(|v| js_value_to_json(v)).collect();
                if let Ok(mut guard) = buf.lock() {
                    guard.push(ConsoleEntry { level: "warn".to_string(), args: values, time_ms: 0 });
                }
                Ok::<(), rquickjs::Error>(())
            })
        }.map_err(|e| e.to_string())?;

        let console_error = {
            let buf = Arc::clone(&console_clone);
            rquickjs::Function::new(ctx.clone(), move |args: Rest<rquickjs::Value<'_>>| {
                let values: Vec<Value> = args.0.iter().map(|v| js_value_to_json(v)).collect();
                if let Ok(mut guard) = buf.lock() {
                    guard.push(ConsoleEntry { level: "error".to_string(), args: values, time_ms: 0 });
                }
                Ok::<(), rquickjs::Error>(())
            })
        }.map_err(|e| e.to_string())?;

        let console_obj = rquickjs::Object::new(ctx.clone()).map_err(|e| e.to_string())?;
        console_obj.set("log",   console_log).map_err(|e| e.to_string())?;
        console_obj.set("warn",  console_warn).map_err(|e| e.to_string())?;
        console_obj.set("error", console_error).map_err(|e| e.to_string())?;
        globals.set("console", console_obj).map_err(|e| e.to_string())?;

        // ── Inject Kubuno namespace ────────────────────────────────────────────
        let kubuno_js = r#"
var Kubuno = {
  Utils: {
    template: function(text, vars) {
      return text.replace(/\{\{(\w+)\}\}/g, function(_, k) {
        return vars[k] !== undefined ? String(vars[k]) : '';
      });
    },
    formatDate: function(date) {
      return new Date(date).toISOString();
    },
    generateId: function() {
      return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
        var r = Math.random() * 16 | 0;
        var v = c === 'x' ? r : (r & 0x3 | 0x8);
        return v.toString(16);
      });
    }
  },
  Script: { props: {} },
  Http: {
    get: function(url) { return null; },
    post: function(url, body) { return null; }
  }
};
// ── Durcissement : retirer réellement les globals dangereux ───────────────────
// `var x = undefined` ne supprime rien d'utile ; on écrase les propriétés du
// véritable objet global et on neutralise le constructeur Function atteignable
// via `(function(){}).constructor`, qui permettrait sinon de reconstruire eval.
(function () {
  var G = (typeof globalThis !== 'undefined') ? globalThis : this;
  var dangerous = [
    'eval', 'Function', 'fetch', 'XMLHttpRequest', 'require', 'process',
    'WebAssembly', 'Reflect', 'Proxy', 'SharedArrayBuffer', 'Atomics', 'import'
  ];
  for (var i = 0; i < dangerous.length; i++) {
    try { G[dangerous[i]] = undefined; } catch (e) {}
  }
  try {
    var FunctionCtor = (function () {}).constructor;
    FunctionCtor.prototype.constructor = undefined;
  } catch (e) {}
})();
"#;
        ctx.eval::<(), _>(kubuno_js).map_err(|e| e.to_string())?;

        // ── Execute user code ──────────────────────────────────────────────────
        let ret: rquickjs::Value = ctx.eval(code).map_err(|e| e.to_string())?;
        let ret_json = js_value_to_json(&ret);
        if ret_json.is_null() {
            Ok(None)
        } else {
            Ok(Some(ret_json))
        }
    });

    let duration_ms = start.elapsed().as_millis() as u64;
    let entries = console_entries.lock().map(|g| g.clone()).unwrap_or_default();

    // Check timeout
    if start.elapsed() >= timeout {
        return ExecutionResult {
            status:         "timeout".to_string(),
            return_value:   None,
            console_output: entries,
            duration_ms,
            error_message:  Some("Timeout dépassé".to_string()),
            error_stack:    None,
        };
    }

    match result {
        Ok(ret) => ExecutionResult {
            status:         "success".to_string(),
            return_value:   ret,
            console_output: entries,
            duration_ms,
            error_message:  None,
            error_stack:    None,
        },
        Err(msg) => ExecutionResult {
            status:         "error".to_string(),
            return_value:   None,
            console_output: entries,
            duration_ms,
            error_message:  Some(msg),
            error_stack:    None,
        },
    }
}

fn js_value_to_json(val: &rquickjs::Value) -> Value {
    use rquickjs::Type;
    match val.type_of() {
        Type::Null | Type::Undefined => Value::Null,
        Type::Bool => val.as_bool().map(Value::Bool).unwrap_or(Value::Null),
        Type::Int => val.as_int().map(|n| json!(n)).unwrap_or(Value::Null),
        Type::Float => val.as_float().map(|n| json!(n)).unwrap_or(Value::Null),
        Type::String => val.as_string()
            .and_then(|s| s.to_string().ok())
            .map(Value::String)
            .unwrap_or(Value::Null),
        _ => {
            // For objects/arrays, try JSON serialization via QuickJS
            Value::String(format!("[object]"))
        }
    }
}
