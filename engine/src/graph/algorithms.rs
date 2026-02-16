//! Graph algorithms using petgraph.

use std::collections::{HashMap, HashSet, VecDeque};
use petgraph::graph::NodeIndex;
use petgraph::Direction;
use petgraph::visit::EdgeRef;

use crate::models::NodeId;
use super::view::GraphView;

/// Compute PageRank scores for all nodes in the graph.
///
/// # Arguments
/// * `graph` - The graph view
/// * `damping` - Damping factor (typically 0.85)
/// * `iterations` - Number of iterations to run
///
/// # Returns
/// HashMap mapping NodeId to PageRank score
pub fn pagerank(
    graph: &GraphView,
    damping: f64,
    iterations: u32,
) -> HashMap<NodeId, f64> {
    let node_count = graph.node_count();
    if node_count == 0 {
        return HashMap::new();
    }

    let initial_rank = 1.0 / node_count as f64;
    let damping_term = (1.0 - damping) / node_count as f64;

    // Initialize ranks
    let mut ranks: HashMap<NodeIndex, f64> = HashMap::new();
    for node_idx in graph.graph.node_indices() {
        ranks.insert(node_idx, initial_rank);
    }

    // Run PageRank iterations
    for _ in 0..iterations {
        let mut new_ranks = HashMap::new();

        for node_idx in graph.graph.node_indices() {
            let mut rank_sum = 0.0;

            // Sum contributions from incoming edges
            for edge in graph.graph.edges_directed(node_idx, Direction::Incoming) {
                let source_idx = edge.source();
                let source_rank = ranks.get(&source_idx).copied().unwrap_or(0.0);
                let out_degree = graph.graph.edges_directed(source_idx, Direction::Outgoing).count();
                
                if out_degree > 0 {
                    rank_sum += source_rank / out_degree as f64;
                }
            }

            new_ranks.insert(node_idx, damping_term + damping * rank_sum);
        }

        ranks = new_ranks;
    }

    // Convert NodeIndex to NodeId
    let mut result = HashMap::new();
    for (idx, rank) in ranks {
        if let Some(node_id) = graph.get_node_id(idx) {
            result.insert(node_id, rank);
        }
    }

    result
}

/// Find all strongly connected components in the graph.
///
/// # Returns
/// Vector of components, where each component is a vector of NodeIds
pub fn connected_components(graph: &GraphView) -> Vec<Vec<NodeId>> {
    use petgraph::algo::kosaraju_scc;

    let sccs = kosaraju_scc(&graph.graph);
    
    sccs.into_iter()
        .map(|component| {
            component
                .into_iter()
                .filter_map(|idx| graph.get_node_id(idx))
                .collect()
        })
        .collect()
}

/// Find the shortest path between two nodes.
///
/// # Returns
/// Some(path) if a path exists, None otherwise
pub fn shortest_path(
    graph: &GraphView,
    from: NodeId,
    to: NodeId,
) -> Option<Vec<NodeId>> {
    let from_idx = graph.get_index(from)?;
    let to_idx = graph.get_index(to)?;

    // Use BFS for unweighted shortest path
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    let mut parent = HashMap::new();

    queue.push_back(from_idx);
    visited.insert(from_idx);

    while let Some(current) = queue.pop_front() {
        if current == to_idx {
            // Reconstruct path
            let mut path = vec![to_idx];
            let mut node = to_idx;
            
            while let Some(&prev) = parent.get(&node) {
                path.push(prev);
                node = prev;
            }
            
            path.reverse();
            
            return Some(
                path.into_iter()
                    .filter_map(|idx| graph.get_node_id(idx))
                    .collect()
            );
        }

        for neighbor in graph.graph.neighbors(current) {
            if !visited.contains(&neighbor) {
                visited.insert(neighbor);
                parent.insert(neighbor, current);
                queue.push_back(neighbor);
            }
        }
    }

    None
}

/// Compute betweenness centrality for all nodes.
///
/// Betweenness centrality measures how often a node appears on shortest paths
/// between other nodes in the graph.
///
/// # Returns
/// HashMap mapping NodeId to betweenness centrality score
pub fn betweenness_centrality(graph: &GraphView) -> HashMap<NodeId, f64> {
    let mut centrality: HashMap<NodeIndex, f64> = HashMap::new();
    
    // Initialize all nodes with 0 centrality
    for node_idx in graph.graph.node_indices() {
        centrality.insert(node_idx, 0.0);
    }

    // For each node as source
    for source in graph.graph.node_indices() {
        // BFS to find all shortest paths from source
        let mut stack = Vec::new();
        let mut paths: HashMap<NodeIndex, Vec<Vec<NodeIndex>>> = HashMap::new();
        let mut dist: HashMap<NodeIndex, i32> = HashMap::new();
        let mut queue = VecDeque::new();

        for node_idx in graph.graph.node_indices() {
            paths.insert(node_idx, Vec::new());
            dist.insert(node_idx, -1);
        }
        
        paths.insert(source, vec![vec![source]]);
        dist.insert(source, 0);
        queue.push_back(source);

        while let Some(current) = queue.pop_front() {
            stack.push(current);
            let current_dist = dist[&current];

            for neighbor in graph.graph.neighbors(current) {
                // First time visiting this neighbor
                if dist[&neighbor] == -1 {
                    dist.insert(neighbor, current_dist + 1);
                    queue.push_back(neighbor);
                }

                // Shortest path to neighbor goes through current
                if dist[&neighbor] == current_dist + 1 {
                    let current_paths = paths[&current].clone();
                    for mut path in current_paths {
                        path.push(neighbor);
                        paths.get_mut(&neighbor).unwrap().push(path);
                    }
                }
            }
        }

        // Count how many shortest paths pass through each node
        for node_idx in graph.graph.node_indices() {
            if node_idx == source {
                continue;
            }

            let node_paths = &paths[&node_idx];
            if node_paths.is_empty() {
                continue;
            }

            let total_paths = node_paths.len() as f64;

            // Count paths through each intermediate node
            let mut intermediate_counts: HashMap<NodeIndex, usize> = HashMap::new();
            for path in node_paths {
                for &intermediate in path.iter().skip(1).take(path.len().saturating_sub(2)) {
                    *intermediate_counts.entry(intermediate).or_insert(0) += 1;
                }
            }

            // Add to centrality scores
            for (intermediate, count) in intermediate_counts {
                *centrality.get_mut(&intermediate).unwrap() += count as f64 / total_paths;
            }
        }
    }

    // Normalize by the number of node pairs
    let node_count = graph.node_count();
    if node_count > 2 {
        let normalization = ((node_count - 1) * (node_count - 2)) as f64;
        for score in centrality.values_mut() {
            *score /= normalization;
        }
    }

    // Convert to NodeId
    let mut result = HashMap::new();
    for (idx, score) in centrality {
        if let Some(node_id) = graph.get_node_id(idx) {
            result.insert(node_id, score);
        }
    }

    result
}

/// Count the number of distinct paths between two nodes (up to a maximum depth).
///
/// This is used for path diversity in confidence scoring.
pub fn count_distinct_paths(
    graph: &GraphView,
    from: NodeId,
    to: NodeId,
    max_depth: u32,
) -> usize {
    let Some(from_idx) = graph.get_index(from) else {
        return 0;
    };
    let Some(to_idx) = graph.get_index(to) else {
        return 0;
    };

    let mut path_count = 0;
    let mut stack = vec![(from_idx, HashSet::new(), 0)];

    while let Some((current, mut visited, depth)) = stack.pop() {
        if depth > max_depth {
            continue;
        }

        if current == to_idx {
            path_count += 1;
            continue;
        }

        visited.insert(current);

        for neighbor in graph.graph.neighbors(current) {
            if !visited.contains(&neighbor) {
                stack.push((neighbor, visited.clone(), depth + 1));
            }
        }
    }

    path_count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Triple;
    use crate::storage::{MemoryStore, TripleStore};

    #[tokio::test]
    async fn test_pagerank() {
        let store = MemoryStore::new();
        
        // Create a graph with cycles: A -> B -> C -> A (strongly connected)
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();
        let d = store.find_or_create_node("D").await.unwrap();
        
        // Create a cycle plus D pointing to B and C
        store.insert_triple(Triple::new(a.id, "points_to", b.id)).await.unwrap();
        store.insert_triple(Triple::new(b.id, "points_to", c.id)).await.unwrap();
        store.insert_triple(Triple::new(c.id, "points_to", a.id)).await.unwrap();
        store.insert_triple(Triple::new(d.id, "points_to", b.id)).await.unwrap();
        store.insert_triple(Triple::new(d.id, "points_to", c.id)).await.unwrap();
        
        let graph = GraphView::from_store(&store).await.unwrap();
        let ranks = pagerank(&graph, 0.85, 50);
        
        // B and C should have higher PageRank (more incoming edges)
        let rank_a = ranks.get(&a.id).unwrap();
        let rank_b = ranks.get(&b.id).unwrap();
        let rank_c = ranks.get(&c.id).unwrap();
        let rank_d = ranks.get(&d.id).unwrap();
        
        // B and C have 2 incoming edges each, A and D have 1 each
        assert!(rank_b > rank_a && rank_c > rank_a);
        assert!(rank_b > rank_d && rank_c > rank_d);
        
        // In strongly connected graphs, sum should be close to 1.0
        let sum: f64 = ranks.values().sum();
        assert!((sum - 1.0).abs() < 0.1, "Sum of PageRank scores was {}", sum);
    }

    #[tokio::test]
    async fn test_connected_components() {
        let store = MemoryStore::new();
        
        // Create two disconnected components: A->B and C->D
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();
        let d = store.find_or_create_node("D").await.unwrap();
        
        store.insert_triple(Triple::new(a.id, "links", b.id)).await.unwrap();
        store.insert_triple(Triple::new(c.id, "links", d.id)).await.unwrap();
        
        let graph = GraphView::from_store(&store).await.unwrap();
        let components = connected_components(&graph);
        
        // Should have at least 2 components
        assert!(components.len() >= 2);
    }

    #[tokio::test]
    async fn test_shortest_path() {
        let store = MemoryStore::new();
        
        // Create a path: A -> B -> C
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();
        
        store.insert_triple(Triple::new(a.id, "next", b.id)).await.unwrap();
        store.insert_triple(Triple::new(b.id, "next", c.id)).await.unwrap();
        
        let graph = GraphView::from_store(&store).await.unwrap();
        let path = shortest_path(&graph, a.id, c.id);
        
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path.len(), 3);
        assert_eq!(path[0], a.id);
        assert_eq!(path[1], b.id);
        assert_eq!(path[2], c.id);
    }

    #[tokio::test]
    async fn test_betweenness_centrality() {
        let store = MemoryStore::new();
        
        // Create a graph where B is a bridge: A -> B -> C, D -> B -> E
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();
        let d = store.find_or_create_node("D").await.unwrap();
        let e = store.find_or_create_node("E").await.unwrap();
        
        store.insert_triple(Triple::new(a.id, "to", b.id)).await.unwrap();
        store.insert_triple(Triple::new(b.id, "to", c.id)).await.unwrap();
        store.insert_triple(Triple::new(d.id, "to", b.id)).await.unwrap();
        store.insert_triple(Triple::new(b.id, "to", e.id)).await.unwrap();
        
        let graph = GraphView::from_store(&store).await.unwrap();
        let centrality = betweenness_centrality(&graph);
        
        // B should have the highest centrality (it's a bridge)
        let cent_b = centrality.get(&b.id).unwrap_or(&0.0);
        let cent_a = centrality.get(&a.id).unwrap_or(&0.0);
        let cent_c = centrality.get(&c.id).unwrap_or(&0.0);
        
        assert!(cent_b > cent_a);
        assert!(cent_b > cent_c);
    }
}
