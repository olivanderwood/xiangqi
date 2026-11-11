//! Book tree navigation helpers (filled in by M4/T4.1).

use crate::schema::BookNode;

/// A path through the tree: indices of the chosen child at each ply.
pub type TreePath = Vec<usize>;

/// Find a node by path.
#[must_use]
pub fn node_at<'a>(tree: &'a [BookNode], path: &TreePath) -> Option<&'a BookNode> {
    let mut level = tree;
    let mut node: Option<&BookNode> = None;
    for &i in path {
        node = level.get(i);
        level = &node?.next;
    }
    node
}
