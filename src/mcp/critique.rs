//! A critique offered when a design is finished, and the lessons the person keeps from it
//! (`specs/design-critique/spec.md`). `critique_design` gathers the evidence the agent judges
//! by; `design_lessons` stores what the person approved, in the project store, so every later
//! session sees it in `get_basic_info` and in the next critique.

use crate::io::atomic;
use crate::project::Store;
use crate::model::{Document, NodeId, NodeKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

const FILE: &str = ".design-lessons.json";
/// Lessons are rules read before every design: short enough to scan.
const MAX_RULE: usize = 160;
/// Lint issues listed in a critique; the counts cover the rest.
pub const MAX_ISSUES: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    pub id: String,
    pub rule: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub why: String,
    /// The project (file id) it belongs to; none for every project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

fn path(store: &Store) -> PathBuf {
    store.root.join(FILE)
}

/// Every stored lesson; none when the file is missing or unreadable.
pub fn load(store: &Store) -> Vec<Lesson> {
    crate::vfs::read(path(store)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// The lessons that apply in `project` (its own and the global ones).
pub fn applying(store: &Store, project: Option<&str>) -> Vec<Lesson> {
    load(store).into_iter().filter(|l| l.project.is_none() || l.project.as_deref() == project).collect()
}

fn save(store: &Store, lessons: &[Lesson]) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(lessons).map_err(|e| e.to_string())?;
    atomic::write(&path(store), &json).map_err(|e| format!("could not save lessons: {e}"))
}

/// Store a lesson the person approved and return it.
pub fn add(store: &Store, rule: &str, why: &str, project: Option<String>) -> Result<Lesson, String> {
    let rule = rule.trim();
    if rule.is_empty() {
        return Err("a lesson needs a rule".into());
    }
    if rule.chars().count() > MAX_RULE {
        return Err(format!("keep the rule under {MAX_RULE} characters; put the reason in `why`"));
    }
    let mut all = load(store);
    if let Some(same) = all.iter().find(|l| l.rule.eq_ignore_ascii_case(rule) && l.project == project) {
        return Ok(same.clone());
    }
    let next = all.iter().filter_map(|l| l.id.strip_prefix('L')?.parse::<u32>().ok()).max().unwrap_or(0) + 1;
    let lesson = Lesson { id: format!("L{next}"), rule: rule.into(), why: why.trim().into(), project };
    all.push(lesson.clone());
    save(store, &all)?;
    Ok(lesson)
}

/// Remove a lesson; false when no lesson has that id.
pub fn remove(store: &Store, id: &str) -> Result<bool, String> {
    let mut all = load(store);
    let before = all.len();
    all.retain(|l| l.id != id);
    if all.len() == before {
        return Ok(false);
    }
    save(store, &all)?;
    Ok(true)
}

/// A reviewer's point of view: who they are, what they ask, and how they look.
pub struct Persona {
    pub id: &'static str,
    pub role: &'static str,
    pub asks: &'static str,
    pub method: &'static str,
}

/// The reviewers, the person's vision first: a critique judges the design against what it is
/// for. Sources: Connor and Irizarry's objective/element/effect/rationale critique, Nielsen's
/// heuristics, severity scale and cognitive walkthrough (NN/g), W3C's POUR principles.
pub const PERSONAS: &[Persona] = &[
    Persona {
        id: "vision",
        role: "The person's vision (product owner)",
        asks: "Does each screen achieve the objective, for the audience, as the brief, references and feedback notes describe?",
        method: "For each key element: the objective it serves, the element, its effect on that objective, and why. Read the person's notes (get_feedback) and references first; flag anything the brief asked for that is missing.",
    },
    Persona {
        id: "ux",
        role: "UX designer",
        asks: "Can people understand and use it without help?",
        method: "Nielsen's 10 heuristics: system status, real-world language, control and freedom (undo, exit), consistency, error prevention, recognition over recall, efficiency, minimalism, error recovery, help. Cite the heuristic for each finding.",
    },
    Persona {
        id: "flow",
        role: "Flow reviewer",
        asks: "Do the screens make one coherent journey for each top task?",
        method: "Walk each top task across screens (the `flow` graph, else screen order and names): a clear entry, a way forward and a way back or out on every screen, no dead ends or orphan screens, the same navigation and labels in the same place throughout, what is entered on one screen shown on the next, a confirmation, success and error screen for each action, a first-run or empty entry. At each step ask: will they try this, notice the control, connect it to their goal, see progress?",
    },
    Persona {
        id: "qa",
        role: "QA tester",
        asks: "What breaks with real data and real states?",
        method: "A matrix per screen, each item pass, fail or n/a: empty, loading, error, disabled, focus, long text and names, minimum content, zero, one and many items, missing images, every variable mode, the narrow and wide screen sizes. Overflow and clipping from get_bounds or lint. A state with no design is an open question, not a pass.",
    },
    Persona {
        id: "visual",
        role: "Visual designer",
        asks: "Does the eye go to the right thing first, and does it hold together?",
        method: "Scale and hierarchy (one focal point per screen), balance, contrast between roles, gestalt proximity and alignment, a steady spacing rhythm on the 4px grid, the type scale, every colour a variable, consistent radii and components.",
    },
    Persona {
        id: "accessibility",
        role: "Accessibility reviewer",
        asks: "Can people with low vision, colour blindness, motor or cognitive limits and keyboard or screen-reader users use it?",
        method: "W3C POUR for a design. Perceivable: text contrast 4.5:1 (3:1 for large text and for control borders and icons), colour never the only signal, real text, the layout holds at 320px wide and 200% text. Operable: targets 44px (24px minimum), a focus state designed for every control, headings and a visible current location, undo or confirm on destructive actions, an alternative to every gesture. Understandable: fields keep a visible label (not only a placeholder), errors say how to fix, the same navigation and labels across screens. Robust: controls named for their role in layers and code links.",
    },
    Persona {
        id: "copy",
        role: "Content designer",
        asks: "Is every word clear, specific and kind?",
        method: "Plain language in the users' words, specific buttons (verbs, not OK), no placeholder or lorem text, errors that name the problem and the fix, empty states that say what to do next, neutral wording about people whose gender is unknown.",
    },
    Persona {
        id: "end-user",
        role: "A user from the audience",
        asks: "As this user, doing my top task, where do I get stuck or give up?",
        method: "Take the audience given (else ask): their goal, context and constraints (first visit, in a hurry, on a phone, low vision). Walk the top task screen by screen: what you see, what you think, where you hesitate, the first point you would give up.",
    },
];

/// Severity of a finding (Nielsen): rate on frequency, impact and persistence.
pub const SEVERITY: &[&str] = &[
    "0 not a problem",
    "1 cosmetic: fix if time allows",
    "2 minor: low priority",
    "3 major: fix before handing over",
    "4 catastrophe: blocks the task, fix now",
];

pub const FINDING: &str = "persona, heuristic or check, element (layer name), what was seen, its effect on the objective, evidence (the tool result or screenshot region), severity 0-4, suggestion (kept apart from the observation)";

/// What the agent does with a critique's evidence, in order.
pub const STEPS: &[&str] = &[
    "Objective: without `objective` (what this must achieve, for whom, the top tasks) ask the person first; a critique without one is opinion.",
    "Evidence: fix every lint error and warning above and lint again; get_screenshot each artboard at scale 1 (2 for small text) in every variable mode; read the person's notes (get_feedback) and the `flow` graph.",
    "Review each persona independently: with subagents, one per persona given only its brief and the evidence; alone, one pass per persona, writing its findings before starting the next. Never let reviewers see or debate each other's findings before rating.",
    "Each persona lists findings in the finding format and at least one thing that works. Check every lesson; a broken lesson is a finding.",
    "Merge: one list, duplicates joined, how many personas found each (two or more is confirmed), severity the mean of their independent ratings.",
    "Fix every severity 3 and 4 finding with the smallest edit and check it again; the work is ready when none is open and lint is clean.",
    "Report to the person: what works, then findings by severity (observation, evidence, suggestion, fixed or open). Phrase open ones as questions about the objective.",
    "Ask the person with ask_user which findings to remember (one question per finding: every project, this project, or not). Save only the approved ones with design_lessons {add}: a one-line rule and its reason.",
];

/// The screens of a page and the arrows drawn between them: the flow a critique walks.
pub fn flow(doc: &Document) -> Value {
    let mut screens = crate::editor::screens::declared(doc);
    if screens.is_empty() {
        screens = doc
            .root
            .iter()
            .copied()
            .filter(|id| doc.get(*id).is_some_and(|n| n.is_frame() && crate::editor::screens::device_of(n.rect.w).is_some()))
            .collect();
    }
    let screen_of = |id: NodeId| std::iter::once(id).chain(doc.ancestors(id)).find(|a| screens.contains(a));
    let mut links: Vec<(NodeId, NodeId)> = doc
        .nodes
        .values()
        .filter_map(|n| match &n.kind {
            NodeKind::Connector { start: Some(s), end: Some(e), .. } => Some((screen_of(s.node)?, screen_of(e.node)?)),
            _ => None,
        })
        .filter(|(a, b)| a != b)
        .collect();
    links.sort_by_key(|(a, b)| (a.0, b.0));
    links.dedup();
    let name = |id: &NodeId| doc.get(*id).map_or(String::new(), |n| n.name.clone());
    let mut v = json!({
        "screens": screens.iter().map(name).collect::<Vec<_>>(),
        "links": links.iter().map(|(a, b)| [name(a), name(b)]).collect::<Vec<_>>(),
    });
    if links.is_empty() {
        v["note"] = json!("no arrows between screens: walk the flow by screen order and names, and ask the person for the intended flow");
    } else {
        let from = |id: &NodeId| links.iter().any(|(a, _)| a == id);
        let to = |id: &NodeId| links.iter().any(|(_, b)| b == id);
        v["deadEnds"] = json!(screens.iter().filter(|s| to(s) && !from(s)).map(name).collect::<Vec<_>>());
        v["unreached"] = json!(screens.iter().filter(|s| !to(s) && !from(s)).map(name).collect::<Vec<_>>());
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lessons_are_kept_per_project_and_removed_by_id() {
        let store = Store { root: std::env::temp_dir().join(format!("papercraft-lessons-{:016x}", fastrand::u64(..))) };
        std::fs::create_dir_all(&store.root).unwrap();
        let all = add(&store, "Screens scroll; they never grow", "", None).unwrap();
        let brand = add(&store, "Headlines use the accent colour", "brand book", Some("p1".into())).unwrap();
        assert_eq!(add(&store, "screens scroll; they never grow", "", None).unwrap().id, all.id, "a repeat is not stored twice");
        assert_eq!(applying(&store, Some("p1")).len(), 2);
        assert_eq!(applying(&store, Some("p2")).len(), 1, "a project's lesson stays in it");
        assert!(remove(&store, &brand.id).unwrap());
        assert!(!remove(&store, &brand.id).unwrap());
        assert_eq!(load(&store).len(), 1);
        assert!(add(&store, &"x".repeat(MAX_RULE + 1), "", None).is_err());
        std::fs::remove_dir_all(&store.root).unwrap();
    }
}
