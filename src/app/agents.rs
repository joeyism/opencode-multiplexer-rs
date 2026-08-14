use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use crate::{
    app::sessions::{SessionStatus, SessionSummary},
    data::{
        parallel_builds::{ParallelSnapshot, PlanTask, TaskState},
        poller::ChildSessionInfo,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Working,
    SubagentsWorking,
    NeedsInput,
    Error,
    Succeeded,
    Pending,
    Idle,
    Unknown,
    Cancelled,
}

impl AgentStatus {
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Working | Self::SubagentsWorking => "●",
            Self::NeedsInput => "?",
            Self::Error => "✖",
            Self::Succeeded => "✓",
            Self::Pending => "○",
            Self::Idle => "○",
            Self::Unknown => "·",
            Self::Cancelled => "⊘",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentNode {
    pub id: String,
    pub title: String,
    pub status: AgentStatus,
    pub depth: usize,
    pub session_id: Option<String>,
    pub parent_id: Option<String>,
    pub depends_on: Vec<String>,
    pub detail: Option<String>,
    pub cwd: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentGraph {
    pub root_session_id: String,
    pub header: String,
    pub parallel: bool,
    pub nodes: Vec<AgentNode>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentsViewState {
    pub graph: Option<AgentGraph>,
    pub selected: usize,
    pub return_focus: super::focus::AppFocus,
}

impl AgentsViewState {
    pub fn open(&mut self, graph: AgentGraph, return_focus: super::focus::AppFocus) {
        self.open_at(graph, return_focus, None);
    }

    pub fn open_at(
        &mut self,
        graph: AgentGraph,
        return_focus: super::focus::AppFocus,
        selected_id: Option<&str>,
    ) {
        self.graph = Some(graph);
        self.return_focus = return_focus;
        self.selected = self
            .graph
            .as_ref()
            .map(|graph| graph.selected_index(selected_id))
            .unwrap_or(0);
    }

    pub fn replace_graph(&mut self, graph: AgentGraph) {
        let selected_id = self.selected_node().map(|node| node.id.clone());
        self.graph = Some(graph);
        self.selected = self
            .graph
            .as_ref()
            .map(|graph| graph.selected_index(selected_id.as_deref()))
            .unwrap_or(0);
    }

    pub fn close(&mut self) -> super::focus::AppFocus {
        self.graph = None;
        self.selected = 0;
        self.return_focus
    }

    pub fn selected_node(&self) -> Option<&AgentNode> {
        self.graph.as_ref()?.nodes.get(self.selected)
    }

    pub fn move_down(&mut self) {
        if let Some(graph) = self.graph.as_ref() {
            self.selected = (self.selected + 1).min(graph.nodes.len().saturating_sub(1));
        }
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_top(&mut self) {
        self.selected = 0;
    }

    pub fn move_bottom(&mut self) {
        if let Some(graph) = self.graph.as_ref() {
            self.selected = graph.nodes.len().saturating_sub(1);
        }
    }
}

impl AgentGraph {
    pub fn from_snapshot(root: &SessionSummary, snapshot: Option<&ParallelSnapshot>) -> Self {
        if let Some(snapshot) = snapshot {
            let mut graph = Self::from_plan(
                root.session_id.as_deref().unwrap_or_default(),
                snapshot
                    .run
                    .as_ref()
                    .map(|run| run.run_id.as_str())
                    .unwrap_or("plan"),
                snapshot.plan.tasks.clone(),
                snapshot
                    .run
                    .as_ref()
                    .map(|run| run.tasks.clone())
                    .unwrap_or_default(),
            );
            for node in &mut graph.nodes {
                node.cwd = root.cwd.clone();
            }
            return graph;
        }
        Self::from_session(root)
    }

    pub fn from_plan(
        root_session_id: &str,
        run_id: &str,
        tasks: Vec<PlanTask>,
        states: Vec<TaskState>,
    ) -> Self {
        let state_by_id: HashMap<&str, &TaskState> = states
            .iter()
            .map(|state| (state.task_id.as_str(), state))
            .collect();
        let task_ids: HashSet<String> = tasks.iter().map(|task| task.id.clone()).collect();
        let mut remaining = tasks;
        remaining.sort_by(|a, b| a.id.cmp(&b.id));
        let mut emitted = HashSet::new();
        let mut nodes = Vec::new();
        let mut depth = 0;

        while !remaining.is_empty() {
            let mut ready = Vec::new();
            let mut blocked = Vec::new();
            for task in remaining {
                let known_deps_ready = task
                    .depends_on
                    .iter()
                    .filter(|dependency| task_ids.contains(*dependency))
                    .all(|dependency| emitted.contains(dependency));
                if known_deps_ready {
                    ready.push(task);
                } else {
                    blocked.push(task);
                }
            }
            if ready.is_empty() {
                ready = blocked;
                blocked = Vec::new();
            }
            for task in ready {
                emitted.insert(task.id.clone());
                let state = state_by_id.get(task.id.as_str()).copied();
                let status =
                    state.map_or(AgentStatus::Unknown, |state| status_from_run(&state.status));
                nodes.push(AgentNode {
                    id: format!("task:{}", task.id),
                    title: task.title,
                    status,
                    depth,
                    session_id: state.and_then(|state| state.session_id.clone()),
                    parent_id: None,
                    depends_on: task.depends_on,
                    detail: state.and_then(|state| state.error.clone()),
                    cwd: PathBuf::new(),
                });
            }
            remaining = blocked;
            depth += 1;
        }

        Self {
            root_session_id: root_session_id.to_string(),
            header: format!("parallel {run_id}"),
            parallel: true,
            nodes,
        }
    }

    pub fn from_session(root: &SessionSummary) -> Self {
        let root_id = root
            .session_id
            .clone()
            .unwrap_or_else(|| format!("local:{}", root.id));
        let mut nodes = vec![session_node(
            &root_id,
            &root.title,
            root.status,
            0,
            None,
            root.model.clone(),
            root.cwd.clone(),
        )];
        append_children(&mut nodes, &root_id, &root.children, 1);
        Self {
            root_session_id: root_id,
            header: "agents".into(),
            parallel: false,
            nodes,
        }
    }

    pub fn selected_index(&self, selected_id: Option<&str>) -> usize {
        selected_id
            .and_then(|id| self.nodes.iter().position(|node| node.id == id))
            .unwrap_or(0)
            .min(self.nodes.len().saturating_sub(1))
    }
}

fn append_children(
    nodes: &mut Vec<AgentNode>,
    parent_id: &str,
    children: &[ChildSessionInfo],
    depth: usize,
) {
    for child in children {
        nodes.push(session_node(
            &child.session_id,
            &child.title,
            child.status,
            depth,
            Some(parent_id.into()),
            None,
            child.cwd.clone(),
        ));
        append_children(nodes, &child.session_id, &child.children, depth + 1);
    }
}

fn session_node(
    id: &str,
    title: &str,
    status: SessionStatus,
    depth: usize,
    parent_id: Option<String>,
    model: Option<String>,
    cwd: PathBuf,
) -> AgentNode {
    AgentNode {
        id: id.into(),
        title: title.into(),
        status: status_from_session(status),
        depth,
        session_id: Some(id.into()),
        parent_id,
        depends_on: Vec::new(),
        detail: model,
        cwd,
    }
}

fn status_from_session(status: SessionStatus) -> AgentStatus {
    match status {
        SessionStatus::Working => AgentStatus::Working,
        SessionStatus::SubagentsWorking => AgentStatus::SubagentsWorking,
        SessionStatus::NeedsInput => AgentStatus::NeedsInput,
        SessionStatus::Idle => AgentStatus::Idle,
        SessionStatus::Error => AgentStatus::Error,
    }
}

fn status_from_run(status: &str) -> AgentStatus {
    match status {
        "running" | "checking" | "integrating" | "merging" => AgentStatus::Working,
        "succeeded" | "completed" => AgentStatus::Succeeded,
        "failed" | "plan_defect" => AgentStatus::Error,
        "cancelled" => AgentStatus::Cancelled,
        "pending" | "blocked" => AgentStatus::Pending,
        _ => AgentStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::parallel_builds::{PlanTask, TaskState};

    #[test]
    fn plan_tasks_are_ordered_in_stable_dependency_waves() {
        let tasks = vec![
            PlanTask::new("finish", "Finish", vec!["build", "test"]),
            PlanTask::new("test", "Test", vec!["prepare"]),
            PlanTask::new("build", "Build", vec!["prepare"]),
            PlanTask::new("prepare", "Prepare", vec![]),
        ];

        let graph = AgentGraph::from_plan("root", "run", tasks, vec![]);

        assert_eq!(
            graph
                .nodes
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            vec!["task:prepare", "task:build", "task:test", "task:finish"]
        );
        assert_eq!(graph.nodes[0].depth, 0);
        assert_eq!(graph.nodes[1].depth, 1);
        assert_eq!(graph.nodes[2].depth, 1);
        assert_eq!(graph.nodes[3].depth, 2);
    }

    #[test]
    fn plan_task_details_report_blocking_dependencies() {
        let tasks = vec![PlanTask::new("child", "Child", vec!["parent"])];
        let graph = AgentGraph::from_plan(
            "root",
            "run",
            tasks,
            vec![TaskState::new("child", "pending", None, None)],
        );

        assert_eq!(graph.nodes[0].status, AgentStatus::Pending);
        assert_eq!(graph.nodes[0].depends_on, vec!["parent"]);
    }
}
