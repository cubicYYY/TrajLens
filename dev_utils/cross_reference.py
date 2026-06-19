"""
Cross-reference G1 (Goal Tree) and G2 (Reasoning DAG) nodes via step ranges.

For each reasoning hypothesis, identifies:
- Which goal tree node it was proposed under
- Which goal tree node it was verified/falsified under (if different)

Usage:
    python dev_utils/cross_reference.py <goal-tree.igr.toml> <reasoning-dag.igr.toml>

Output: table showing hypothesis → goal node mapping.
"""

import sys
import tomllib
from pathlib import Path


def load_goal_tree(path: str):
    with open(path, "rb") as f:
        data = tomllib.load(f)
    nodes = data.get("nodes", [])
    edges = data.get("edges", [])
    # Find leaves (no sub edge from them)
    parents = set(e["source_id"] for e in edges if e["edge_type"] == "sub")
    return nodes, parents


def load_reasoning_dag(path: str):
    with open(path, "rb") as f:
        data = tomllib.load(f)
    return data.get("nodes", [])


def find_goal_node_at_step(step: int, goal_nodes: list, parents: set) -> str | None:
    """Find the most specific (deepest/narrowest) goal node covering a step."""
    candidates = []
    for n in goal_nodes:
        sr = n.get("step_range", [0, 0])
        if sr[0] <= step < sr[1]:
            # Prefer leaves over parents, narrower over wider
            is_leaf = n["node_id"] not in parents
            width = sr[1] - sr[0]
            candidates.append((is_leaf, -width, n))

    if not candidates:
        return None
    # Sort: leaves first, then narrowest range
    candidates.sort(key=lambda x: (not x[0], x[1]))
    return candidates[0][2]["node_id"]


def cross_reference(goal_tree_path: str, reasoning_dag_path: str):
    goal_nodes, parents = load_goal_tree(goal_tree_path)
    reasoning_nodes = load_reasoning_dag(reasoning_dag_path)

    goal_map = {n["node_id"]: n for n in goal_nodes}

    print(f"Goal Tree: {len(goal_nodes)} nodes")
    print(f"Reasoning DAG: {len(reasoning_nodes)} nodes")
    print()
    print(
        f"{'Hyp ID':<6} {'Status':<12} {'Proposed':<10} {'Resolved':<10} "
        f"{'Goal@Proposed':<15} {'Goal@Resolved':<15} {'Content'}"
    )
    print("-" * 120)

    for rn in reasoning_nodes:
        rid = rn.get("node_id", "?")
        status = rn.get("status", "") or "fact"
        sr = rn.get("step_range", [0, 0])
        content = rn.get("content", "")[:45]

        proposed_step = sr[0] if isinstance(sr, list) and len(sr) >= 1 else 0
        resolved_step = (
            sr[1] if isinstance(sr, list) and len(sr) >= 2 else proposed_step
        )

        goal_at_proposed = find_goal_node_at_step(proposed_step, goal_nodes, parents)
        goal_at_resolved = find_goal_node_at_step(resolved_step, goal_nodes, parents)

        # Mark if hypothesis spans multiple goals (proposed in one, resolved in another)
        cross_goal = "⚡" if goal_at_proposed != goal_at_resolved else ""

        print(
            f"{rid:<6} {status:<12} step {proposed_step:<5} step {resolved_step:<5} "
            f"{goal_at_proposed or '?':<15} {goal_at_resolved or '?':<15} "
            f"{cross_goal}{content}"
        )

    # Summary
    print()
    cross_count = sum(
        1
        for rn in reasoning_nodes
        if (sr := rn.get("step_range", [0, 0]))
        and find_goal_node_at_step(sr[0], goal_nodes, parents)
        != find_goal_node_at_step(sr[1] if len(sr) > 1 else sr[0], goal_nodes, parents)
    )
    print(
        f"Hypotheses spanning multiple goals: {cross_count}/{len(reasoning_nodes)} "
        f"(marked with ⚡)"
    )


if __name__ == "__main__":
    if len(sys.argv) < 3:
        print(
            "Usage: python cross_reference.py <goal-tree.igr.toml> <reasoning-dag.igr.toml>"
        )
        sys.exit(1)
    cross_reference(sys.argv[1], sys.argv[2])
