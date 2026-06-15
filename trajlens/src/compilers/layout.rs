/// Sugiyama hierarchical graph layout algorithm.
///
/// Implements the same layered layout as grandalf (Python):
/// 1. Layer assignment (longest-path from sources)
/// 2. Crossing reduction (barycenter heuristic)
/// 3. Coordinate assignment (simple averaging)
///
/// Handles disconnected components by laying out each independently
/// and stacking them horizontally.
use std::collections::{HashMap, HashSet, VecDeque};

/// Input to the layout engine: a node with an ID and dimensions.
#[derive(Debug, Clone)]
pub struct LayoutNode {
    pub id: String,
    pub width: f64,
    pub height: f64,
}

/// Input edge (directed, source → target).
#[derive(Debug, Clone)]
pub struct LayoutEdge {
    pub source: String,
    pub target: String,
}

/// Output: positioned node with center coordinates.
#[derive(Debug, Clone)]
pub struct PositionedNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Layout configuration.
pub struct LayoutConfig {
    pub x_spacing: f64,
    pub y_spacing: f64,
}

/// Run Sugiyama layout on a graph (possibly disconnected).
/// Returns positioned nodes with center coordinates (top-left at 0,0).
pub fn sugiyama_layout(
    nodes: &[LayoutNode],
    edges: &[LayoutEdge],
    config: &LayoutConfig,
) -> Vec<PositionedNode> {
    if nodes.is_empty() {
        return Vec::new();
    }

    let node_ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let valid_edges: Vec<&LayoutEdge> = edges
        .iter()
        .filter(|e| node_ids.contains(e.source.as_str()) && node_ids.contains(e.target.as_str()))
        .collect();

    // Find connected components
    let components = find_components(nodes, &valid_edges);

    let mut all_positioned: Vec<PositionedNode> = Vec::new();
    let mut x_offset: f64 = 0.0;
    let component_spacing = 60.0;

    for component_ids in &components {
        let comp_nodes: Vec<&LayoutNode> = nodes
            .iter()
            .filter(|n| component_ids.contains(n.id.as_str()))
            .collect();
        let comp_edges: Vec<&LayoutEdge> = valid_edges
            .iter()
            .filter(|e| {
                component_ids.contains(e.source.as_str())
                    && component_ids.contains(e.target.as_str())
            })
            .copied()
            .collect();

        let positioned = layout_component(&comp_nodes, &comp_edges, config);

        if positioned.is_empty() {
            continue;
        }

        // Find bounding box of this component
        let min_x = positioned
            .iter()
            .map(|p| p.x - p.width / 2.0)
            .fold(f64::INFINITY, f64::min);
        let max_x = positioned
            .iter()
            .map(|p| p.x + p.width / 2.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_y = positioned
            .iter()
            .map(|p| p.y - p.height / 2.0)
            .fold(f64::INFINITY, f64::min);

        // Shift to x_offset, normalize y to 0
        for mut p in positioned {
            p.x = p.x - min_x + x_offset;
            p.y = p.y - min_y;
            all_positioned.push(p);
        }

        let comp_width = max_x - min_x;
        x_offset += comp_width + component_spacing;
    }

    all_positioned
}

/// Layout a single connected component.
fn layout_component(
    nodes: &[&LayoutNode],
    edges: &[&LayoutEdge],
    config: &LayoutConfig,
) -> Vec<PositionedNode> {
    if nodes.is_empty() {
        return Vec::new();
    }
    if nodes.len() == 1 {
        return vec![PositionedNode {
            id: nodes[0].id.clone(),
            x: nodes[0].width / 2.0,
            y: nodes[0].height / 2.0,
            width: nodes[0].width,
            height: nodes[0].height,
        }];
    }

    let node_map: HashMap<&str, &LayoutNode> = nodes.iter().map(|n| (n.id.as_str(), *n)).collect();

    // Phase 1: Layer assignment (longest path from sources)
    let layers = assign_layers(nodes, edges);

    // Phase 2: Ordering within layers (barycenter crossing reduction)
    let ordered_layers = reduce_crossings(&layers, edges);

    // Phase 3: Coordinate assignment
    assign_coordinates(&ordered_layers, &node_map, config)
}

/// Assign each node to a layer using longest-path algorithm.
/// Sources (no incoming edges) get layer 0. Each other node gets
/// max(predecessor layers) + 1.
fn assign_layers<'a>(nodes: &[&'a LayoutNode], edges: &[&LayoutEdge]) -> Vec<Vec<&'a str>> {
    let node_ids: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();

    // Build adjacency
    let mut incoming: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
    for &id in &node_ids {
        incoming.insert(id, Vec::new());
        outgoing.insert(id, Vec::new());
    }
    for edge in edges {
        incoming
            .get_mut(edge.target.as_str())
            .map(|v| v.push(edge.source.as_str()));
        outgoing
            .get_mut(edge.source.as_str())
            .map(|v| v.push(edge.target.as_str()));
    }

    // Layered topological sort with cycle tolerance.
    //
    // Standard Kahn's algorithm fails when the graph has cycles or back-edges
    // (every node ends up with in-degree > 0, queue stays empty, all nodes
    // collapse into layer 0). The Reasoning Artifact DAG legitimately has
    // back-edges (a `contradicts` edge from a verified ground-truth node back
    // to its falsified hypotheses), so we must tolerate them.
    //
    // Strategy:
    //   1. Process all in-degree-0 nodes via Kahn's algorithm (handles the
    //      acyclic skeleton correctly).
    //   2. If the queue empties but unprocessed nodes remain (cycle/back-edges),
    //      seed the queue with the unprocessed node having the smallest
    //      remaining in-degree — effectively "ignoring" one back-edge to break
    //      the cycle. Repeat until all nodes are placed.
    //   3. Visited tracking prevents infinite loops on the same cycle.
    let mut in_degree: HashMap<&str, usize> = node_ids
        .iter()
        .map(|&id| (id, incoming[id].len()))
        .collect();
    let mut layer_of: HashMap<&str, usize> = HashMap::new();
    let mut queue: VecDeque<&str> = VecDeque::new();
    let mut visited: std::collections::HashSet<&str> = std::collections::HashSet::new();

    // Initial seed: all in-degree-0 nodes
    for &id in &node_ids {
        if in_degree[id] == 0 {
            queue.push_back(id);
            layer_of.insert(id, 0);
        }
    }

    loop {
        // Standard Kahn's algorithm step
        while let Some(node) = queue.pop_front() {
            if !visited.insert(node) {
                continue;
            }
            let node_layer = layer_of[node];
            for &target in outgoing.get(node).unwrap_or(&Vec::new()) {
                let target_layer = layer_of
                    .get(target)
                    .copied()
                    .unwrap_or(0)
                    .max(node_layer + 1);
                layer_of.insert(target, target_layer);
                if let Some(deg) = in_degree.get_mut(target) {
                    if *deg > 0 {
                        *deg -= 1;
                    }
                    if *deg == 0 && !visited.contains(target) {
                        queue.push_back(target);
                    }
                }
            }
        }

        // If unprocessed nodes remain, we have cycles/back-edges.
        // Break the cycle: pick the unprocessed node with the smallest
        // remaining in-degree, give it the layer it's already accumulated
        // (or 0 if none), and re-seed the queue.
        let unprocessed: Vec<&str> = node_ids
            .iter()
            .copied()
            .filter(|id| !visited.contains(id))
            .collect();

        if unprocessed.is_empty() {
            break;
        }

        let seed = unprocessed
            .iter()
            .min_by_key(|id| in_degree.get(*id).copied().unwrap_or(0))
            .copied()
            .unwrap();

        layer_of.entry(seed).or_insert(0);
        // Force in-degree to 0 so Kahn's accepts it
        in_degree.insert(seed, 0);
        queue.push_back(seed);
    }

    // Safety: any node still missing a layer (shouldn't happen) gets layer 0
    for &id in &node_ids {
        layer_of.entry(id).or_insert(0);
    }

    // Group by layer
    let max_layer = layer_of.values().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<&str>> = vec![Vec::new(); max_layer + 1];
    for &id in &node_ids {
        let layer = layer_of[id];
        layers[layer].push(id);
    }

    layers
}

/// Barycenter heuristic for crossing reduction.
/// Iterates top-down and bottom-up, reordering nodes within each layer
/// by the average position of their neighbors in the adjacent layer.
fn reduce_crossings<'a>(layers: &[Vec<&'a str>], edges: &[&LayoutEdge]) -> Vec<Vec<&'a str>> {
    let mut ordered = layers.to_vec();

    // Build neighbor maps
    let mut down_neighbors: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut up_neighbors: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in edges {
        down_neighbors
            .entry(edge.source.as_str())
            .or_default()
            .push(edge.target.as_str());
        up_neighbors
            .entry(edge.target.as_str())
            .or_default()
            .push(edge.source.as_str());
    }

    // Run 4 iterations of sweeping
    for _ in 0..4 {
        // Top-down sweep
        for layer_idx in 1..ordered.len() {
            let prev_layer = &ordered[layer_idx - 1];
            let pos_map: HashMap<&str, usize> = prev_layer
                .iter()
                .enumerate()
                .map(|(i, &id)| (id, i))
                .collect();

            let mut barycenters: Vec<(&str, f64)> = ordered[layer_idx]
                .iter()
                .map(|&node| {
                    let neighbors = up_neighbors.get(node).cloned().unwrap_or_default();
                    if neighbors.is_empty() {
                        (node, f64::MAX)
                    } else {
                        let sum: f64 = neighbors
                            .iter()
                            .filter_map(|n| pos_map.get(n).map(|&p| p as f64))
                            .sum();
                        let count = neighbors
                            .iter()
                            .filter(|n| pos_map.contains_key(*n))
                            .count();
                        let bc = if count > 0 {
                            sum / count as f64
                        } else {
                            f64::MAX
                        };
                        (node, bc)
                    }
                })
                .collect();

            barycenters.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            ordered[layer_idx] = barycenters.into_iter().map(|(id, _)| id).collect();
        }

        // Bottom-up sweep
        for layer_idx in (0..ordered.len().saturating_sub(1)).rev() {
            let next_layer = &ordered[layer_idx + 1];
            let pos_map: HashMap<&str, usize> = next_layer
                .iter()
                .enumerate()
                .map(|(i, &id)| (id, i))
                .collect();

            let mut barycenters: Vec<(&str, f64)> = ordered[layer_idx]
                .iter()
                .map(|&node| {
                    let neighbors = down_neighbors.get(node).cloned().unwrap_or_default();
                    if neighbors.is_empty() {
                        (node, f64::MAX)
                    } else {
                        let sum: f64 = neighbors
                            .iter()
                            .filter_map(|n| pos_map.get(n).map(|&p| p as f64))
                            .sum();
                        let count = neighbors
                            .iter()
                            .filter(|n| pos_map.contains_key(*n))
                            .count();
                        let bc = if count > 0 {
                            sum / count as f64
                        } else {
                            f64::MAX
                        };
                        (node, bc)
                    }
                })
                .collect();

            barycenters.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            ordered[layer_idx] = barycenters.into_iter().map(|(id, _)| id).collect();
        }
    }

    ordered
}

/// Assign x,y coordinates to nodes based on their layer and position within layer.
fn assign_coordinates(
    layers: &[Vec<&str>],
    node_map: &HashMap<&str, &LayoutNode>,
    config: &LayoutConfig,
) -> Vec<PositionedNode> {
    let mut result = Vec::new();

    let mut y_cursor: f64 = 0.0;

    for layer in layers {
        if layer.is_empty() {
            y_cursor += config.y_spacing;
            continue;
        }

        // Find max height in this layer
        let max_height: f64 = layer
            .iter()
            .filter_map(|id| node_map.get(id).map(|n| n.height))
            .fold(0.0, f64::max);

        // Center of this layer's y band
        let layer_cy = y_cursor + max_height / 2.0;

        // Compute total width of this layer
        let total_width: f64 = layer
            .iter()
            .filter_map(|id| node_map.get(id).map(|n| n.width))
            .sum::<f64>()
            + (layer.len() as f64 - 1.0) * config.x_spacing;

        // Position nodes centered horizontally
        let mut x_cursor = -total_width / 2.0;

        for &node_id in layer {
            if let Some(&node) = node_map.get(node_id) {
                let cx = x_cursor + node.width / 2.0;
                result.push(PositionedNode {
                    id: node_id.to_string(),
                    x: cx,
                    y: layer_cy,
                    width: node.width,
                    height: node.height,
                });
                x_cursor += node.width + config.x_spacing;
            }
        }

        y_cursor += max_height + config.y_spacing;
    }

    result
}

/// Find connected components using BFS on undirected edges.
fn find_components<'a>(nodes: &'a [LayoutNode], edges: &[&'a LayoutEdge]) -> Vec<HashSet<&'a str>> {
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    for node in nodes {
        adjacency.entry(node.id.as_str()).or_default();
    }
    for edge in edges {
        adjacency
            .entry(edge.source.as_str())
            .or_default()
            .push(edge.target.as_str());
        adjacency
            .entry(edge.target.as_str())
            .or_default()
            .push(edge.source.as_str());
    }

    let mut visited: HashSet<&str> = HashSet::new();
    let mut components: Vec<HashSet<&str>> = Vec::new();

    for node in nodes {
        let id = node.id.as_str();
        if visited.contains(id) {
            continue;
        }

        let mut component: HashSet<&str> = HashSet::new();
        let mut queue: VecDeque<&str> = VecDeque::new();
        queue.push_back(id);
        visited.insert(id);

        while let Some(current) = queue.pop_front() {
            component.insert(current);
            if let Some(neighbors) = adjacency.get(current) {
                for &neighbor in neighbors {
                    if !visited.contains(neighbor) {
                        visited.insert(neighbor);
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        components.push(component);
    }

    components
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_node() {
        let nodes = vec![LayoutNode {
            id: "a".into(),
            width: 100.0,
            height: 50.0,
        }];
        let result = sugiyama_layout(
            &nodes,
            &[],
            &LayoutConfig {
                x_spacing: 40.0,
                y_spacing: 60.0,
            },
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "a");
    }

    #[test]
    fn test_linear_chain() {
        let nodes = vec![
            LayoutNode {
                id: "a".into(),
                width: 100.0,
                height: 40.0,
            },
            LayoutNode {
                id: "b".into(),
                width: 100.0,
                height: 40.0,
            },
            LayoutNode {
                id: "c".into(),
                width: 100.0,
                height: 40.0,
            },
        ];
        let edges = vec![
            LayoutEdge {
                source: "a".into(),
                target: "b".into(),
            },
            LayoutEdge {
                source: "b".into(),
                target: "c".into(),
            },
        ];
        let config = LayoutConfig {
            x_spacing: 40.0,
            y_spacing: 60.0,
        };
        let result = sugiyama_layout(&nodes, &edges, &config);
        assert_eq!(result.len(), 3);

        // Should be vertically stacked: a on top, c on bottom
        let pos: HashMap<&str, &PositionedNode> =
            result.iter().map(|p| (p.id.as_str(), p)).collect();
        assert!(pos["a"].y < pos["b"].y);
        assert!(pos["b"].y < pos["c"].y);
    }

    #[test]
    fn test_disconnected_components() {
        let nodes = vec![
            LayoutNode {
                id: "a".into(),
                width: 80.0,
                height: 40.0,
            },
            LayoutNode {
                id: "b".into(),
                width: 80.0,
                height: 40.0,
            },
            LayoutNode {
                id: "x".into(),
                width: 80.0,
                height: 40.0,
            },
            LayoutNode {
                id: "y".into(),
                width: 80.0,
                height: 40.0,
            },
        ];
        let edges = vec![
            LayoutEdge {
                source: "a".into(),
                target: "b".into(),
            },
            LayoutEdge {
                source: "x".into(),
                target: "y".into(),
            },
        ];
        let config = LayoutConfig {
            x_spacing: 40.0,
            y_spacing: 60.0,
        };
        let result = sugiyama_layout(&nodes, &edges, &config);
        assert_eq!(result.len(), 4);

        // Components should be separated horizontally
        let pos: HashMap<&str, &PositionedNode> =
            result.iter().map(|p| (p.id.as_str(), p)).collect();
        let comp1_max_x = pos["a"].x.max(pos["b"].x) + 40.0;
        let comp2_min_x = pos["x"].x.min(pos["y"].x) - 40.0;
        assert!(
            comp2_min_x > comp1_max_x - 1.0,
            "Components should not overlap"
        );
    }
}
