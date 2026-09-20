// ~~~ tree.rs ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// A multi card used to be one linear chain
// it is now a tree where a step can have several children (branches)
// a linear chain is just a tree nown in which every step has one child
// so all chain logic lives here, in pure functions over &[Item] with no DB
// access, making it unit-testable
//
//  is_tree == false  legacy chain: parent of a step = previous step by position
//  is_tree == true   `Item::parent_id` is authoritative, None = a root path
//                     (attached directly to the cards question)
//
// Vocabulary:
//  root path   a step whose parent is the cards question. Several of them =
//               alternative answers to the same question
//  branch      several children of one step or a fork
//  path        root -> ... -> node, the sequence a review session walks
//  frontier    the due nodes that have no due ancestor, an ancestor must be
//               recalled before anything below it is asked, and failing an
//               ancestor makes every descendant due, which is the chain
//               lock/unlock behaviour generalised to a tree

use std::collections::{HashMap, HashSet};
use std::fmt;

use chrono::{DateTime, Utc};

use crate::models::{Item, StepDraft};

// ~~~ Runtime tree over stored items

pub struct StepTree<'a> {
    items:    &'a [Item],
    parent:   Vec<Option<usize>>,
    children: Vec<Vec<usize>>, // ordered by position
    roots:    Vec<usize>,      // ordered by position
}

impl<'a> StepTree<'a> {
    /// Build a tree over `items` (all the items of ONE card)
    pub fn new(is_tree: bool, items: &'a [Item]) -> Self {
        let n = items.len();

        // indices in display order: position, then original order for ties
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&i| (items[i].position, i));

        let mut parent: Vec<Option<usize>> = vec![None; n];
        if is_tree {
            let by_id: HashMap<&str, usize> = items
                .iter()
                .enumerate()
                .map(|(i, it)| (it.id.as_str(), i))
                .collect();
            for i in 0..n {
                parent[i] = items[i]
                    .parent_id
                    .as_deref()
                    .and_then(|p| by_id.get(p).copied())
                    .filter(|&p| p != i);
            }
            // cut cycles. Walk up from each node. Reaching a node thats
            // already on the *current* walk means its a cycle member, so
            // that member becomes a root. state: 0 new, 1 on this walk, 2 done
            let mut state = vec![0u8; n];
            for start in 0..n {
                let mut walk = Vec::new();
                let mut cur = Some(start);
                while let Some(c) = cur {
                    match state[c] {
                        2 => break,
                        1 => {
                            parent[c] = None;
                            break;
                        }
                        _ => {
                            state[c] = 1;
                            walk.push(c);
                            cur = parent[c];
                        }
                    }
                }
                for c in walk {
                    state[c] = 2;
                }
            }
        } else {
            for w in order.windows(2) {
                parent[w[1]] = Some(w[0]);
            }
        }

        let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut roots = Vec::new();
        for &i in &order {
            match parent[i] {
                Some(p) => children[p].push(i),
                None => roots.push(i),
            }
        }

        Self { items, parent, children, roots }
    }

    pub fn len(&self) -> usize { self.items.len() }
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool { self.items.is_empty() }
    pub fn item(&self, i: usize) -> &Item { &self.items[i] }
    pub fn parent(&self, i: usize) -> Option<usize> { self.parent[i] }
    #[allow(dead_code)]
    pub fn children(&self, i: usize) -> &[usize] { &self.children[i] }
    #[allow(dead_code)]
    pub fn roots(&self) -> &[usize] { &self.roots }
    pub fn is_leaf(&self, i: usize) -> bool { self.children[i].is_empty() }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.items.iter().position(|it| it.id == id)
    }

    /// 0 for a root, 1 for its children, ...
    pub fn depth(&self, i: usize) -> usize {
        let mut d = 0;
        let mut cur = i;
        while let Some(p) = self.parent[cur] {
            d += 1;
            cur = p;
        }
        d
    }

    /// root -> ... -> `i`, inclusive
    pub fn path_to(&self, i: usize) -> Vec<usize> {
        let mut path = vec![i];
        let mut cur = i;
        while let Some(p) = self.parent[cur] {
            path.push(p);
            cur = p;
        }
        path.reverse();
        path
    }

    /// everything below `i` (not `i` itself), in pre-order
    pub fn descendants(&self, i: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack: Vec<usize> = self.children[i].iter().rev().copied().collect();
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(self.children[n].iter().rev().copied());
        }
        out
    }

    /// every node, parents before children, siblings in display order
    pub fn dfs_order(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.len());
        let mut stack: Vec<usize> = self.roots.iter().rev().copied().collect();
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(self.children[n].iter().rev().copied());
        }
        out
    }

    /// one root -> leaf path per leaf, in display order
    pub fn leaf_paths(&self) -> Vec<Vec<usize>> {
        self.dfs_order()
            .into_iter()
            .filter(|&i| self.is_leaf(i))
            .map(|i| self.path_to(i))
            .collect()
    }

    /// the steps a session must target right now, due nodes with no due
    /// ancestor in display order. For a linear chain this is exactly one node
    pub fn frontier(&self, now: DateTime<Utc>) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack: Vec<usize> = self.roots.iter().rev().copied().collect();
        while let Some(n) = stack.pop() {
            if self.items[n].due_at <= now {
                out.push(n); // don't descend: everything below is gated by this
            } else {
                stack.extend(self.children[n].iter().rev().copied());
            }
        }
        out
    }

    /// One review path per frontier node. Each path is the ordered
    /// root -> target list of items, so the last item is always the target
    pub fn due_paths(&self, now: DateTime<Utc>) -> Vec<Vec<Item>> {
        self.frontier(now)
            .into_iter()
            .map(|t| self.path_to(t).into_iter().map(|i| self.items[i].clone()).collect())
            .collect()
    }
}

// ~~~ Strict validation (import) ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeError {
    DuplicateId(String),
    UnknownParent { item: String, parent: String },
    SelfParent(String),
    Cycle(String),
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TreeError::DuplicateId(id) => write!(f, "duplicate step id {id}"),
            TreeError::UnknownParent { item, parent } =>
                write!(f, "step {item} points at parent {parent}, which is not in the same card"),
            TreeError::SelfParent(id) => write!(f, "step {id} is its own parent"),
            TreeError::Cycle(id) => write!(f, "step {id} is part of a parent cycle"),
        }
    }
}

impl std::error::Error for TreeError {}

/// Strict check for the items of one tree card: unique ids, every parent
/// exists in the same card, no cycles
pub fn validate(items: &[Item]) -> Result<(), TreeError> {
    let mut ids: HashSet<&str> = HashSet::new();
    for it in items {
        if !ids.insert(it.id.as_str()) {
            return Err(TreeError::DuplicateId(it.id.clone()));
        }
    }
    for it in items {
        if let Some(p) = it.parent_id.as_deref() {
            if p == it.id {
                return Err(TreeError::SelfParent(it.id.clone()));
            }
            if !ids.contains(p) {
                return Err(TreeError::UnknownParent { item: it.id.clone(), parent: p.to_string() });
            }
        }
    }
    // cycle check on the raw parent pointers
    let parent_of: HashMap<&str, &str> = items
        .iter()
        .filter_map(|it| it.parent_id.as_deref().map(|p| (it.id.as_str(), p)))
        .collect();
    for it in items {
        let mut cur = it.id.as_str();
        for _ in 0..=items.len() {
            match parent_of.get(cur) {
                Some(&p) => cur = p,
                None => break,
            }
        }
        if parent_of.contains_key(cur) {
            return Err(TreeError::Cycle(it.id.clone()));
        }
    }
    Ok(())
}

// ~~~ Editor drafts ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// the card editor works on a flat `Vec<StepDraft>` kept in DFS pre-order
// parent links are draft keys (stable while editing, unlike indices), and
// the invariant is, every drafts parent appears EARLIER in the vec, and each
// subtree is one contiguous run

/// Existing items -> drafts, in DFS order, keeping each step's `db_id`
/// Auto-generated "Step N" labels are turned back into blank names
pub fn drafts_from_items(is_tree: bool, items: &[Item]) -> Vec<StepDraft> {
    let tree = StepTree::new(is_tree, items);
    let order = tree.dfs_order();
    let key_of: HashMap<usize, u32> =
        order.iter().enumerate().map(|(k, &i)| (i, k as u32)).collect();
    order
        .iter()
        .enumerate()
        .map(|(k, &i)| {
            let it = tree.item(i);
            let auto = format!("Step {}", tree.depth(i) + 1);
            StepDraft {
                key: k as u32,
                db_id: Some(it.id.clone()),
                parent: tree.parent(i).map(|p| key_of[&p]),
                name: if it.prompt == auto { String::new() } else { it.prompt.clone() },
                answer: it.answer.clone(),
                image: it.image_path.clone(),
            }
        })
        .collect()
}

pub fn next_key(drafts: &[StepDraft]) -> u32 {
    drafts.iter().map(|d| d.key).max().map_or(0, |m| m + 1)
}

fn index_of_key(drafts: &[StepDraft], key: u32) -> Option<usize> {
    drafts.iter().position(|d| d.key == key)
}

/// `[i, end)` is the subtree rooted at draft `i`
pub fn subtree_end(drafts: &[StepDraft], i: usize) -> usize {
    let mut in_tree: HashSet<u32> = HashSet::new();
    in_tree.insert(drafts[i].key);
    let mut end = i + 1;
    while end < drafts.len() {
        match drafts[end].parent {
            Some(p) if in_tree.contains(&p) => {
                in_tree.insert(drafts[end].key);
                end += 1;
            }
            _ => break,
        }
    }
    end
}

pub fn child_count(drafts: &[StepDraft], key: Option<u32>) -> usize {
    drafts.iter().filter(|d| d.parent == key).count()
}

/// add a new step as the LAST child of `parent` (`None` = a new root path)
/// returns its index. If parent has no children yet this extends the chain
/// if it already has some, this is a new branch
pub fn add_child(drafts: &mut Vec<StepDraft>, parent: Option<u32>, mut d: StepDraft) -> usize {
    d.key = next_key(drafts);
    d.parent = parent;
    let at = match parent {
        Some(p) => match index_of_key(drafts, p) {
            Some(pi) => subtree_end(drafts, pi),
            None => { d.parent = None; drafts.len() }
        },
        None => drafts.len(), // after every existing root subtree
    };
    drafts.insert(at, d);
    at
}

/// splice a new step in right after draft `sel`, it becomes `sel`s only
/// child and adopts all of sels previous children. In a linear chain
/// this is "insert a step after this one". Returns the new index
pub fn insert_after(drafts: &mut Vec<StepDraft>, sel: usize, mut d: StepDraft) -> usize {
    let sel_key = drafts[sel].key;
    d.key = next_key(drafts);
    d.parent = Some(sel_key);
    let new_key = d.key;
    for x in drafts.iter_mut() {
        if x.parent == Some(sel_key) {
            x.parent = Some(new_key);
        }
    }
    drafts.insert(sel + 1, d);
    sel + 1
}

/// remove only draft `i`, its children move up to `i`s parent
/// no other steps are lost so returns nothing, indices after `i` shift down
pub fn remove_splice(drafts: &mut Vec<StepDraft>, i: usize) {
    let key = drafts[i].key;
    let parent = drafts[i].parent;
    for x in drafts.iter_mut() {
        if x.parent == Some(key) {
            x.parent = parent;
        }
    }
    drafts.remove(i);
    // re-sort to restore "subtree contiguous" if the promoted children were
    // ahead of later siblings of `i`
    normalize_order(drafts);
}

/// remove draft `i` AND everything below it. Returns the number removed
pub fn remove_subtree(drafts: &mut Vec<StepDraft>, i: usize) -> usize {
    let end = subtree_end(drafts, i);
    drafts.drain(i..end);
    end - i
}

/// re-derive DFS pre-order from parent links, siblings keep their
/// current relative order
pub fn normalize_order(drafts: &mut Vec<StepDraft>) {
    let keys: HashSet<u32> = drafts.iter().map(|d| d.key).collect();
    for d in drafts.iter_mut() {
        if let Some(p) = d.parent {
            if !keys.contains(&p) || p == d.key {
                d.parent = None;
            }
        }
    }
    let mut kids: HashMap<Option<u32>, Vec<usize>> = HashMap::new();
    for (i, d) in drafts.iter().enumerate() {
        kids.entry(d.parent).or_default().push(i);
    }
    let mut order = Vec::with_capacity(drafts.len());
    let mut stack: Vec<usize> = kids.get(&None).cloned().unwrap_or_default();
    stack.reverse();
    while let Some(i) = stack.pop() {
        order.push(i);
        if let Some(k) = kids.get(&Some(drafts[i].key)) {
            stack.extend(k.iter().rev().copied());
        }
    }
    if order.len() == drafts.len() {
        let old = std::mem::take(drafts);
        let mut slots: Vec<Option<StepDraft>> = old.into_iter().map(Some).collect();
        *drafts = order.into_iter().filter_map(|i| slots[i].take()).collect();
    }
    // a cycle among drafts would leave order shorter, so leave as is
}

/// blank-answer drafts are dropped when saving (as they always were) but
/// their children are kept, they move up to the dropped steps parent
pub fn prune_blank(drafts: &[StepDraft]) -> Vec<StepDraft> {
    let mut out: Vec<StepDraft> = drafts.to_vec();
    let mut i = 0;
    while i < out.len() {
        if out[i].answer.trim().is_empty() {
            let key = out[i].key;
            let parent = out[i].parent;
            for x in out.iter_mut() {
                if x.parent == Some(key) {
                    x.parent = parent;
                }
            }
            out.remove(i);
        } else {
            i += 1;
        }
    }
    normalize_order(&mut out);
    out
}

/// editor-level rule, when a step has several children, the learner has to
/// tell them apart in review, so every one of them needs a name
/// *the same applies to several root paths*
pub fn check_branch_names(drafts: &[StepDraft]) -> Result<(), &'static str> {
    let mut groups: HashMap<Option<u32>, Vec<&StepDraft>> = HashMap::new();
    for d in drafts {
        groups.entry(d.parent).or_default().push(d);
    }
    for kids in groups.values() {
        if kids.len() > 1 && kids.iter().any(|d| d.name.trim().is_empty()) {
            return Err("Every branch needs a name (steps that share a parent, or several \
                        top-level paths) so you can tell them apart in review.");
        }
    }
    Ok(())
}

/// true if the drafts form a single chain (no forks anywhere)
#[allow(dead_code)]
pub fn is_linear(drafts: &[StepDraft]) -> bool {
    let mut count: HashMap<Option<u32>, usize> = HashMap::new();
    for d in drafts {
        *count.entry(d.parent).or_default() += 1;
    }
    count.values().all(|&n| n <= 1)
}

/// depth of every draft (0 = root path), by parent links
pub fn draft_depths(drafts: &[StepDraft]) -> Vec<usize> {
    let idx: HashMap<u32, usize> = drafts.iter().enumerate().map(|(i, d)| (d.key, i)).collect();
    drafts
        .iter()
        .map(|d| {
            let mut depth = 0;
            let mut cur = d.parent;
            while let Some(p) = cur {
                depth += 1;
                cur = idx.get(&p).and_then(|&i| drafts[i].parent);
                if depth > drafts.len() { break; }
            }
            depth
        })
        .collect()
}

/// how one draft should be drawn in the editor outline
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineRow {
    /// Text drawn before the step number: the tree "rails" and, for the first
    /// step of a branch, its connector. A plain chain has none (it stays flat),
    /// only forks add columns. For example, two alternative root paths where
    /// the first one continues and then forks again:
    ///
    /// ┌─ 1. path A            head of the first alternative
    /// │  2. ...               chain continues, rail stays on
    /// │  ├─ 3. ...            first branch of a fork under step 2
    /// │  └─ 3....             last branch
    /// └─ 1. path B            head of the last alternative
    ///    2. ...               rail ends, nothing is left below it
    pub guide:   String,
    /// 1-based position within its path (review calls "step N")
    pub step_no: usize,
    /// `Some(is_last_sibling)` if this step is one of several siblings
    pub branch:  Option<bool>,
}

const RAIL:  &str = "│  ";
const BLANK: &str = "   ";

pub fn outline(drafts: &[StepDraft]) -> Vec<OutlineRow> {
    let depths = draft_depths(drafts);
    let idx: HashMap<u32, usize> = drafts.iter().enumerate().map(|(i, d)| (d.key, i)).collect();
    let mut sibs: HashMap<Option<u32>, Vec<u32>> = HashMap::new();
    for d in drafts {
        sibs.entry(d.parent).or_default().push(d.key);
    }

    // What every row's own children must start with. Drafts are in DFS order so
    // a parent is always computed before its children
    let mut child_prefix: Vec<String> = vec![String::new(); drafts.len()];
    let mut rows = Vec::with_capacity(drafts.len());

    for (i, d) in drafts.iter().enumerate() {
        let prefix = d
            .parent
            .and_then(|p| idx.get(&p).copied())
            .filter(|&p| p < i)
            .map(|p| child_prefix[p].clone())
            .unwrap_or_default();

        let group = &sibs[&d.parent];
        let (guide, branch) = if group.len() > 1 {
            let first = group.first() == Some(&d.key);
            let last  = group.last()  == Some(&d.key);
            // several top-level paths have no parent row above them to hang from,
            // so the first one opens a bracket (┌─) and the rest hang off it
            let connector = if d.parent.is_none() && first {
                "┌─ "
            } else if last {
                "└─ "
            } else {
                "├─ "
            };
            // below a branch, keep the rail going only while later siblings exist
            child_prefix[i] = format!("{prefix}{}", if last { BLANK } else { RAIL });
            (format!("{prefix}{connector}"), Some(last))
        } else {
            child_prefix[i] = prefix.clone();
            (prefix, None)
        };

        rows.push(OutlineRow { guide, step_no: depths[i] + 1, branch });
    }
    rows
}

// ~~~ Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ItemKind, ScaffoldState};
    use chrono::Duration;

    fn item(id: &str, pos: i32, parent: Option<&str>, due_in_min: i64) -> Item {
        Item {
            id: id.into(),
            card_id: "c".into(),
            position: pos,
            parent_id: parent.map(str::to_string),
            kind: ItemKind::Step,
            prompt: format!("Step {pos}"),
            answer: format!("answer {id}"),
            due_at: Utc::now() + Duration::minutes(due_in_min),
            interval_days: 0.0,
            stability: 0.0,
            difficulty: 0.0,
            last_reviewed_at: None,
            lapses: 0,
            review_count: 0,
            confidence_avg: 0.0,
            image_path: None,
            consecutive_fails: 0,
            consecutive_hards: 0,
            scaffold_state: ScaffoldState::Normal,
            scaffold_passes: 0,
            scaffold_pass_date: None,
            weak_spans: Vec::new(),
        }
    }

    fn ids(t: &StepTree, v: &[usize]) -> Vec<String> {
        v.iter().map(|&i| t.item(i).id.clone()).collect()
    }

    // root
    //  |- a1 - a2 - a3
    //  |- b1 - b2
    // (two root paths, like "SSD" vs "HDD" for one question)
    fn two_root_paths(due: [i64; 5]) -> Vec<Item> {
        vec![
            item("a1", 1, None, due[0]),
            item("a2", 2, Some("a1"), due[1]),
            item("b1", 3, None, due[2]),
            item("b2", 4, Some("b1"), due[3]),
            item("a3", 5, Some("a2"), due[4]),
        ]
    }

    // trunk with a fork in the middle
    //  t1 - t2 - a1 - a2
    //          |- b1
    fn forked() -> Vec<Item> {
        vec![
            item("t1", 1, None, -1),
            item("t2", 2, Some("t1"), -1),
            item("a1", 3, Some("t2"), -1),
            item("a2", 4, Some("a1"), -1),
            item("b1", 5, Some("t2"), -1),
        ]
    }

    #[test]
    fn legacy_chain_derives_parents_from_position() {
        // parent_id is ignored when is_tree == false
        let mut items = vec![
            item("s1", 1, None, -1),
            item("s2", 2, Some("nonsense"), -1),
            item("s3", 3, None, -1),
        ];
        items[2].parent_id = Some("s1".into());
        let t = StepTree::new(false, &items);
        assert_eq!(t.roots(), &[0]);
        assert_eq!(t.parent(1), Some(0));
        assert_eq!(t.parent(2), Some(1));
        assert_eq!(t.depth(2), 2);
    }

    #[test]
    fn legacy_chain_is_ordered_by_position_not_slice_order() {
        let items = vec![item("s3", 3, None, -1), item("s1", 1, None, -1), item("s2", 2, None, -1)];
        let t = StepTree::new(false, &items);
        assert_eq!(ids(&t, &t.path_to(0)), ["s1", "s2", "s3"]);
    }

    #[test]
    fn tree_parents_children_and_order() {
        let items = forked();
        let t = StepTree::new(true, &items);
        assert_eq!(t.roots(), &[0]);
        assert_eq!(ids(&t, t.children(1)), ["a1", "b1"]); // t2's children by position
        assert_eq!(ids(&t, &t.dfs_order()), ["t1", "t2", "a1", "a2", "b1"]);
        assert_eq!(t.depth(3), 3); // a2
        assert_eq!(ids(&t, &t.path_to(3)), ["t1", "t2", "a1", "a2"]);
    }

    #[test]
    fn descendants_of_trunk_cover_both_branches_but_leaf_is_isolated() {
        let items = forked();
        let t = StepTree::new(true, &items);
        let trunk = t.index_of("t2").unwrap();
        assert_eq!(ids(&t, &t.descendants(trunk)), ["a1", "a2", "b1"]);
        // forgetting a1 must not touch the sibling branch b1
        let a1 = t.index_of("a1").unwrap();
        assert_eq!(ids(&t, &t.descendants(a1)), ["a2"]);
        assert!(t.descendants(t.index_of("b1").unwrap()).is_empty());
    }

    #[test]
    fn leaf_paths_walk_each_branch_from_the_root() {
        let items = forked();
        let t = StepTree::new(true, &items);
        let paths: Vec<Vec<String>> = t.leaf_paths().iter().map(|p| ids(&t, p)).collect();
        assert_eq!(paths, vec![
            vec!["t1", "t2", "a1", "a2"],
            vec!["t1", "t2", "b1"],
        ]);
    }

    #[test]
    fn frontier_is_first_due_step_for_a_linear_chain() {
        // has to match the old `position(|it| it.due_at <= now)` rule
        for first_due in 0..4 {
            let items: Vec<Item> = (0..4)
                .map(|i| {
                    let due = if i >= first_due { -1 } else { 60 };
                    item(&format!("s{i}"), i as i32 + 1, None, due)
                })
                .collect();
            let t = StepTree::new(false, &items);
            let f = t.frontier(Utc::now());
            assert_eq!(f, vec![first_due], "first_due={first_due}");
            // and the path is the old `items[..=idx]`
            let paths = t.due_paths(Utc::now());
            assert_eq!(paths.len(), 1);
            let got: Vec<&str> = paths[0].iter().map(|i| i.id.as_str()).collect();
            let want: Vec<String> = (0..=first_due).map(|i| format!("s{i}")).collect();
            assert_eq!(got, want.iter().map(String::as_str).collect::<Vec<_>>());
        }
    }

    #[test]
    fn frontier_nothing_due() {
        let items: Vec<Item> = (0..3).map(|i| item(&format!("s{i}"), i + 1, None, 60)).collect();
        assert!(StepTree::new(false, &items).frontier(Utc::now()).is_empty());
    }

    #[test]
    fn brand_new_root_fork_targets_both_paths() {
        // everything due: both root steps are frontier, deeper ones are gated
        let items = two_root_paths([-1, -1, -1, -1, -1]);
        let t = StepTree::new(true, &items);
        assert_eq!(ids(&t, &t.frontier(Utc::now())), ["a1", "b1"]);
        let paths = t.due_paths(Utc::now());
        assert!(paths.iter().all(|p| p.len() == 1));
    }

    #[test]
    fn frontier_skips_gated_descendants_and_finds_independent_branches() {
        //  t1 (not due) - t2 (not due) - a1 (due) - a2 (due)
        //                              |- b1 (not due)
        let mut items = forked();
        items[0].due_at = Utc::now() + Duration::days(3);
        items[1].due_at = Utc::now() + Duration::days(3);
        items[4].due_at = Utc::now() + Duration::days(3);
        let t = StepTree::new(true, &items);
        assert_eq!(ids(&t, &t.frontier(Utc::now())), ["a1"]); // a2 is gated by a1
        let paths = t.due_paths(Utc::now());
        let got: Vec<&str> = paths[0].iter().map(|i| i.id.as_str()).collect();
        assert_eq!(got, ["t1", "t2", "a1"]); // trunk included as context, target last
    }

    #[test]
    fn frontier_two_branches_due_at_once() {
        let mut items = forked();
        items[0].due_at = Utc::now() + Duration::days(3);
        items[1].due_at = Utc::now() + Duration::days(3);
        let t = StepTree::new(true, &items);
        assert_eq!(ids(&t, &t.frontier(Utc::now())), ["a1", "b1"]);
    }

    #[test]
    fn due_trunk_gates_everything_below_it() {
        let mut items = forked();
        items[2].due_at = Utc::now() - Duration::minutes(5);
        items[3].due_at = Utc::now() - Duration::minutes(5);
        items[4].due_at = Utc::now() - Duration::minutes(5);
        items[0].due_at = Utc::now() + Duration::days(3);
        items[1].due_at = Utc::now() - Duration::minutes(1); // t2 due
        let t = StepTree::new(true, &items);
        assert_eq!(ids(&t, &t.frontier(Utc::now())), ["t2"]);
    }

    #[test]
    fn dangling_parent_becomes_root_and_self_parent_is_ignored() {
        let items = vec![
            item("x", 1, Some("ghost"), -1),
            item("y", 2, Some("y"), -1),
        ];
        let t = StepTree::new(true, &items);
        assert_eq!(t.roots(), &[0, 1]);
    }

    #[test]
    fn cycles_are_cut_not_looped() {
        let items = vec![
            item("a", 1, Some("c"), -1),
            item("b", 2, Some("a"), -1),
            item("c", 3, Some("b"), -1),
            item("tail", 4, Some("c"), -1), // hangs off the cycle, must stay attached
        ];
        let t = StepTree::new(true, &items);
        assert_eq!(t.roots().len(), 1, "exactly one cycle member is promoted to root");
        assert_eq!(t.dfs_order().len(), 4, "every node is reachable exactly once");
        let tail = t.index_of("tail").unwrap();
        assert_eq!(t.parent(tail), Some(t.index_of("c").unwrap()));
    }

    #[test]
    fn validate_accepts_good_and_rejects_bad_trees() {
        assert_eq!(validate(&forked()), Ok(()));
        assert_eq!(validate(&two_root_paths([0; 5])), Ok(()));

        let bad = vec![item("a", 1, Some("nope"), 0)];
        assert!(matches!(validate(&bad), Err(TreeError::UnknownParent { .. })));

        let me = vec![item("a", 1, Some("a"), 0)];
        assert_eq!(validate(&me), Err(TreeError::SelfParent("a".into())));

        let cyc = vec![item("a", 1, Some("b"), 0), item("b", 2, Some("a"), 0)];
        assert!(matches!(validate(&cyc), Err(TreeError::Cycle(_))));

        let dup = vec![item("a", 1, None, 0), item("a", 2, None, 0)];
        assert_eq!(validate(&dup), Err(TreeError::DuplicateId("a".into())));
    }

    // ~~ drafts ~~

    fn draft(name: &str, answer: &str) -> StepDraft {
        StepDraft { key: 0, db_id: None, parent: None, name: name.into(), answer: answer.into(), image: None }
    }

    fn names(d: &[StepDraft]) -> Vec<String> {
        d.iter().map(|x| if x.name.is_empty() { x.answer.clone() } else { x.name.clone() }).collect()
    }

    fn parent_names(d: &[StepDraft]) -> Vec<Option<String>> {
        let nm = |k: u32| d.iter().find(|x| x.key == k).map(|x| x.answer.clone()).unwrap();
        d.iter().map(|x| x.parent.map(nm)).collect()
    }

    #[test]
    fn drafts_roundtrip_from_items_keeps_ids_and_strips_auto_labels() {
        let mut items = forked();
        items[4].prompt = "HDD".into(); // b1 is named; the rest are auto "Step N"
        items[2].prompt = "Step 3".into(); // a1 depth 2 -> auto label is "Step 3"
        let d = drafts_from_items(true, &items);
        assert_eq!(d.iter().map(|x| x.db_id.clone().unwrap()).collect::<Vec<_>>(),
                   ["t1", "t2", "a1", "a2", "b1"]);
        assert!(d[0].name.is_empty() && d[2].name.is_empty());
        assert_eq!(d[4].name, "HDD");
        // parents expressed through keys
        assert_eq!(d[0].parent, None);
        assert_eq!(d[1].parent, Some(d[0].key));
        assert_eq!(d[4].parent, Some(d[1].key));
    }

    #[test]
    fn add_child_extends_then_branches() {
        let mut d = Vec::new();
        let r = add_child(&mut d, None, draft("", "root"));
        let rk = d[r].key;
        let a = add_child(&mut d, Some(rk), draft("", "a"));
        let a_key = d[a].key;
        add_child(&mut d, Some(rk), draft("B", "b")); // second child of root = fork
        add_child(&mut d, Some(a_key), draft("", "a2")); // goes under a, i.e. before B
        assert_eq!(names(&d), ["root", "a", "a2", "B"]);
        assert_eq!(check_branch_names(&d), Err("Every branch needs a name (steps that share a parent, or several top-level paths) so you can tell them apart in review."));
        d[1].name = "A".into();
        assert!(check_branch_names(&d).is_ok());
        assert!(!is_linear(&d));
    }

    #[test]
    fn add_child_with_none_parent_creates_a_second_root_path() {
        let mut d = Vec::new();
        add_child(&mut d, None, draft("SSD", "s1"));
        let s1 = d[0].key;
        add_child(&mut d, Some(s1), draft("", "s2"));
        add_child(&mut d, None, draft("HDD", "h1"));
        assert_eq!(names(&d), ["SSD", "s2", "HDD"]);
        assert_eq!(child_count(&d, None), 2);
    }

    #[test]
    fn insert_after_splices_and_adopts_children() {
        let mut d = Vec::new();
        add_child(&mut d, None, draft("", "s1"));
        let k1 = d[0].key;
        add_child(&mut d, Some(k1), draft("", "s2"));
        insert_after(&mut d, 0, draft("", "new"));
        assert_eq!(names(&d), ["s1", "new", "s2"]);
        assert_eq!(parent_names(&d), [None, Some("s1".into()), Some("new".into())]);
        assert!(is_linear(&d));
    }

    #[test]
    fn remove_splice_keeps_children_and_restores_contiguity() {
        // fork: root ┬ A ─ a ; └ B ; delete A -> `a` is promoted under root
        let mut d = Vec::new();
        add_child(&mut d, None, draft("", "root"));
        let rk = d[0].key;
        add_child(&mut d, Some(rk), draft("A", "A"));
        let ak = d[1].key;
        add_child(&mut d, Some(rk), draft("B", "B"));
        add_child(&mut d, Some(ak), draft("", "a"));
        assert_eq!(names(&d), ["root", "A", "a", "B"]);
        remove_splice(&mut d, 1);
        assert_eq!(names(&d), ["root", "a", "B"]);
        assert_eq!(parent_names(&d), [None, Some("root".into()), Some("root".into())]);
    }

    #[test]
    fn remove_subtree_drops_the_whole_branch() {
        let mut d = Vec::new();
        add_child(&mut d, None, draft("", "root"));
        let rk = d[0].key;
        add_child(&mut d, Some(rk), draft("A", "A"));
        let ak = d[1].key;
        add_child(&mut d, Some(ak), draft("", "a"));
        add_child(&mut d, Some(rk), draft("B", "B"));
        assert_eq!(remove_subtree(&mut d, 1), 2);
        assert_eq!(names(&d), ["root", "B"]);
    }

    #[test]
    fn prune_blank_promotes_children_of_dropped_steps() {
        let mut d = Vec::new();
        add_child(&mut d, None, draft("", "s1"));
        let k1 = d[0].key;
        add_child(&mut d, Some(k1), draft("", "   ")); // blank
        let kb = d[1].key;
        add_child(&mut d, Some(kb), draft("", "s3"));
        let p = prune_blank(&d);
        assert_eq!(names(&p), ["s1", "s3"]);
        assert_eq!(parent_names(&p), [None, Some("s1".into())]);
    }

    fn guides(d: &[StepDraft]) -> Vec<String> {
        outline(d).into_iter().map(|r| r.guide).collect()
    }

    #[test]
    fn outline_keeps_chains_flat_and_indents_forks() {
        //  1. root                 indent 0
        //  |- A   (step 2)          indent 1, branch
        //    a   (step 3)          indent 1 (chain continuation)
        //  |- B   (step 2)          indent 1, branch, last
        let mut d = Vec::new();
        add_child(&mut d, None, draft("", "root"));
        let rk = d[0].key;
        add_child(&mut d, Some(rk), draft("A", "A"));
        let ak = d[1].key;
        add_child(&mut d, Some(ak), draft("", "a"));
        add_child(&mut d, Some(rk), draft("B", "B"));
        assert_eq!(guides(&d), ["", "├─ ", "│  ", "└─ "]);
        let o = outline(&d);
        assert_eq!(o.iter().map(|r| r.step_no).collect::<Vec<_>>(), [1, 2, 3, 2]);
        assert_eq!(o.iter().map(|r| r.branch).collect::<Vec<_>>(), [None, Some(false), None, Some(true)]);
    }

    #[test]
    fn outline_draws_root_alternatives_as_a_bracket_with_a_nested_fork() {
        // the shape from a real card: two top-level paths, and the first one forks again
        //  ┌─ 1. path A
        //  │  2. a2
        //  │  ├─ 3. a3-one
        //  │  └─ 3. a3-two
        //  └─ 1. path B
        //     2. b2
        let mut d = Vec::new();
        add_child(&mut d, None, draft("A", "a1"));
        let a1 = d[0].key;
        add_child(&mut d, Some(a1), draft("", "a2"));
        let a2 = d[1].key;
        add_child(&mut d, Some(a2), draft("one", "a3-one"));
        add_child(&mut d, Some(a2), draft("two", "a3-two"));
        add_child(&mut d, None, draft("B", "b1"));
        let b1 = d.last().unwrap().key;
        add_child(&mut d, Some(b1), draft("", "b2"));
        assert_eq!(names(&d), ["A", "a2", "one", "two", "B", "b2"]);
        assert_eq!(guides(&d), ["┌─ ", "│  ", "│  ├─ ", "│  └─ ", "└─ ", "   "]);
    }

    #[test]
    fn outline_uses_a_middle_connector_for_three_root_paths() {
        let mut d = Vec::new();
        for n in ["A", "B", "C"] {
            add_child(&mut d, None, draft(n, &n.to_lowercase()));
        }
        assert_eq!(guides(&d), ["┌─ ", "├─ ", "└─ "]);
    }

    #[test]
    fn outline_rails_end_after_the_last_sibling() {
        // a fork whose LAST branch continues: no rail should be drawn beside it
        let mut d = Vec::new();
        add_child(&mut d, None, draft("", "root"));
        let rk = d[0].key;
        add_child(&mut d, Some(rk), draft("A", "A"));
        add_child(&mut d, Some(rk), draft("B", "B"));
        let bk = d[2].key;
        add_child(&mut d, Some(bk), draft("", "b2"));
        assert_eq!(guides(&d), ["", "├─ ", "└─ ", "   "]);
    }

    #[test]
    fn outline_of_a_plain_chain_has_no_guides_and_no_branches() {
        let d = StepDraft::chain(&[
            ("".into(), "a".into(), None),
            ("".into(), "b".into(), None),
            ("".into(), "c".into(), None),
        ]);
        let o = outline(&d);
        assert!(o.iter().all(|r| r.guide.is_empty() && r.branch.is_none()));
        assert_eq!(o.iter().map(|r| r.step_no).collect::<Vec<_>>(), [1, 2, 3]);
        assert!(is_linear(&d));
    }
}
