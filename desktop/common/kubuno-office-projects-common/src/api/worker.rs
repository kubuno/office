//! The worker thread of an open server project: it owns the HTTP transport, runs one job at a time (a call of
//! the editor's outbox, a read of the project or of its side lists) and reports each result as an [`Event`]
//! through the window's sink. It never touches the editor: the UI thread decides what to send next
//! ([`crate::session::ProjectSession`]).

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

use kubuno_office_documents_common::api::client::{self, Transport};
use kubuno_office_projects_core::change::HttpCall;
use kubuno_office_projects_core::model::{Baseline, Calendars, ProjectData, Settings, Version};
use kubuno_office_projects_core::resources::{OrgMember, TimeOff};
use serde_json::Value;

use super::routes;

/// What the worker is asked to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    /// One call of the outbox.
    Send { seq: u64, call: HttpCall },
    /// Read the project.
    Fetch,
    /// Read the side lists (calendars, settings, baselines, versions, the instance's default week).
    FetchSide,
    /// Read the resources workspace's side data: the directory members and the cost settings.
    FetchResourceSide,
    /// Read the time off of one resource.
    FetchTimeOff { resource: String },
    /// Stop.
    Close,
}

/// The side lists of a project (each `None` when its read failed).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Side {
    pub calendars: Option<Calendars>,
    pub settings: Option<Settings>,
    pub baselines: Option<Vec<Baseline>>,
    pub versions: Option<Vec<Version>>,
    /// `office.project_default_include_weekends`.
    pub include_weekends: Option<bool>,
}

/// What the worker reports.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The transport is ready (the account the shell shows).
    Connected { me: Option<String> },
    /// No account, no broker: the project opens from its journal, if any.
    ConnectFailed(String),
    /// Call `seq` succeeded.
    Sent { seq: u64, response: Value },
    /// The server refused call `seq` (4xx).
    Refused { seq: u64, message: String },
    /// The server could not be reached (network, 5xx, expired session): try again later.
    Offline { seq: Option<u64>, why: String },
    Fetched(Box<ProjectData>),
    Side(Box<Side>),
    /// The project could not be read (refused, not found).
    FetchFailed(String),
    /// The directory members and the cost settings (`None` when a read failed).
    ResourceSide { members: Option<Vec<OrgMember>>, currency: Option<String>, default_hourly_rate: Option<f64> },
    /// The time off of a resource (`None` when the read failed).
    TimeOff { resource: String, entries: Option<Vec<TimeOff>> },
}

/// Where the events go (the window posts them to its UI thread).
pub type Sink = Arc<dyn Fn(Event) + Send + Sync>;

/// The transport of an account: built on the worker thread (the broker may block).
pub type Connect = Box<dyn FnOnce() -> Result<(Box<dyn Transport + Send>, Option<String>), String> + Send>;

/// A running worker.
pub struct Worker {
    tx: Sender<Job>,
    handle: Option<JoinHandle<()>>,
}

fn classify(e: client::Error) -> Result<String, String> {
    // Ok(message) = a refusal the user must see; Err(why) = try again later.
    match e {
        client::Error::Transport(m) => Err(m),
        client::Error::Unauthorized => Err("session expirée".into()),
        e if e.retryable() => Err(e.to_string()),
        client::Error::Validation(m) | client::Error::Conflict(m) | client::Error::PolicyDisabled(m) if !m.is_empty() => Ok(m),
        client::Error::NotFound => Ok("élément introuvable sur le serveur".into()),
        client::Error::Forbidden => Ok("accès refusé : ce projet est en lecture seule pour vous".into()),
        e => Ok(e.to_string()),
    }
}

fn send(t: &dyn Transport, r: &client::Request) -> Result<Value, client::Error> {
    let resp = t.send(r)?;
    if !resp.ok() {
        return Err(client::Error::from_response(resp.status, &resp.body));
    }
    resp.parse()
}

fn run_job(t: &dyn Transport, id: &str, job: Job) -> Option<Event> {
    match job {
        Job::Close => None,
        Job::Send { seq, call } => {
            let Some(r) = routes::call(&call) else { return Some(Event::Refused { seq, message: format!("méthode {} non prise en charge", call.method) }) };
            Some(match send(t, &r) {
                Ok(v) => Event::Sent { seq, response: v },
                Err(e) => match classify(e) {
                    Ok(message) => Event::Refused { seq, message },
                    Err(why) => Event::Offline { seq: Some(seq), why },
                },
            })
        }
        Job::Fetch => Some(match send(t, &routes::get_project(id)) {
            Ok(v) => match serde_json::from_value::<ProjectData>(v) {
                Ok(d) => Event::Fetched(Box::new(d)),
                Err(e) => Event::FetchFailed(format!("projet illisible : {e}")),
            },
            Err(e) => match classify(e) {
                Ok(m) => Event::FetchFailed(m),
                Err(why) => Event::Offline { seq: None, why },
            },
        }),
        Job::FetchSide => {
            let calendars = send(t, &routes::calendars(id)).ok().and_then(|v| serde_json::from_value(v).ok());
            let settings = send(t, &routes::settings(id)).ok().and_then(|v| serde_json::from_value(v).ok());
            let baselines = send(t, &routes::baselines(id)).ok().and_then(|v| serde_json::from_value(v.get("baselines").cloned().unwrap_or(Value::Null)).ok());
            let versions = send(t, &routes::versions(id)).ok().and_then(|v| serde_json::from_value(v.get("versions").cloned().unwrap_or(v)).ok());
            let include_weekends = send(t, &routes::core_config()).ok().and_then(|v| v.get("config").and_then(|c| c.get("office.project_default_include_weekends")).and_then(Value::as_bool));
            Some(Event::Side(Box::new(Side { calendars, settings, baselines, versions, include_weekends })))
        }
        Job::FetchResourceSide => {
            let members = send(t, &routes::org_members(id)).ok().and_then(|v| serde_json::from_value(v.get("members").cloned().unwrap_or(Value::Null)).ok());
            let costs = send(t, &routes::costs(id)).ok();
            let config = costs.as_ref().and_then(|v| v.get("config"));
            let currency = config.and_then(|c| c.get("currency")).and_then(Value::as_str).map(str::to_string);
            let default_hourly_rate = config.and_then(|c| c.get("default_hourly_rate")).and_then(Value::as_f64);
            Some(Event::ResourceSide { members, currency, default_hourly_rate })
        }
        Job::FetchTimeOff { resource } => {
            let entries = send(t, &routes::time_off(id, &resource)).ok().and_then(|v| serde_json::from_value(v.get("time_off").cloned().unwrap_or(Value::Null)).ok());
            Some(Event::TimeOff { resource, entries })
        }
    }
}

impl Worker {
    /// Starts the worker of project `id`: connects, then runs the jobs in order until [`Job::Close`].
    pub fn start(id: String, connect: Connect, sink: Sink) -> Self {
        let (tx, rx): (Sender<Job>, Receiver<Job>) = channel();
        let handle = std::thread::Builder::new()
            .name("kubuno-projects-worker".into())
            .spawn(move || {
                let (transport, me) = match connect() {
                    Ok(x) => x,
                    Err(e) => {
                        sink(Event::ConnectFailed(e));
                        return;
                    }
                };
                sink(Event::Connected { me });
                while let Ok(job) = rx.recv() {
                    match run_job(transport.as_ref(), &id, job) {
                        Some(ev) => sink(ev),
                        None => break,
                    }
                }
            })
            .ok();
        Self { tx, handle }
    }

    /// A sender of jobs (the session posts through it).
    pub fn sender(&self) -> Sender<Job> {
        self.tx.clone()
    }

    pub fn post(&self, job: Job) {
        let _ = self.tx.send(job);
    }

    /// Asks the worker to stop after what it is doing, and waits for it up to `timeout`.
    pub fn close(mut self, timeout: std::time::Duration) -> bool {
        let _ = self.tx.send(Job::Close);
        let Some(h) = self.handle.take() else { return true };
        let deadline = std::time::Instant::now() + timeout;
        while !h.is_finished() {
            if std::time::Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let _ = h.join();
        true
    }
}

/// Connects as the account the shell shows (its access token borrowed from the shell's broker).
pub fn connect_account() -> Result<(Box<dyn Transport + Send>, Option<String>), String> {
    let account = kubuno_desktop_sync::current_account().map_err(|e| format!("compte : {e}"))?.ok_or_else(|| "aucun compte connecté : connectez-vous dans Kubuno Desktop".to_string())?;
    let me = Some(account.user_id.clone());
    Ok((Box::new(client::SyncTransport::for_account(account)), me))
}

#[cfg(test)]
mod tests {
    use super::*;
    use client::{Request, Response};
    use serde_json::json;
    use std::sync::Mutex;

    struct Fake;
    impl Transport for Fake {
        fn send(&self, r: &Request) -> client::ApiResult<Response> {
            Ok(match (r.method, r.path.as_str()) {
                (client::Method::Get, "/api/v1/office/projects/p") => Response::json(200, &json!({"project": {"id": "p", "title": "T"}, "tasks": []})),
                (client::Method::Post, "/api/v1/office/projects/p/dependencies") => Response::json(422, &json!({"error": "VALIDATION", "message": "Cette liaison créerait une boucle"})),
                (client::Method::Delete, _) => return Err(client::Error::Transport("hors ligne".into())),
                _ => Response::json(200, &json!({"ok": true})),
            })
        }
    }

    #[test]
    fn jobs_report_results_refusals_and_offline() {
        let got = Arc::new(Mutex::new(Vec::new()));
        let g = got.clone();
        let sink: Sink = Arc::new(move |e| g.lock().expect("lock").push(e));
        let w = Worker::start("p".into(), Box::new(|| Ok((Box::new(Fake) as Box<dyn Transport + Send>, Some("me".into())))), sink);
        w.post(Job::Fetch);
        w.post(Job::Send { seq: 1, call: HttpCall { method: "POST", path: "/projects/p/dependencies".into(), body: Some(json!({})) } });
        w.post(Job::Send { seq: 2, call: HttpCall { method: "DELETE", path: "/projects/p/tasks/x".into(), body: None } });
        assert!(w.close(std::time::Duration::from_secs(5)));
        let ev = got.lock().expect("lock").clone();
        assert_eq!(ev[0], Event::Connected { me: Some("me".into()) });
        assert!(matches!(&ev[1], Event::Fetched(d) if d.project.title == "T"));
        assert_eq!(ev[2], Event::Refused { seq: 1, message: "Cette liaison créerait une boucle".into() });
        assert!(matches!(&ev[3], Event::Offline { seq: Some(2), .. }));
    }
}
