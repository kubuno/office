//! The save path against a real Kubuno core (ignored by default: it needs one).
//!
//! `KUBUNO_PRES_TEST_URL=http://127.0.0.1:18280 KUBUNO_PRES_TEST_TOKEN_FILE=…token.txt cargo test -p
//! kubuno-office-presentations-common --test live_roundtrip -- --ignored --nocapture`
//!
//! It creates a presentation, then checks what the desktop relies on: a local edit (a new element, a new slide,
//! notes, a background) reaches the server and comes back identical; a slide changed on the server since the
//! last save is reported as a conflict and NOT overwritten; « keep mine » then writes it; a slide over the
//! server's 2 MiB body limit is reported as too large while the others still save.

use std::collections::{HashMap, HashSet};

use kubuno_office_presentations_common::api::client::{ApiResult, Client, Error, Method, Request, Response, Transport};
use kubuno_office_presentations_common::api::live::Remote;
use kubuno_office_presentations_common::api::session::{save, same_json, unsaved, Resolution, Server};
use kubuno_office_presentations_common::model::workspace::Workspace;
use kubuno_office_slides_core::insert::{self, TextBoxOpts};
use kubuno_office_slides_core::model::Deck;

struct Http {
    base: String,
    token: String,
    http: reqwest::blocking::Client,
}

impl Transport for Http {
    fn send(&self, r: &Request) -> ApiResult<Response> {
        let url = format!("{}{}", self.base, r.path);
        let m = match r.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Patch => reqwest::Method::PATCH,
            Method::Delete => reqwest::Method::DELETE,
        };
        let mut b = self.http.request(m, url).bearer_auth(&self.token).header("Content-Type", "application/json");
        for (k, v) in &r.headers {
            b = b.header(k, v);
        }
        let body = match (&r.body, r.method) {
            (Some(x), _) => Some(x.clone()),
            (None, Method::Post) => Some("{}".to_string()),
            _ => None,
        };
        if let Some(x) = body {
            b = b.body(x);
        }
        let resp = b.send().map_err(|e| Error::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        let body = resp.bytes().map_err(|e| Error::Transport(e.to_string()))?.to_vec();
        Ok(Response { status, body })
    }
}

fn client() -> Option<Client> {
    let base = std::env::var("KUBUNO_PRES_TEST_URL").ok()?;
    let token = std::fs::read_to_string(std::env::var("KUBUNO_PRES_TEST_TOKEN_FILE").ok()?).ok()?.trim().to_string();
    let http = reqwest::blocking::Client::builder().build().ok()?;
    Some(Client::new(Box::new(Http { base, token, http })))
}

fn open(remote: &Remote) -> Deck {
    let meta = remote.client.meta(&remote.id).unwrap_or_else(|e| panic!("meta: {e:?}"));
    let (content, _) = remote.client.join(&remote.id).unwrap_or_else(|e| panic!("join: {e:?}"));
    Deck::from_server(&meta.value, &content)
}

#[test]
#[ignore = "needs a running Kubuno core (KUBUNO_PRES_TEST_URL, KUBUNO_PRES_TEST_TOKEN_FILE)"]
fn edits_round_trip_and_conflicts_are_never_overwritten() {
    let Some(c) = client() else { panic!("set KUBUNO_PRES_TEST_URL and KUBUNO_PRES_TEST_TOKEN_FILE") };
    // A fresh presentation.
    let created = c.call(&Request::new(Method::Post, "/api/v1/office/presentations").body(r#"{"title":"Desktop round trip"}"#.into())).unwrap_or_else(|e| panic!("create: {e:?}"));
    let v: serde_json::Value = serde_json::from_slice(&created.body).unwrap_or_default();
    let id = v.get("id").or_else(|| v.pointer("/presentation/id")).or_else(|| v.pointer("/data/id")).and_then(|x| x.as_str()).unwrap_or_else(|| panic!("no id in {v}")).to_string();
    println!("presentation {id}");
    let mut remote = Remote { client: c, id: id.clone() };
    let mut base = open(&remote);
    assert!(!base.slides.is_empty(), "a new presentation has a slide");

    // 1. Local edits: a text box, a second slide, notes, a background.
    let mut ws = Workspace::new(base.clone(), 7);
    ws.insert(1, |ids, z| vec![insert::text_box(ids, "Bonjour « desktop »", &TextBoxOpts { font_size: Some(40.0), ..Default::default() }, z)]);
    ws.set_notes("Notes écrites sur le bureau");
    ws.set_background(r##"{"type":"color","color":"#fde7e1"}"##);
    let first = ws.deck.slides[0].id.clone();
    let second = ws.new_slide_after(Some(&first));
    let local = ws.deck.clone();
    let rep = save(&mut remote, &mut base, &local, &mut HashMap::new(), &HashSet::new());
    println!("save 1: {rep:?}");
    assert!(rep.clean(), "{rep:?}");
    let back = open(&remote);
    assert!(!unsaved(&back, &local), "the server holds exactly the local deck");
    assert_eq!(back.slides.iter().map(|s| s.id.clone()).collect::<Vec<_>>(), [first.clone(), second.clone()]);
    assert!(same_json(&remote.slide(&first).unwrap_or_default(), &local.slides[0].data.to_json()));

    // 2. Someone else changes slide 1 on the server; our stale edit of it must not overwrite theirs.
    let theirs = r#"[{"id":"other","type":"text","x":0.1,"y":0.1,"w":0.3,"h":0.1,"content":{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"theirs"}]}]}}]"#;
    remote.client.put_slide(&id, &first, Some(theirs), None).unwrap_or_else(|e| panic!("put theirs: {e:?}"));
    let mut ws2 = Workspace::new(local.clone(), 8);
    ws2.insert(1, |ids, z| vec![insert::text_box(ids, "mine", &TextBoxOpts::default(), z)]);
    let mine = ws2.deck.clone();
    let mut resolved = HashMap::new();
    let rep = save(&mut remote, &mut base, &mine, &mut resolved, &HashSet::new());
    println!("save 2: {rep:?}");
    assert_eq!(rep.conflicts, [first.clone()]);
    assert!(remote.slide(&first).unwrap_or_default().contains("theirs"), "their version is untouched");
    // « Garder la mienne »: written on the next pass.
    resolved.insert(first.clone(), Resolution::KeepMine);
    let rep = save(&mut remote, &mut base, &mine, &mut resolved, &HashSet::new());
    println!("save 3: {rep:?}");
    assert!(rep.clean(), "{rep:?}");
    assert!(remote.slide(&first).unwrap_or_default().contains("mine"));

    // 3. A slide over 2 MiB is refused (413), terminal for it; the other slides still save.
    let mut ws3 = Workspace::new(mine.clone(), 9);
    ws3.select_slide(&second, false, false);
    ws3.set_notes(&"x".repeat(2_200_000));
    ws3.select_slide(&first, false, false);
    ws3.set_notes("encore");
    let big = ws3.deck.clone();
    let rep = save(&mut remote, &mut base, &big, &mut HashMap::new(), &HashSet::new());
    println!("save 4: conflicts {:?}, too large {:?}, failed {:?}", rep.conflicts, rep.too_large, rep.failed);
    assert_eq!(rep.too_large, [second.clone()]);
    assert!(remote.slide(&first).unwrap_or_default().contains("encore"));
}
