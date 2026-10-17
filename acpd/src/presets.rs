//! Named permission presets for one agent.
//!
//! A preset only names the `workspaceAccess` flags a client can already send on
//! `POST /v1/sessions`. It does not change host config, widen workspace roots, or
//! answer a consent. Auto-approve and shell review stay on the host config and
//! apply to every qualifying session, with or without a preset. The effects text
//! describes that config. It never grants a right the caller's ceiling has turned
//! off: the flags are the intersection, and form elicitation stays off.
use crate::{
    auto_approve::{AutoApproveConfig, AutoRisk},
    connection::WorkspaceAccess,
    shell_review::ShellReviewConfig,
};
use serde::Serialize;

/// What the host config already does for this agent. Read-only.
pub struct Facts<'a> {
    pub auto_approve: Option<&'a AutoApproveConfig>,
    pub shell_review: Option<&'a ShellReviewConfig>,
    /// Registry `ownTools`: the host cannot see this agent's own edits.
    pub own_tools: bool,
    /// The agent runs on another host. Local auto-approve and shell review do not apply.
    pub remote: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub effects: Vec<String>,
    pub workspace_access: WorkspaceAccess,
}

const WRITES_ASK: &str = "Each write asks you before it runs.";
const WRITES_OFF: &str = "Writes are off for this session.";
const COMMANDS_ASK: &str = "Each command asks you before it runs.";
const COMMANDS_OFF: &str = "Commands are off for this session.";
const WORKTREE_EDITS: &str =
    "Edits inside a task worktree that match the host's auto-approve rules are allowed once.";
const ORDINARY_FOLDER: &str = "An ordinary folder is not auto-approved.";
const FOLDER_EDITS: &str =
    "Edits inside the session folder that match the host's auto-approve rules are allowed once.";
const OUTSIDE: &str = "Deletes, fetches and anything outside the folder still ask.";
const UNDECIDED: &str = "Some command lines may be allowed or refused by the host's shell rules. Undecided lines still ask.";
const OWN_TOOLS: &str = "This agent's own tools are not covered. Those still ask you.";

fn auto_on<'a>(facts: &'a Facts<'a>) -> Option<&'a AutoApproveConfig> {
    if facts.remote {
        return None;
    }
    facts
        .auto_approve
        .filter(|policy| policy.max_risk != AutoRisk::None)
}

fn review_on(facts: &Facts<'_>) -> bool {
    !facts.remote && facts.shell_review.is_some_and(|review| review.rules)
}

/// The rights a named preset would use before the session ceiling is applied.
fn bundle() -> WorkspaceAccess {
    WorkspaceAccess {
        read_files: true,
        write_files: true,
        terminal: true,
        form_elicitation: false,
    }
}

/// Intersection with the rights the session is allowed to ask for. A preset cannot
/// turn a refused right back on, and it never opts into form elicitation.
fn cap(bundle: WorkspaceAccess, ceiling: WorkspaceAccess) -> WorkspaceAccess {
    WorkspaceAccess {
        read_files: bundle.read_files && ceiling.read_files,
        write_files: bundle.write_files && ceiling.write_files,
        terminal: bundle.terminal && ceiling.terminal,
        form_elicitation: false,
    }
}

fn push_writes(
    effects: &mut Vec<String>,
    access: WorkspaceAccess,
    policy: Option<&AutoApproveConfig>,
) {
    if !access.write_files {
        effects.push(WRITES_OFF.to_owned());
        return;
    }
    let Some(policy) = policy else {
        effects.push(WRITES_ASK.to_owned());
        return;
    };
    if policy.only_in_worktree {
        effects.push(WORKTREE_EDITS.to_owned());
        effects.push(ORDINARY_FOLDER.to_owned());
    } else {
        effects.push(FOLDER_EDITS.to_owned());
    }
    effects.push(OUTSIDE.to_owned());
}

fn push_commands(effects: &mut Vec<String>, access: WorkspaceAccess, review: bool) {
    if !access.terminal {
        effects.push(COMMANDS_OFF.to_owned());
    } else if review {
        effects.push(UNDECIDED.to_owned());
    } else {
        effects.push(COMMANDS_ASK.to_owned());
    }
}

fn push_own(effects: &mut Vec<String>, facts: &Facts<'_>) {
    if facts.own_tools {
        effects.push(OWN_TOOLS.to_owned());
    }
}

fn make(
    id: &'static str,
    label: &'static str,
    ceiling: WorkspaceAccess,
    fill: impl FnOnce(&mut Vec<String>, WorkspaceAccess),
) -> Preset {
    let access = cap(bundle(), ceiling);
    let mut effects = Vec::new();
    fill(&mut effects, access);
    Preset {
        id,
        label,
        effects,
        workspace_access: access,
    }
}

/// Presets for one agent, in display order. `ceiling` is the rights the session may
/// already ask for. The HTTP handler passes [`WorkspaceAccess::default`].
pub fn presets_for(facts: &Facts<'_>, ceiling: WorkspaceAccess) -> Vec<Preset> {
    let policy = auto_on(facts);
    let review = review_on(facts);
    let mut presets = vec![make(
        "supervised",
        "Supervised",
        ceiling,
        |effects, access| {
            push_writes(effects, access, policy);
            push_commands(effects, access, review);
            push_own(effects, facts);
        },
    )];
    if policy.is_some() && cap(bundle(), ceiling).write_files {
        presets.push(make(
            "auto-edits",
            "Auto-accept edits",
            ceiling,
            |effects, access| {
                push_writes(effects, access, policy);
                push_commands(effects, access, review);
                push_own(effects, facts);
            },
        ));
    }
    if review && cap(bundle(), ceiling).terminal {
        presets.push(make(
            "auto-review",
            "Auto-review",
            ceiling,
            |effects, access| {
                push_writes(effects, access, policy);
                push_commands(effects, access, true);
                push_own(effects, facts);
            },
        ));
    }
    presets
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_review::ModelReviewConfig;

    fn policy(risk: AutoRisk, worktree: bool) -> AutoApproveConfig {
        AutoApproveConfig {
            max_risk: risk,
            only_in_worktree: worktree,
            ..Default::default()
        }
    }
    fn review(rules: bool) -> ShellReviewConfig {
        ShellReviewConfig {
            rules,
            ..Default::default()
        }
    }
    fn facts<'a>(
        auto_approve: Option<&'a AutoApproveConfig>,
        shell_review: Option<&'a ShellReviewConfig>,
        own_tools: bool,
        remote: bool,
    ) -> Facts<'a> {
        Facts {
            auto_approve,
            shell_review,
            own_tools,
            remote,
        }
    }
    fn ids(presets: &[Preset]) -> Vec<&str> {
        presets.iter().map(|preset| preset.id).collect()
    }
    fn text(preset: &Preset) -> String {
        preset.effects.join(" ")
    }

    #[test]
    fn presets_disappear_when_their_config_is_off() {
        let open = WorkspaceAccess::default();
        let off = presets_for(&facts(None, None, false, false), open);
        assert_eq!(ids(&off), ["supervised"]);
        let supervised = &off[0];
        assert!(
            text(supervised).contains(WRITES_ASK),
            "{}",
            text(supervised)
        );
        assert!(
            text(supervised).contains(COMMANDS_ASK),
            "{}",
            text(supervised)
        );
        assert!(!text(supervised).contains("allowed once"));
        assert!(!text(supervised).contains("Undecided"));

        let idle = policy(AutoRisk::None, true);
        let quiet = review(false);
        let named_but_off = presets_for(&facts(Some(&idle), Some(&quiet), false, false), open);
        assert_eq!(ids(&named_but_off), ["supervised"]);

        let edits = policy(AutoRisk::Medium, true);
        let rules = review(true);
        let both = presets_for(&facts(Some(&edits), Some(&rules), true, false), open);
        assert_eq!(ids(&both), ["supervised", "auto-edits", "auto-review"]);
        assert!(both.iter().all(|preset| text(preset).contains(OWN_TOOLS)));
        assert!(text(&both[1]).contains("task worktree"));
        assert!(text(&both[1]).contains(OUTSIDE));
        assert!(text(&both[2]).contains("Undecided lines still ask"));
        assert!(
            both.iter()
                .all(|preset| preset.id != "full" && preset.id != "full-access")
        );
        assert!(
            both.iter()
                .all(|preset| !text(preset).to_lowercase().contains("full access"))
        );

        let anywhere = policy(AutoRisk::Low, false);
        let folder = presets_for(&facts(Some(&anywhere), None, false, false), open);
        assert!(text(&folder[1]).contains("session folder"));
        assert!(!text(&folder[1]).contains("task worktree"));

        let remote = presets_for(&facts(Some(&edits), Some(&rules), true, true), open);
        assert_eq!(ids(&remote), ["supervised"]);
        assert!(!text(&remote[0]).contains("allowed once"));
        assert!(!text(&remote[0]).contains("Undecided"));
        assert!(text(&remote[0]).contains(OWN_TOOLS));
    }

    #[test]
    fn json_shape_omits_form_elicitation_and_secrets() {
        let model = ModelReviewConfig {
            base_url: "https://example.test/secret-path".into(),
            model: "m".into(),
            api_key_env: "SUPER_KEY".into(),
            timeout_seconds: 10,
            max_requests_per_minute: 6,
            max_command_bytes: 2048,
        };
        let edits = AutoApproveConfig {
            max_risk: AutoRisk::Medium,
            model: Some(model.clone()),
            ..Default::default()
        };
        let rules = ShellReviewConfig {
            rules: true,
            model: Some(model),
            ..Default::default()
        };
        let presets = presets_for(
            &facts(Some(&edits), Some(&rules), false, false),
            WorkspaceAccess::default(),
        );
        let body = serde_json::to_string(&presets).unwrap();
        assert!(body.contains("\"id\":\"supervised\""));
        assert!(body.contains(
            "\"workspaceAccess\":{\"readFiles\":true,\"writeFiles\":true,\"terminal\":true}"
        ));
        assert!(!body.contains("formElicitation"));
        assert!(!body.contains("example.test"));
        assert!(!body.contains("SUPER_KEY"));
        assert!(!body.contains("secret-path"));
    }

    #[test]
    fn a_preset_cannot_turn_a_refused_write_or_terminal_back_on() {
        let ceiling = WorkspaceAccess {
            read_files: false,
            write_files: false,
            terminal: false,
            form_elicitation: true,
        };
        let edits = policy(AutoRisk::Medium, false);
        let rules = review(true);
        let presets = presets_for(&facts(Some(&edits), Some(&rules), true, false), ceiling);
        assert_eq!(ids(&presets), ["supervised"]);
        let access = presets[0].workspace_access;
        assert!(!access.read_files);
        assert!(!access.write_files);
        assert!(!access.terminal);
        assert!(!access.form_elicitation);
        let joined = text(&presets[0]);
        assert!(!joined.contains("allowed once"), "{joined}");
        assert!(joined.contains(WRITES_OFF), "{joined}");
        assert!(joined.contains(COMMANDS_OFF), "{joined}");
    }
}
