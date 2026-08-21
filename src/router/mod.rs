use axum::{
    middleware,
    routing::{delete, get, patch, post, put},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    handlers::{
        collab_ws,
        diagrams, collab_authz, document_collaborators, spreadsheet_collaborators, presentation_collaborators, project_collaborators, project_modules, project_baselines, project_versions, project_settings, project_calendars, project_charter, project_wbs, project_risks, project_issues, project_costs, project_stakeholders, project_quality, project_communications, project_changes, project_closure, project_procurement, project_portfolio, project_deliverables, project_requirements, time_entries,
        document_comments, document_convert, document_shares, document_templates,
        doc_macros,
        documents, fonts, health, init, presentations, projects, spreadsheet_convert, spreadsheets,
        data_datasources, data_datasets, data_measures, data_model, data_execute, data_reports,
        script_scripts, script_execute, script_triggers, script_runs, script_macros, script_api_types,
        maths_formulas,
        wb_boards, wb_websocket, wb_thumbnail, wb_collaborators,
    },
    middleware::require_auth,
    state::AppState,
};

pub fn build(state: AppState) -> Router {
    let authed = Router::new()
        // Init dossiers utilisateur
        .route("/ensure-folders",                      post(init::ensure_user_folders))
        // Documents (sous-module — namespacé sous /documents, ne squatte plus la racine)
        .route("/documents",                              get(documents::list).post(documents::create))
        .route("/documents/delta",                        get(documents::delta))
        .route("/documents/open-by-file",                 post(documents::open_by_file))
        .route("/documents/:id",                          get(documents::get).patch(documents::update))
        .route("/documents/:id/export/docx",              get(document_convert::export_as_docx))
        .route("/documents/:id/export/odt",               get(document_convert::export_as_odt))
        .route("/documents/:id/save-source",              post(document_convert::save_to_source))
        .route("/spreadsheets/:id/save-source",           post(spreadsheet_convert::save_to_source))
        .route("/documents/import",                       post(document_convert::import_document))
        .route("/documents/:id/trash",                    post(documents::trash))
        .route("/documents/:id/restore",                  post(documents::restore))
        .route("/documents/:id/delete",                   delete(documents::delete))
        .route("/documents/:id/duplicate",                post(documents::duplicate))
        // Versions de documents
        .route("/documents/:id/versions",                 get(documents::list_versions).post(documents::create_version))
        .route("/documents/:id/versions/:ver_id/restore", post(documents::restore_version))
        // Commentaires de documents
        .route("/documents/:doc_id/comments",             get(document_comments::list).post(document_comments::create))
        .route("/documents/:doc_id/comments/:id",         patch(document_comments::update).delete(document_comments::delete))
        .route("/documents/:doc_id/comments/:id/resolve", post(document_comments::resolve))
        // Partages de documents (lien public par token)
        .route("/documents/:doc_id/shares",               get(document_shares::list).post(document_shares::create))
        .route("/documents/:doc_id/shares/:id",           delete(document_shares::revoke))
        // Recherche de destinataires (office-wide, reste à la racine)
        .route("/recipients",                             get(document_collaborators::search_recipients))
        // Collaborateurs de documents
        .route("/documents/:doc_id/collaborators",          get(document_collaborators::list).post(document_collaborators::add))
        .route("/documents/:doc_id/collaborators/:user_id", patch(document_collaborators::update).delete(document_collaborators::remove))
        // Modèles de documents
        .route("/documents/templates",                    get(document_templates::list).post(document_templates::create))
        .route("/documents/templates/:id",                delete(document_templates::delete))
        // Polices personnalisées
        .route("/fonts",                               get(fonts::list).post(fonts::add))
        .route("/fonts/:id",                           delete(fonts::delete))
        // Tableurs
        // Macros « container-bound » (dans la donnée du document) — endpoint générique
        .route("/doc-macros/:doc_type/:doc_id",        get(doc_macros::get).put(doc_macros::put))
        .route("/spreadsheets/open-by-file",           post(spreadsheets::open_by_file))
        .route("/spreadsheets/delta",                  get(spreadsheets::delta))
        .route("/spreadsheets",                        get(spreadsheets::list).post(spreadsheets::create))
        .route("/spreadsheets/:id",                    get(spreadsheets::get).patch(spreadsheets::update))
        .route("/spreadsheets/:id/trash",              post(spreadsheets::trash))
        .route("/spreadsheets/:id/restore",            post(spreadsheets::restore))
        .route("/spreadsheets/:id/delete",             delete(spreadsheets::delete))
        .route("/spreadsheets/:id/duplicate",           post(spreadsheets::duplicate))
        .route("/spreadsheets/:id/sheets",             post(spreadsheets::create_sheet))
        .route("/spreadsheets/:id/sheets/:sheet_id",   get(spreadsheets::get_sheet).patch(spreadsheets::update_sheet).delete(spreadsheets::delete_sheet))
        .route("/spreadsheets/:id/versions",           get(spreadsheets::list_versions).post(spreadsheets::create_version))
        // Export tableur (miroir des exports documents)
        .route("/spreadsheets/:id/export/xlsx",        get(spreadsheet_convert::export_as_xlsx))
        .route("/spreadsheets/:id/export/ods",         get(spreadsheet_convert::export_as_ods))
        // Partage utilisateur-à-utilisateur (collaborateurs) — tableur
        .route("/spreadsheets/:id/collaborators",          get(spreadsheet_collaborators::list).post(spreadsheet_collaborators::add))
        .route("/spreadsheets/:id/collaborators/:user_id", patch(spreadsheet_collaborators::update).delete(spreadsheet_collaborators::remove))
        // Collaboration WebSocket — tableur
        // Collaboration WebSocket — document
        // Présentations
        .route("/presentations/open-by-file",                      post(presentations::open_by_file))
        .route("/presentations/delta",                             get(presentations::delta))
        .route("/presentations",                                   get(presentations::list).post(presentations::create))
        .route("/presentations/:id",                               get(presentations::get).patch(presentations::update))
        .route("/presentations/:id/trash",                         post(presentations::trash))
        .route("/presentations/:id/restore",                       post(presentations::restore))
        .route("/presentations/:id/delete",                        delete(presentations::delete))
        .route("/presentations/:id/duplicate",                     post(presentations::duplicate))
        .route("/presentations/:id/slides",                        get(presentations::list_slides).post(presentations::create_slide))
        .route("/presentations/:id/slides/reorder",                patch(presentations::reorder_slides))
        .route("/presentations/:id/slides/:sid",                   get(presentations::get_slide).put(presentations::update_slide_elements).patch(presentations::update_slide_meta).delete(presentations::delete_slide))
        .route("/presentations/:id/slides/:sid/duplicate",         post(presentations::duplicate_slide))
        .route("/presentations/:id/slides/:sid/thumbnail",         post(presentations::upload_thumbnail))
        // Assets image (sortis du doc Yjs) — dossier caché Office/.media/<id>
        .route("/presentations/:id/assets",                        post(presentations::upload_asset))
        .route("/presentations/:id/assets/:file_id",               get(presentations::get_asset))
        // Partage utilisateur-à-utilisateur (collaborateurs) — présentation
        .route("/presentations/:id/collaborators",                 get(presentation_collaborators::list).post(presentation_collaborators::add))
        .route("/presentations/:id/collaborators/:user_id",        patch(presentation_collaborators::update).delete(presentation_collaborators::remove))
        // Collaboration WebSocket — présentation
        // Projets
        .route("/projects/open-by-file",                           post(projects::open_by_file))
        .route("/projects",                                        get(projects::list).post(projects::create))
        .route("/projects/:id",                                    get(projects::get).patch(projects::update))
        .route("/projects/:id/trash",                              post(projects::trash))
        .route("/projects/:id/restore",                            post(projects::restore))
        .route("/projects/:id/delete",                             delete(projects::delete))
        .route("/projects/:id/duplicate",                          post(projects::duplicate))
        .route("/projects/:id/tasks",                              post(projects::create_task))
        .route("/projects/:id/tasks/:tid",                         patch(projects::update_task).delete(projects::delete_task))
        .route("/projects/:id/tasks/:tid/assign",                  post(projects::assign_resource))
        .route("/projects/:id/tasks/:tid/assign/:rid",             delete(projects::unassign_resource))
        .route("/projects/:id/dependencies",                       post(projects::create_dependency))
        .route("/projects/:id/dependencies/:did",                  delete(projects::delete_dependency))
        .route("/projects/:id/resources",                          get(projects::list_resources).post(projects::create_resource))
        .route("/projects/:id/resources/:rid",                     patch(projects::update_resource).delete(projects::delete_resource))
        .route("/projects/:id/cpm",                                post(projects::compute_cpm))
        // Partage utilisateur-à-utilisateur (collaborateurs) — projet
        .route("/projects/:id/collaborators",                      get(project_collaborators::list).post(project_collaborators::add))
        .route("/projects/:id/collaborators/:user_id",             patch(project_collaborators::update).delete(project_collaborators::remove))
        // Ressources d'un projet cloud : modules Kubuno rattachés
        .route("/projects/:id/modules",                            get(project_modules::list).post(project_modules::attach))
        .route("/projects/:id/modules/:mid",                       delete(project_modules::detach))
        // Plans de référence (baseline) — projet de gestion
        .route("/projects/:id/baselines",                          get(project_baselines::list).post(project_baselines::capture))
        .route("/projects/:id/baselines/:bid",                    patch(project_baselines::update).delete(project_baselines::delete))
        .route("/projects/:id/baselines/:bid/variance",           get(project_baselines::variance))
        // Versions / roadmap — projet de gestion
        // ── Portfolio ──────────────────────────────────────────────────────
        // Its own top-level path, not under /projects, so it can never be read as
        // a project identifier.
        .route("/portfolio",                                       get(project_portfolio::overview))
        // ── Procurement ────────────────────────────────────────────────────
        .route("/projects/:id/procurement",                        get(project_procurement::list).post(project_procurement::create))
        .route("/projects/:id/procurement/:pid",                   patch(project_procurement::update).delete(project_procurement::delete))
        .route("/projects/:id/procurement/:pid/payments",          post(project_procurement::add_payment))
        .route("/projects/:id/procurement/:pid/payments/:yid",     patch(project_procurement::update_payment).delete(project_procurement::delete_payment))
        // ── Closure and lessons learned ────────────────────────────────────
        .route("/projects/:id/closure",                            get(project_closure::overview).put(project_closure::update))
        .route("/projects/:id/closure/close",                      post(project_closure::close))
        .route("/projects/:id/closure/reopen",                     post(project_closure::reopen))
        .route("/projects/:id/lessons",                            get(project_closure::list_lessons).post(project_closure::create_lesson))
        .route("/projects/:id/lessons/:lid",                       patch(project_closure::update_lesson).delete(project_closure::delete_lesson))
        // ── Change control ─────────────────────────────────────────────────
        .route("/projects/:id/changes",                            get(project_changes::list).post(project_changes::create))
        .route("/projects/:id/changes/:cid",                       patch(project_changes::update).delete(project_changes::delete))
        .route("/projects/:id/changes/:cid/assess",                post(project_changes::assess))
        .route("/projects/:id/changes/:cid/decide",                post(project_changes::decide))
        // ── Communications and decisions ───────────────────────────────────
        .route("/projects/:id/communications",                     get(project_communications::plan).post(project_communications::create))
        .route("/projects/:id/communications/log",                 get(project_communications::list_log).post(project_communications::log_sent))
        .route("/projects/:id/communications/:cid",                patch(project_communications::update).delete(project_communications::delete))
        .route("/projects/:id/decisions",                          get(project_communications::list_decisions).post(project_communications::create_decision))
        .route("/projects/:id/decisions/:did",                     patch(project_communications::update_decision).delete(project_communications::delete_decision))
        // ── Quality ────────────────────────────────────────────────────────
        .route("/projects/:id/quality",                            get(project_quality::overview))
        .route("/projects/:id/quality/metrics",                    post(project_quality::create_metric))
        .route("/projects/:id/quality/metrics/:mid",               patch(project_quality::update_metric).delete(project_quality::delete_metric))
        .route("/projects/:id/quality/metrics/:mid/measurements",  post(project_quality::add_measurement))
        .route("/projects/:id/quality/metrics/:mid/measurements/:rid", delete(project_quality::delete_measurement))
        .route("/projects/:id/quality/checks",                     get(project_quality::list_checks).post(project_quality::create_check))
        .route("/projects/:id/quality/checks/:cid",                patch(project_quality::update_check).delete(project_quality::delete_check))
        // ── Stakeholders and RACI ──────────────────────────────────────────
        .route("/projects/:id/stakeholders",                       get(project_stakeholders::list).post(project_stakeholders::create))
        .route("/projects/:id/stakeholders/:sid",                  patch(project_stakeholders::update).delete(project_stakeholders::delete))
        .route("/projects/:id/raci",                               get(project_stakeholders::matrix))
        .route("/projects/:id/tasks/:tid/raci/:sid",               put(project_stakeholders::set_role).delete(project_stakeholders::clear_role))
        // ── Cost and earned value ──────────────────────────────────────────
        .route("/projects/:id/costs",                              get(project_costs::overview))
        .route("/projects/:id/costs/config",                       put(project_costs::update_config))
        .route("/projects/:id/costs/entries",                      get(project_costs::list_entries).post(project_costs::create_entry))
        .route("/projects/:id/costs/entries/:eid",                 patch(project_costs::update_entry).delete(project_costs::delete_entry))
        // ── Risk register ──────────────────────────────────────────────────
        .route("/projects/:id/risks",                              get(project_risks::list).post(project_risks::create))
        .route("/projects/:id/risks/:rid",                         patch(project_risks::update).delete(project_risks::delete))
        // Lives in the issue handler, but belongs here: a risk that came true.
        .route("/projects/:id/risks/:rid/materialize",             post(project_issues::materialize))
        .route("/projects/:id/issues",                             get(project_issues::list).post(project_issues::create))
        .route("/projects/:id/issues/:iid",                        patch(project_issues::update).delete(project_issues::delete))
        // ── Scope: breakdown, deliverables, requirements ──────────────────
        .route("/projects/:id/wbs",                                get(project_wbs::get_wbs))
        .route("/projects/:id/wbs/renumber",                       post(project_wbs::renumber_endpoint))
        .route("/projects/:id/tasks/:tid/dictionary",              get(project_wbs::get_entry).put(project_wbs::update_entry))
        .route("/projects/:id/deliverables",                       get(project_deliverables::list).post(project_deliverables::create))
        .route("/projects/:id/deliverables/:did",                  patch(project_deliverables::update).delete(project_deliverables::delete))
        .route("/projects/:id/deliverables/:did/accept",           post(project_deliverables::accept))
        .route("/projects/:id/deliverables/:did/reject",           post(project_deliverables::reject))
        .route("/projects/:id/requirements",                       get(project_requirements::list).post(project_requirements::create))
        .route("/projects/:id/requirements/:rid",                  patch(project_requirements::update).delete(project_requirements::delete))
        .route("/projects/:id/requirements/:rid/links",            post(project_requirements::add_link))
        .route("/projects/:id/requirements/:rid/links/:lid",       delete(project_requirements::delete_link))
        .route("/projects/:id/traceability",                       get(project_requirements::traceability))
        .route("/projects/:id/charter",                            get(project_charter::get).put(project_charter::update))
        .route("/projects/:id/charter/approve",                    post(project_charter::approve))
        .route("/projects/:id/charter/revise",                     post(project_charter::revise))
        .route("/projects/:id/charter/revisions",                  get(project_charter::revisions))
        .route("/projects/:id/charter/milestones",                 post(project_charter::add_milestone))
        .route("/projects/:id/charter/milestones/generate",        post(project_charter::generate_milestones))
        .route("/projects/:id/charter/milestones/:mid",            patch(project_charter::update_milestone).delete(project_charter::delete_milestone))
        .route("/projects/:id/calendars",                          get(project_calendars::list).post(project_calendars::create))
        .route("/projects/:id/calendars/:cid",                     patch(project_calendars::update).delete(project_calendars::delete))
        .route("/projects/:id/calendars/:cid/exceptions",          put(project_calendars::set_exception))
        .route("/projects/:id/calendars/:cid/exceptions/:day",     delete(project_calendars::delete_exception))
        .route("/projects/:id/settings",                           get(project_settings::get).put(project_settings::update))
        .route("/projects/:id/versions",                           get(project_versions::list).post(project_versions::create))
        .route("/projects/:id/versions/:vid",                      patch(project_versions::update).delete(project_versions::delete))
        .route("/projects/:id/time-entries",                       get(time_entries::list).post(time_entries::create))
        .route("/projects/:id/time-entries/:eid",                  patch(time_entries::update).delete(time_entries::delete))
        // Collaboration WebSocket — projet
        // Diagrammes
        .route("/diagrams/open-by-file",                           post(diagrams::open_by_file))
        .route("/diagrams/delta",                                  get(diagrams::delta))
        .route("/diagrams",                                        get(diagrams::list).post(diagrams::create))
        .route("/diagrams/:id",                                    get(diagrams::get).patch(diagrams::update))
        .route("/diagrams/:id/trash",                              post(diagrams::trash))
        .route("/diagrams/:id/restore",                            post(diagrams::restore))
        .route("/diagrams/:id/delete",                             delete(diagrams::delete))
        .route("/diagrams/:id/duplicate",                          post(diagrams::duplicate))
        .route("/diagrams/:id/export/json",                        get(diagrams::export_json))
        .route("/diagrams/:id/pages",                              get(diagrams::list_pages).post(diagrams::create_page))
        .route("/diagrams/:id/pages/reorder",                      patch(diagrams::reorder_pages))
        .route("/diagrams/:id/pages/:pid",                         get(diagrams::get_page).patch(diagrams::update_page_meta).delete(diagrams::delete_page))
        .route("/diagrams/:id/pages/:pid/data",                    put(diagrams::update_page_data))
        // Editing sessions — documents
        .route("/documents/:id/editing/join",                      post(documents::join_editing))
        .route("/documents/:id/editing/save",                      post(documents::save_editing))
        .route("/documents/:id/editing/leave",                     delete(documents::leave_editing))
        .route("/documents/:id/editing/ping",                      post(documents::ping_editing))
        // Editing sessions — tableurs
        .route("/spreadsheets/:id/editing/join",                   post(spreadsheets::join_editing))
        .route("/spreadsheets/:id/editing/save",                   post(spreadsheets::save_editing))
        .route("/spreadsheets/:id/editing/leave",                  delete(spreadsheets::leave_editing))
        .route("/spreadsheets/:id/editing/ping",                   post(spreadsheets::ping_editing))
        // Editing sessions — présentations
        .route("/presentations/:id/editing/join",                  post(presentations::join_editing))
        .route("/presentations/:id/editing/leave",                 delete(presentations::leave_editing))
        .route("/presentations/:id/editing/ping",                  post(presentations::ping_editing))
        // Editing sessions — diagrammes
        .route("/diagrams/:id/editing/join",                       post(diagrams::join_editing))
        .route("/diagrams/:id/editing/leave",                      delete(diagrams::leave_editing))
        .route("/diagrams/:id/editing/ping",                       post(diagrams::ping_editing))
        // Collaboration WebSocket — diagramme (par page)
        // Formes personnalisées
        .route("/shapes/custom",                                   get(diagrams::list_custom_shapes).post(diagrams::create_custom_shape))
        .route("/shapes/custom/:sid",                              delete(diagrams::delete_custom_shape))
        // ── Data sub-module ────────────────────────────────────────────────────
        // Datasources
        .route("/data/datasources",                                get(data_datasources::list).post(data_datasources::create))
        .route("/data/datasources/:id",                            get(data_datasources::get).patch(data_datasources::update).delete(data_datasources::delete))
        .route("/data/datasources/:id/test",                       post(data_datasources::test_connection))
        // Datasets
        .route("/data/datasets",                                   get(data_datasets::list).post(data_datasets::create))
        .route("/data/datasets/:id",                               get(data_datasets::get).patch(data_datasets::update).delete(data_datasets::delete))
        .route("/data/datasets/:id/refresh",                       post(data_datasets::refresh))
        .route("/data/datasets/:id/preview",                       get(data_datasets::preview))
        .route("/data/datasets/:id/validate-sql",                  post(data_datasets::validate_m))
        // Mesures
        .route("/data/measures",                                   get(data_measures::list).post(data_measures::create))
        .route("/data/measures/:id",                               patch(data_measures::update).delete(data_measures::delete))
        .route("/data/measures/validate",                          post(data_measures::validate))
        // Modèle sémantique
        .route("/data/model",                                      get(data_model::get_model))
        .route("/data/model/relations",                            post(data_model::create_relation))
        .route("/data/model/relations/:id",                        patch(data_model::update_relation).delete(data_model::delete_relation))
        // Exécution
        .route("/data/execute",                                    post(data_execute::execute))
        .route("/data/execute/measure",                            post(data_execute::evaluate_measure))
        // Rapports
        .route("/data/reports",                                    get(data_reports::list).post(data_reports::create))
        .route("/data/reports/open-by-file",                       post(data_reports::open_by_file))
        .route("/data/reports/:id",                                get(data_reports::get).patch(data_reports::update).delete(data_reports::delete))
        .route("/data/reports/:id/trash",                          post(data_reports::trash))
        .route("/data/reports/:id/restore",                        post(data_reports::restore))
        .route("/data/reports/:id/duplicate",                      post(data_reports::duplicate))
        .route("/data/reports/:id/pages",                          post(data_reports::create_page))
        .route("/data/reports/:id/pages/:pid",                     patch(data_reports::update_page).delete(data_reports::delete_page))
        // Widgets
        .route("/data/pages/:id/widgets",                          post(data_reports::create_widget))
        .route("/data/widgets/:id",                                patch(data_reports::update_widget).delete(data_reports::delete_widget))
        .route("/data/widgets/batch",                              patch(data_reports::batch_update_widgets))
        // ── Script sub-module ──────────────────────────────────────────────────
        .route("/script/scripts",                                  get(script_scripts::list).post(script_scripts::create))
        .route("/script/scripts/open-by-file",                     post(script_scripts::open_by_file))
        .route("/script/scripts/:id",                              get(script_scripts::get).patch(script_scripts::update).delete(script_scripts::delete))
        .route("/script/scripts/:id/trash",                        post(script_scripts::trash))
        .route("/script/scripts/:id/restore",                      post(script_scripts::restore))
        .route("/script/scripts/:id/duplicate",                    post(script_scripts::duplicate))
        .route("/script/scripts/:id/compile",                      post(script_scripts::compile))
        .route("/script/scripts/:id/run",                          post(script_execute::run_script))
        .route("/script/scripts/:id/runs",                         get(script_runs::list_for_script))
        .route("/script/scripts/:id/triggers",                     get(script_triggers::list).post(script_triggers::create))
        .route("/script/triggers/:id",                             patch(script_triggers::update).delete(script_triggers::delete))
        .route("/script/triggers/:id/toggle",                      post(script_triggers::toggle))
        .route("/script/runs/:run_id/stream",                      get(script_execute::stream_run))
        .route("/script/runs/:id",                                 get(script_runs::get))
        .route("/script/macros",                                   get(script_macros::list).post(script_macros::create))
        .route("/script/macros/:id",                               delete(script_macros::delete))
        .route("/script/macros/:doc_type/:doc_id",                 get(script_macros::list_for_document))
        .route("/script/macros/:id/run",                           post(script_macros::run_macro))
        .route("/script/api-types",                                get(script_api_types::get_types))
        // ── Maths sub-module ───────────────────────────────────────────────────
        .route("/maths/formulas",                                  get(maths_formulas::list).post(maths_formulas::create))
        .route("/maths/formulas/open-by-file",                     post(maths_formulas::open_by_file))
        .route("/maths/formulas/:id",                              get(maths_formulas::get).patch(maths_formulas::update).delete(maths_formulas::delete))
        .route("/maths/formulas/:id/trash",                        post(maths_formulas::trash))
        .route("/maths/formulas/:id/restore",                      post(maths_formulas::restore))
        .route("/maths/formulas/:id/duplicate",                    post(maths_formulas::duplicate))
        // ── Whiteboard sub-module ──────────────────────────────────────────────
        .route("/whiteboard/boards/delta",                         get(wb_boards::delta))
        .route("/whiteboard/boards",                               get(wb_boards::list).post(wb_boards::create))
        .route("/whiteboard/boards/open-by-file",                  post(wb_boards::open_by_file))
        .route("/whiteboard/boards/:id",                           get(wb_boards::get).patch(wb_boards::update).delete(wb_boards::delete))
        .route("/whiteboard/boards/:id/trash",                     post(wb_boards::trash))
        .route("/whiteboard/boards/:id/restore",                   post(wb_boards::restore))
        .route("/whiteboard/boards/:id/duplicate",                 post(wb_boards::duplicate))
        .route("/whiteboard/boards/:id/thumbnail",                 post(wb_thumbnail::upload_thumbnail))
        .route("/whiteboard/boards/:id/sync",                      get(wb_websocket::ws_handler))
        .route("/whiteboard/boards/:id/collaborators",             get(wb_collaborators::list).post(wb_collaborators::add))
        .route("/whiteboard/boards/:id/collaborators/:user_id",    patch(wb_collaborators::update).delete(wb_collaborators::remove))
        // Collaboration temps réel Yjs générique (documents/spreadsheets/presentations/diagrams)
        .route("/collab/:entity_type/:entity_id/sync",             get(collab_ws::ws_handler))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth))
        .with_state(state.clone());

    // Accès public (documents partagés par token)
    let public = Router::new()
        .route("/public/:token",                       get(document_shares::get_public))
        .with_state(state.clone());

    let system = Router::new()
        .route("/health",                              get(health::health))
        .with_state(state.clone());

    // Routes internes (core → module) : protégées par X-Internal-Secret dans le handler.
    let internal = Router::new()
        .route("/internal/collab/authorize",           post(collab_authz::authorize))
        .with_state(state);

    Router::new()
        .merge(system)
        .merge(public)
        .merge(internal)
        .nest("/", authed)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
