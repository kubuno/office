pub mod config;
pub mod converters;
pub mod errors;
pub mod events;
/// FilesClient + gestion centralisée des noms : face CLIENT du module `files`.
/// Tout le stockage est délégué à `files` (jamais d'accès direct à kubuno-storage).
/// Alias `files_client` conservé pour compat des chemins existants.
pub use kubuno_drive::client as files_client;
pub mod handlers;
pub mod maths;
pub mod middleware;
pub mod models;
pub mod router;
pub mod script;
pub mod services;
pub mod state;
pub mod whiteboard;
