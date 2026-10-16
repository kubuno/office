//! The sample project: what `--sample` and the Visual Studio designer show - a small office move with two
//! phases, links of every kind, a milestone, resources and assignments, scheduled by the shared scheduler.

use kubuno_office_projects_core::calendar::WorkingCalendar;
use kubuno_office_projects_core::cpm;
use kubuno_office_projects_core::model::{Assignment, Dependency, Project, ProjectData, Resource, Task};
use kubuno_office_projects_core::wbs;
use serde_json::{Map, Value};

fn task(id: &str, parent: Option<&str>, pos: i32, name: &str, ty: &str, dur: i32, progress: i32) -> Task {
    let mut t = Task::new(id, "sample");
    t.parent_id = parent.map(str::to_string);
    t.position = pos;
    t.name = name.into();
    t.task_type = ty.into();
    t.duration_days = dur;
    t.progress = progress;
    t.created_at = format!("2026-09-28T09:00:{pos:02}Z");
    t.status = match progress {
        100 => "completed",
        0 => "not_started",
        _ => "in_progress",
    }
    .into();
    t
}

fn dep(n: usize, from: &str, to: &str, ty: &str, lag: i32) -> Dependency {
    Dependency { id: format!("d{n}"), project_id: "sample".into(), from_task_id: from.into(), to_task_id: to.into(), dep_type: ty.into(), lag_days: lag, rest: Map::new() }
}

fn resource(id: &str, name: &str, color: &str, rate: Option<f64>) -> Resource {
    Resource {
        id: id.into(),
        project_id: "sample".into(),
        name: name.into(),
        role: String::new(),
        color: color.into(),
        capacity: 1.0,
        hourly_rate: rate,
        user_id: None,
        avatar_url: None,
        kind: "person".into(),
        unit_label: None,
        overtime_rate: None,
        cost_per_use: None,
        skills: Vec::new(),
        created_at: "2026-09-28T09:00:00Z".into(),
        rest: Map::new(),
    }
}

/// The sample, scheduled (Monday–Friday).
pub fn project() -> ProjectData {
    let project = Project {
        id: "sample".into(),
        owner_id: "me".into(),
        title: "Déménagement des bureaux".into(),
        description: "Projet d'exemple".into(),
        color: "#1e7a6f".into(),
        status: "active".into(),
        start_date: Some("2026-10-05".into()),
        end_date: Some("2026-11-20".into()),
        kind: "management".into(),
        labels: Value::Object(Map::new()),
        created_at: "2026-09-28T09:00:00Z".into(),
        updated_at: "2026-10-01T09:00:00Z".into(),
        ..Project::default()
    };
    let mut tasks = vec![
        task("p1", None, 0, "Préparation", "summary", 1, 0),
        task("t1", Some("p1"), 1, "Inventaire du mobilier", "task", 3, 100),
        task("t2", Some("p1"), 2, "Choix du déménageur", "task", 4, 60),
        task("t3", Some("p1"), 3, "Plan d'aménagement", "task", 5, 20),
        task("m1", Some("p1"), 4, "Validation du plan", "milestone", 0, 0),
        task("p2", None, 5, "Installation", "summary", 1, 0),
        task("t4", Some("p2"), 6, "Câblage réseau", "task", 4, 0),
        task("t5", Some("p2"), 7, "Transport des cartons", "task", 2, 0),
        task("t6", Some("p2"), 8, "Montage des postes", "task", 3, 0),
        task("t7", None, 9, "Inauguration", "milestone", 0, 0),
    ];
    tasks[3].priority = "high".into();
    tasks[6].priority = "critical".into();
    let deps = vec![
        dep(1, "t1", "t3", "FS", 0),
        dep(2, "t2", "t5", "FS", 2),
        dep(3, "t3", "m1", "FS", 0),
        dep(4, "m1", "t4", "FS", 0),
        dep(5, "t4", "t6", "SS", 2),
        dep(6, "t5", "t6", "FS", 0),
        dep(7, "t6", "t7", "FS", 1),
    ];
    let resources = vec![resource("r1", "Alice Martin", "#1a73e8", Some(55.0)), resource("r2", "Bruno Diallo", "#188038", Some(48.0)), resource("r3", "Déménageurs Express", "#e37400", None)];
    let assignments = [("t1", "r1"), ("t2", "r1"), ("t3", "r2"), ("t4", "r2"), ("t5", "r3"), ("t6", "r1"), ("t6", "r2")]
        .iter()
        .enumerate()
        .map(|(i, (t, r))| Assignment { id: format!("a{}", i + 1), task_id: (*t).into(), resource_id: (*r).into(), units: 1.0, rest: Map::new() })
        .collect();
    wbs::renumber(&mut tasks);
    let mut data = ProjectData { project, tasks, dependencies: deps, resources, assignments, rest: Map::new() };
    data.sort_tasks();
    if let Some(start) = data.project.start() {
        let cal = WorkingCalendar::default_week(false, start);
        if let Ok(s) = cpm::schedule(&data.tasks, &data.dependencies, start, &cal) {
            cpm::apply(&mut data.tasks, &s);
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_is_scheduled() {
        let p = project();
        assert_eq!(p.tasks.len(), 10);
        assert!(p.tasks.iter().all(|t| t.early_start.is_some()));
        assert!(p.tasks.iter().any(|t| t.is_critical));
        let inaug = p.task("t7").expect("milestone");
        assert!(inaug.es() > 10);
    }
}
