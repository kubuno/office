use crate::{config::Settings, files_client::FilesClient};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;
use std::collections::HashMap;
use tokio::sync::RwLock;

/// Message broadcast aux clients WebSocket d'un document.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CollabMessage {
    ContentUpdated {
        user_id:  Uuid,
        content:  serde_json::Value,
        title:    String,
    },
    PresenceChange {
        user_id: Uuid,
        action:  String,
    },
}

/// Message broadcast aux clients WebSocket d'une feuille de tableur.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SheetMessage {
    SheetUpdated {
        user_id:  Uuid,
        sheet_id: Uuid,
        data:     serde_json::Value,
    },
    PresenceChange {
        user_id: Uuid,
        action:  String,
    },
}

/// Hub de collaboration temps-réel (broadcast par document).
pub struct CollabHub {
    channels: RwLock<HashMap<Uuid, broadcast::Sender<CollabMessage>>>,
}

impl CollabHub {
    pub fn new() -> Self {
        CollabHub { channels: RwLock::new(HashMap::new()) }
    }

    pub async fn subscribe(&self, doc_id: Uuid) -> broadcast::Receiver<CollabMessage> {
        let channels = self.channels.read().await;
        if let Some(tx) = channels.get(&doc_id) {
            return tx.subscribe();
        }
        drop(channels);

        let mut channels = self.channels.write().await;
        let (tx, rx) = broadcast::channel(64);
        channels.insert(doc_id, tx);
        rx
    }

    pub async fn publish(&self, doc_id: Uuid, msg: CollabMessage) {
        let channels = self.channels.read().await;
        if let Some(tx) = channels.get(&doc_id) {
            let _ = tx.send(msg);
        }
    }
}

/// Hub de collaboration pour les feuilles de tableur.
pub struct SpreadsheetCollabHub {
    channels: RwLock<HashMap<Uuid, broadcast::Sender<SheetMessage>>>,
}

impl SpreadsheetCollabHub {
    pub fn new() -> Self {
        SpreadsheetCollabHub { channels: RwLock::new(HashMap::new()) }
    }

    pub async fn subscribe(&self, sheet_id: Uuid) -> broadcast::Receiver<SheetMessage> {
        let channels = self.channels.read().await;
        if let Some(tx) = channels.get(&sheet_id) {
            return tx.subscribe();
        }
        drop(channels);
        let mut channels = self.channels.write().await;
        let (tx, rx) = broadcast::channel(64);
        channels.insert(sheet_id, tx);
        rx
    }

    pub async fn publish(&self, sheet_id: Uuid, msg: SheetMessage) {
        let channels = self.channels.read().await;
        if let Some(tx) = channels.get(&sheet_id) {
            let _ = tx.send(msg);
        }
    }
}

/// Message broadcast aux clients WebSocket d'une présentation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PresentationMessage {
    SlideUpdated {
        user_id:  Uuid,
        slide_id: Uuid,
        elements: serde_json::Value,
    },
    SlideAdded {
        user_id:  Uuid,
        slide_id: Uuid,
        position: i32,
    },
    SlideDeleted {
        user_id:  Uuid,
        slide_id: Uuid,
    },
    SlideReordered {
        user_id: Uuid,
    },
    PresenceChange {
        user_id: Uuid,
        action:  String,
    },
}

/// Hub de collaboration pour les présentations.
pub struct PresentationHub {
    channels: RwLock<HashMap<Uuid, broadcast::Sender<PresentationMessage>>>,
}

impl PresentationHub {
    pub fn new() -> Self {
        PresentationHub { channels: RwLock::new(HashMap::new()) }
    }

    pub async fn subscribe(&self, pres_id: Uuid) -> broadcast::Receiver<PresentationMessage> {
        let channels = self.channels.read().await;
        if let Some(tx) = channels.get(&pres_id) {
            return tx.subscribe();
        }
        drop(channels);
        let mut channels = self.channels.write().await;
        let (tx, rx) = broadcast::channel(64);
        channels.insert(pres_id, tx);
        rx
    }

    pub async fn publish(&self, pres_id: Uuid, msg: PresentationMessage) {
        let channels = self.channels.read().await;
        if let Some(tx) = channels.get(&pres_id) {
            let _ = tx.send(msg);
        }
    }
}

/// Message broadcast aux clients WebSocket d'une page de diagramme.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DiagramMessage {
    PageUpdated {
        user_id: Uuid,
        data:    serde_json::Value,
    },
    PresenceChange {
        user_id: Uuid,
        action:  String,
    },
}

/// Hub de collaboration pour les diagrammes (par page).
pub struct DiagramHub {
    channels: RwLock<HashMap<Uuid, broadcast::Sender<DiagramMessage>>>,
}

impl DiagramHub {
    pub fn new() -> Self {
        DiagramHub { channels: RwLock::new(HashMap::new()) }
    }

    pub async fn subscribe(&self, page_id: Uuid) -> broadcast::Receiver<DiagramMessage> {
        let channels = self.channels.read().await;
        if let Some(tx) = channels.get(&page_id) {
            return tx.subscribe();
        }
        drop(channels);
        let mut channels = self.channels.write().await;
        let (tx, rx) = broadcast::channel(64);
        channels.insert(page_id, tx);
        rx
    }

    pub async fn publish(&self, page_id: Uuid, msg: DiagramMessage) {
        let channels = self.channels.read().await;
        if let Some(tx) = channels.get(&page_id) {
            let _ = tx.send(msg);
        }
    }
}

/// Message broadcast aux clients WebSocket d'un projet.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProjectMessage {
    TaskUpdated {
        user_id: Uuid,
        task_id: Uuid,
        data:    serde_json::Value,
    },
    TaskAdded {
        user_id: Uuid,
        task_id: Uuid,
    },
    TaskDeleted {
        user_id: Uuid,
        task_id: Uuid,
    },
    CpmUpdated {
        user_id: Uuid,
    },
    PresenceChange {
        user_id: Uuid,
        action:  String,
    },
}

/// Hub de collaboration pour les projets.
pub struct ProjectHub {
    channels: RwLock<HashMap<Uuid, broadcast::Sender<ProjectMessage>>>,
}

impl ProjectHub {
    pub fn new() -> Self {
        ProjectHub { channels: RwLock::new(HashMap::new()) }
    }

    pub async fn subscribe(&self, project_id: Uuid) -> broadcast::Receiver<ProjectMessage> {
        let channels = self.channels.read().await;
        if let Some(tx) = channels.get(&project_id) {
            return tx.subscribe();
        }
        drop(channels);
        let mut channels = self.channels.write().await;
        let (tx, rx) = broadcast::channel(64);
        channels.insert(project_id, tx);
        rx
    }

    pub async fn publish(&self, project_id: Uuid, msg: ProjectMessage) {
        let channels = self.channels.read().await;
        if let Some(tx) = channels.get(&project_id) {
            let _ = tx.send(msg);
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db:           PgPool,
    pub settings:     Arc<Settings>,
    pub hub:          Arc<CollabHub>,
    pub sheet_hub:    Arc<SpreadsheetCollabHub>,
    pub pres_hub:     Arc<PresentationHub>,
    pub project_hub:  Arc<ProjectHub>,
    pub diagram_hub:  Arc<DiagramHub>,
    pub files_client: Arc<FilesClient>,
}
