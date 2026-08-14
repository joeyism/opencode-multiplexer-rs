use crate::app::sessions::SessionStatus;
use crate::data::poller::ChildSessionInfo;
use crate::ops::opencode_events::ServeEvent;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestKind {
    Permission,
    Question,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRequest {
    pub session_id: String,
    pub kind: RequestKind,
    pub port: Option<u16>,
}

#[derive(Debug, Default, Clone)]
pub struct OpenRequestTracker {
    // request_id -> OpenRequest
    open: HashMap<String, OpenRequest>,
}

impl OpenRequestTracker {
    pub fn apply(&mut self, port: u16, event: &ServeEvent) {
        match event {
            ServeEvent::PermissionAsked {
                request_id,
                session_id,
            } => {
                self.open.insert(
                    request_id.clone(),
                    OpenRequest {
                        session_id: session_id.clone(),
                        kind: RequestKind::Permission,
                        port: Some(port),
                    },
                );
            }
            ServeEvent::PermissionReplied { request_id, .. } => {
                self.open.remove(request_id);
            }
            ServeEvent::QuestionAsked {
                request_id,
                session_id,
            } => {
                self.open.insert(
                    request_id.clone(),
                    OpenRequest {
                        session_id: session_id.clone(),
                        kind: RequestKind::Question,
                        port: Some(port),
                    },
                );
            }
            ServeEvent::QuestionResolved { request_id, .. } => {
                self.open.remove(request_id);
            }
            ServeEvent::SessionCreated(_) => {}
        }
    }

    pub fn session_needs_input(&self, session_id: &str) -> bool {
        self.open.values().any(|r| r.session_id == session_id)
    }

    pub fn reconcile_port(&mut self, port: u16, open_permissions: Vec<(String, String)>) {
        // Remove all current permission requests for this port
        self.open
            .retain(|_, v| !(v.port == Some(port) && matches!(v.kind, RequestKind::Permission)));

        // Add new ones
        for (request_id, session_id) in open_permissions {
            self.open.insert(
                request_id,
                OpenRequest {
                    session_id,
                    kind: RequestKind::Permission,
                    port: Some(port),
                },
            );
        }
    }

    pub fn reconcile_questions(&mut self, port: u16, open_questions: Vec<(String, String)>) {
        self.open
            .retain(|_, v| !(v.port == Some(port) && matches!(v.kind, RequestKind::Question)));

        for (request_id, session_id) in open_questions {
            self.open.insert(
                request_id,
                OpenRequest {
                    session_id,
                    kind: RequestKind::Question,
                    port: Some(port),
                },
            );
        }
    }

    pub fn tree_needs_input(&self, session_id: &str, children: &[ChildSessionInfo]) -> bool {
        if self.session_needs_input(session_id) {
            return true;
        }
        for child in children {
            if self.tree_needs_input(&child.session_id, &child.children) {
                return true;
            }
        }
        false
    }

    pub fn overlay_status(
        &self,
        session_id: &str,
        children: &[ChildSessionInfo],
        base: SessionStatus,
    ) -> SessionStatus {
        if self.tree_needs_input(session_id, children) {
            SessionStatus::NeedsInput
        } else {
            base
        }
    }

    pub fn overlay_child_tree(&self, children: &mut [ChildSessionInfo]) {
        for child in children.iter_mut() {
            // Note: tree_needs_input includes self, so this covers child + its descendants
            if self.tree_needs_input(&child.session_id, &child.children) {
                child.status = SessionStatus::NeedsInput;
            }
            self.overlay_child_tree(&mut child.children);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asked_marks_session_as_needing_input() {
        let mut t = OpenRequestTracker::default();
        let event = ServeEvent::PermissionAsked {
            request_id: "p1".into(),
            session_id: "s1".into(),
        };
        t.apply(4200, &event);
        assert!(t.session_needs_input("s1"));
        assert!(!t.session_needs_input("s2"));
    }

    #[test]
    fn replied_clears_request() {
        let mut t = OpenRequestTracker::default();
        let asked = ServeEvent::PermissionAsked {
            request_id: "p1".into(),
            session_id: "s1".into(),
        };
        t.apply(4200, &asked);
        assert!(t.session_needs_input("s1"));

        let replied = ServeEvent::PermissionReplied {
            request_id: "p1".into(),
            session_id: "s1".into(),
        };
        t.apply(4200, &replied);
        assert!(!t.session_needs_input("s1"));
    }

    #[test]
    fn multiple_requests_until_all_cleared() {
        let mut t = OpenRequestTracker::default();
        t.apply(
            4200,
            &ServeEvent::PermissionAsked {
                request_id: "p1".into(),
                session_id: "s1".into(),
            },
        );
        t.apply(
            4200,
            &ServeEvent::PermissionAsked {
                request_id: "p2".into(),
                session_id: "s1".into(),
            },
        );
        assert!(t.session_needs_input("s1"));

        t.apply(
            4200,
            &ServeEvent::PermissionReplied {
                request_id: "p1".into(),
                session_id: "s1".into(),
            },
        );
        assert!(t.session_needs_input("s1"));

        t.apply(
            4200,
            &ServeEvent::PermissionReplied {
                request_id: "p2".into(),
                session_id: "s1".into(),
            },
        );
        assert!(!t.session_needs_input("s1"));
    }

    #[test]
    fn reconcile_port_replaces_only_matching_port_permissions() {
        let mut t = OpenRequestTracker::default();
        // s1 p1 on port 4200
        t.apply(
            4200,
            &ServeEvent::PermissionAsked {
                request_id: "p1".into(),
                session_id: "s1".into(),
            },
        );
        // s2 p2 on port 4201
        t.apply(
            4201,
            &ServeEvent::PermissionAsked {
                request_id: "p2".into(),
                session_id: "s2".into(),
            },
        );

        // Reconcile port 4200: p1 is gone, p3 is new
        t.reconcile_port(4200, vec![("p3".into(), "s3".into())]);

        assert!(!t.session_needs_input("s1"));
        assert!(t.session_needs_input("s2")); // 4201 preserved
        assert!(t.session_needs_input("s3"));
    }

    #[test]
    fn question_asked_and_resolved() {
        let mut t = OpenRequestTracker::default();
        t.apply(
            4200,
            &ServeEvent::QuestionAsked {
                request_id: "q1".into(),
                session_id: "s1".into(),
            },
        );
        assert!(t.session_needs_input("s1"));

        t.apply(
            4200,
            &ServeEvent::QuestionResolved {
                request_id: "q1".into(),
                session_id: "s1".into(),
            },
        );
        assert!(!t.session_needs_input("s1"));
    }
}
