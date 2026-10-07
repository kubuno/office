//! The save session's outside world for a diagram: [`DiagramServer`] answers the office session's
//! [`Server`] with the diagram routes, [`Link`] adds the crash journal.
//!
//! The session (`kubuno_office_desktop_common::api::session`, shared with Documents) compares and saves
//! ONE byte string: the canonical form of the diagram (`crate::doc`). Its rules hold here unchanged —
//! the server content is re-read and its digest compared right before each save (diagrams have no etag
//! at all, so the digest is the only protection), the editing session is joined at open, pinged and
//! left once at close, a 413 is terminal — and a save turns the difference between what the server
//! holds (the read that just passed the guard) and the canonical bytes into the diagram routes: pages
//! created with the window's ids, renamed, reordered, deleted, and the data of each changed page `PUT`.

use std::path::PathBuf;

use kubuno_office_desktop_common::api::client::Error as ApiError;
use kubuno_office_desktop_common::api::remote::FileJournal;
use kubuno_office_desktop_common::api::session::{self, Error, Journal, SaveJob, Server, Snapshot};
use kubuno_office_diagrams_core::file::Page;
use serde_json::{Map, Value};

use super::client::DiagramClient;
use crate::doc::{canonical_of_content, canonical_of_export, pages_from_canonical};

/// Maps a REST failure onto the four shapes the session tells apart.
pub fn map(e: ApiError) -> Error {
    match e {
        ApiError::Precondition => Error::Precondition,
        ApiError::TooLarge => Error::TooLarge,
        ApiError::Forbidden | ApiError::NotFound => Error::NotOwner,
        other => Error::Transport(other.to_string()),
    }
}

/// The diagram routes as the session's server.
pub struct DiagramServer {
    pub client: DiagramClient,
    /// What the server held at the last read (`fetch` or `join`): what a save diffs against.
    last: Option<Vec<Page>>,
}

impl DiagramServer {
    pub fn new(client: DiagramClient) -> Self {
        Self { client, last: None }
    }

    fn remember(&mut self, canonical: &[u8]) {
        self.last = pages_from_canonical(canonical);
    }
}

impl Server for DiagramServer {
    fn fetch(&mut self, id: &str) -> Result<Snapshot, Error> {
        let export = self.client.export_json(id).map_err(map)?;
        let content = canonical_of_export(&export).ok_or_else(|| Error::Transport("export illisible".into()))?;
        self.remember(&content);
        Ok(Snapshot { content, etag: None, editors: Vec::new() })
    }

    fn patch(&mut self, job: &SaveJob) -> Result<Option<String>, Error> {
        let mine = pages_from_canonical(&job.content).ok_or_else(|| Error::Transport("contenu illisible".into()))?;
        let theirs = match self.last.take() {
            Some(p) => p,
            // Never saved blind: read what the server holds first.
            None => pages_from_canonical(&self.fetch(&job.id)?.content).unwrap_or_default(),
        };
        let id = &job.id;
        // Pages created here, then the metadata, then the data, then the order, then the deletions:
        // a failure half-way leaves every page the server had, and the next save completes the rest.
        for p in &mine {
            match theirs.iter().find(|t| t.id == p.id) {
                None => {
                    self.client.create_page(id, &p.id, &p.name, &p.bg_color).map_err(map)?;
                    if !p.data.shapes.is_empty() || !p.data.connectors.is_empty() || p.data.layers.is_some() || !p.data.rest.is_empty() {
                        self.client.put_page_data(id, &p.id, &p.data.to_value()).map_err(map)?;
                    }
                }
                Some(t) => {
                    if t.name != p.name || t.bg_color != p.bg_color {
                        self.client.update_page_meta(id, &p.id, &p.name, &p.bg_color).map_err(map)?;
                    }
                    if t.data != p.data {
                        self.client.put_page_data(id, &p.id, &p.data.to_value()).map_err(map)?;
                    }
                }
            }
        }
        let order: Vec<String> = mine.iter().map(|p| p.id.clone()).collect();
        let kept: Vec<String> = theirs.iter().filter(|t| order.contains(&t.id)).map(|t| t.id.clone()).collect();
        let mine_kept: Vec<String> = order.iter().filter(|i| theirs.iter().any(|t| &t.id == *i)).cloned().collect();
        if kept != mine_kept {
            self.client.reorder_pages(id, &order).map_err(map)?;
        }
        for t in theirs.iter().filter(|t| !order.contains(&t.id)) {
            self.client.delete_page(id, &t.id).map_err(map)?;
        }
        Ok(None)
    }

    fn join(&mut self, id: &str) -> Result<Snapshot, Error> {
        let (content, editors) = self.client.join(id).map_err(map)?;
        let (_, meta) = self.client.get(id).map_err(map)?;
        let content = canonical_of_content(&content, &meta).ok_or_else(|| Error::Transport("contenu illisible".into()))?;
        self.remember(&content);
        Ok(Snapshot { content, etag: None, editors })
    }

    fn ping(&mut self, id: &str) -> Result<(), Error> {
        self.client.ping(id).map_err(map)
    }

    fn leave(&mut self, id: &str) -> Result<(), Error> {
        self.client.leave(id).map_err(map)
    }

    fn config(&mut self) -> Result<Map<String, Value>, Error> {
        self.client.core_config().map_err(map)
    }
}

/// The crash journal of diagrams: `<data>/diagrams/journal` (the sandbox's under `KUBUNO_SANDBOX_DIR`).
pub fn journal_dir() -> Option<PathBuf> {
    kubuno_desktop_account::paths::user_data_dir().ok().map(|d| d.join("diagrams").join("journal"))
}

/// The session host of a real diagram: the routes plus the journal.
pub struct Link {
    pub server: DiagramServer,
    pub journal: FileJournal,
}

impl Server for Link {
    fn fetch(&mut self, id: &str) -> Result<Snapshot, Error> {
        self.server.fetch(id)
    }
    fn patch(&mut self, job: &SaveJob) -> Result<Option<String>, Error> {
        self.server.patch(job)
    }
    fn join(&mut self, id: &str) -> Result<Snapshot, Error> {
        self.server.join(id)
    }
    fn ping(&mut self, id: &str) -> Result<(), Error> {
        self.server.ping(id)
    }
    fn leave(&mut self, id: &str) -> Result<(), Error> {
        self.server.leave(id)
    }
    fn config(&mut self) -> Result<Map<String, Value>, Error> {
        self.server.config()
    }
}

impl Journal for Link {
    fn write(&mut self, id: &str, bytes: &[u8]) -> Result<(), Error> {
        self.journal.write(id, bytes)
    }
    fn read(&mut self, id: &str) -> Option<Vec<u8>> {
        self.journal.read(id)
    }
    fn clear(&mut self, id: &str) {
        self.journal.clear(id)
    }
}

/// Connects to diagram `id` as the account the shell shows. Returns the host and the title; offline,
/// the title is the id and the session opens from the journal.
pub fn connect(id: &str) -> Result<(Link, String), String> {
    let dir = journal_dir().ok_or_else(|| "dossier de données introuvable".to_string())?;
    let account = kubuno_desktop_sync::current_account()
        .map_err(|e| format!("compte : {e}"))?
        .ok_or_else(|| "aucun compte connecté : connectez-vous dans Kubuno Desktop".to_string())?;
    let client = DiagramClient::new(account.key.clone());
    let mut link = Link { server: DiagramServer::new(client), journal: FileJournal::at(dir) };
    let title = match link.server.client.get(id) {
        Ok((d, _)) => d.title,
        Err(ApiError::Transport(_)) if link.journal.read(id).is_some() => id.to_string(),
        Err(e) => return Err(e.to_string()),
    };
    Ok((link, title))
}

/// Re-exported for the window: the session's types.
pub use session::{Protection, Resolution, State};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::client::tests::Fake;
    use kubuno_office_desktop_common::api::client::Response;
    use kubuno_office_diagrams_core::model::PageData;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    type Calls = Arc<Mutex<Vec<(String, String, Option<Value>)>>>;

    fn server(answer: Box<dyn Fn(&str, &str) -> Response + Send + Sync>) -> (DiagramServer, Calls) {
        let seen: Calls = Arc::default();
        let client = DiagramClient::with_wire(Box::new(Fake { seen: seen.clone(), answer }));
        (DiagramServer::new(client), seen)
    }

    fn export() -> Value {
        json!({ "pages": [
            { "id": "a", "name": "Un", "position": 0, "bg_color": "#ffffff", "data": { "shapes": [], "connectors": [] } },
            { "id": "b", "name": "Deux", "position": 1, "bg_color": "#ffffff", "data": { "shapes": [], "connectors": [] } }
        ] })
    }

    #[test]
    fn a_save_turns_the_difference_into_the_diagram_routes() {
        let (mut srv, seen) = server(Box::new(|m, p| if m == "GET" && p.ends_with("/export/json") { Response::json(200, &export()) } else { Response::json(200, &json!({ "page": { "id": "c", "name": "Trois", "position": 2, "bg_color": "#ffffff" } })) }));
        srv.fetch("d").expect("fetch");
        let mut pages = pages_from_canonical(&canonical_of_export(&export()).expect("c")).expect("p");
        pages[0].name = "Premier".into();
        pages[1].data = PageData::from_value(&json!({ "shapes": [{ "id": "s" }], "connectors": [] }));
        pages.swap(0, 1);
        pages.push(Page::new("c", "Trois", PageData::default()));
        pages.retain(|p| p.id != "a");
        let job = SaveJob { id: "d".into(), content: crate::doc::canonical(&pages), content_digest: String::new(), expect: None, if_match: None, idempotency_key: String::new() };
        srv.patch(&job).expect("saved");
        let calls: Vec<(String, String)> = seen.lock().expect("seen").iter().skip(1).map(|(m, p, _)| (m.clone(), p.clone())).collect();
        assert_eq!(
            calls,
            vec![
                ("PUT".into(), "/api/v1/office/diagrams/d/pages/b/data".into()),
                ("POST".into(), "/api/v1/office/diagrams/d/pages".into()),
                ("DELETE".into(), "/api/v1/office/diagrams/d/pages/a".into()),
            ],
            "b's data, the new page c (empty: no data write), a deleted; no reorder (b then c is the server's order once a is gone)"
        );
    }

    #[test]
    fn join_and_fetch_agree_on_the_same_diagram() {
        let (mut srv, _) = server(Box::new(|m, p| {
            if p.ends_with("/editing/join") {
                Response::json(200, &json!({ "content": { "version": 1, "pages": { "b": { "shapes": [], "connectors": [] }, "a": { "shapes": [], "connectors": [] } } }, "editors": [{ "user_id": "u", "display_name": "Ana" }] }))
            } else if p.ends_with("/export/json") {
                Response::json(200, &export())
            } else if m == "GET" {
                Response::json(200, &json!({ "diagram": { "id": "d", "title": "T" }, "pages": [{ "id": "b", "name": "Deux", "position": 1, "bg_color": "#ffffff" }, { "id": "a", "name": "Un", "position": 0, "bg_color": "#ffffff" }] }))
            } else {
                Response::json(200, &json!({}))
            }
        }));
        let joined = srv.join("d").expect("join");
        assert_eq!(joined.editors, vec!["Ana".to_string()]);
        let fetched = srv.fetch("d").expect("fetch");
        assert_eq!(session::Digest::of(&joined.content), session::Digest::of(&fetched.content));
    }

    #[test]
    fn rest_failures_map_to_the_session_shapes() {
        assert_eq!(map(ApiError::TooLarge), Error::TooLarge);
        assert_eq!(map(ApiError::NotFound), Error::NotOwner);
        assert!(matches!(map(ApiError::Transport("x".into())), Error::Transport(_)));
    }
}
