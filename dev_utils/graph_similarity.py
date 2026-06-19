"""
Cross-graph node similarity: find common components across different goal trees.

Uses sentence embeddings to encode each node's label+result, then reports
similar node pairs across different graphs (cosine similarity > threshold).

Usage:
    python dev_utils/graph_similarity.py tree1.igr.toml tree2.igr.toml [tree3.igr.toml ...]
    python dev_utils/graph_similarity.py --dir output/variance_test/run_*/exploit_rce_a3/main/goal-tree.igr.toml
    python dev_utils/graph_similarity.py --threshold 0.8 tree1.igr.toml tree2.igr.toml

Requirements:
    pip install sentence-transformers numpy
"""

import argparse
import glob
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path


@dataclass
class GraphNode:
    """A node extracted from a goal tree IGR for similarity comparison."""

    graph_id: str  # which graph this came from
    node_id: str
    label: str
    result: str
    goal_type: str
    status: str
    step_range: tuple[int, int]
    text: str  # combined text for embedding


def load_nodes(igr_path: str) -> list[GraphNode]:
    """Load all non-ROOT nodes from a goal tree IGR file."""
    with open(igr_path, "rb") as f:
        data = tomllib.load(f)

    if data.get("graph_type") != "goal_transition_tree":
        return []

    # Use enough path components to distinguish graphs (e.g., "run_1/exploit_rce_a3")
    parts = Path(igr_path).parts
    graph_id = "/".join(parts[-4:-1]) if len(parts) >= 4 else Path(igr_path).stem
    nodes = []
    for n in data.get("nodes", []):
        nid = n.get("node_id", "")
        if nid == "ROOT" or nid == data.get("root_id", "ROOT"):
            continue
        label = n.get("label", "")
        result = n.get("result", "")
        text = f"{label}. {result}".strip(". ")
        nodes.append(
            GraphNode(
                graph_id=graph_id,
                node_id=nid,
                label=label,
                result=result,
                goal_type=n.get("goal_type", ""),
                status=n.get("status", ""),
                step_range=tuple(n.get("step_range", [0, 0])),
                text=text,
            )
        )
    return nodes


def compute_similarities(
    all_nodes: list[GraphNode], threshold: float = 0.75
) -> list[tuple[GraphNode, GraphNode, float]]:
    """Compute pairwise cosine similarity using sentence embeddings.

    Uses AWS Bedrock Titan Embeddings (no local model download needed).
    Falls back to sentence-transformers if Bedrock unavailable.
    """
    import numpy as np

    texts = [n.text for n in all_nodes]

    # Try Bedrock Titan Embeddings first (no download, uses existing AWS creds)
    try:
        embeddings = _embed_with_bedrock(texts)
        print(f"  Using Bedrock Titan Embeddings ({len(texts)} texts)", file=sys.stderr)
    except Exception as e:
        # Fall back to sentence-transformers (requires local model)
        try:
            from sentence_transformers import SentenceTransformer

            print(
                f"  Bedrock failed ({e}), using sentence-transformers", file=sys.stderr
            )
            model = SentenceTransformer("all-MiniLM-L6-v2")
            embeddings = model.encode(
                texts, normalize_embeddings=True, show_progress_bar=False
            )
        except ImportError:
            print(
                "ERROR: Neither Bedrock nor sentence-transformers available.",
                file=sys.stderr,
            )
            print(
                "  Set AWS credentials for Bedrock, or: pip install sentence-transformers",
                file=sys.stderr,
            )
            sys.exit(1)

    # Normalize embeddings for cosine similarity via dot product
    norms = np.linalg.norm(embeddings, axis=1, keepdims=True)
    norms[norms == 0] = 1
    embeddings = embeddings / norms
    sim_matrix = np.dot(embeddings, embeddings.T)

    return _extract_pairs(all_nodes, sim_matrix, threshold)


def _embed_with_bedrock(texts: list[str]):
    """Embed texts using AWS Bedrock Titan Embeddings V2."""
    import json
    import os
    import numpy as np

    try:
        import boto3
    except ImportError:
        raise RuntimeError("boto3 not installed")

    region = os.environ.get(
        "AWS_DEFAULT_REGION_NAME", os.environ.get("AWS_REGION", "us-east-1")
    )
    client = boto3.client("bedrock-runtime", region_name=region)
    model_id = "amazon.titan-embed-text-v2:0"

    embeddings = []
    for text in texts:
        body = json.dumps({"inputText": text[:2000]})  # Titan limit
        response = client.invoke_model(modelId=model_id, body=body)
        result = json.loads(response["body"].read())
        embeddings.append(result["embedding"])

    return np.array(embeddings)

    return _extract_pairs(all_nodes, sim_matrix, threshold)


def _extract_pairs(all_nodes, sim_matrix, threshold):
    """Extract cross-graph pairs above threshold from similarity matrix."""
    pairs = []
    for i in range(len(all_nodes)):
        for j in range(i + 1, len(all_nodes)):
            if all_nodes[i].graph_id == all_nodes[j].graph_id:
                continue
            score = float(sim_matrix[i, j])
            if score >= threshold:
                pairs.append((all_nodes[i], all_nodes[j], score))
    pairs.sort(key=lambda x: -x[2])
    return pairs


def cluster_similar_nodes(
    pairs: list[tuple[GraphNode, GraphNode, float]],
) -> list[list[tuple[GraphNode, float]]]:
    """Group similar nodes into clusters (greedy union-find)."""
    clusters: list[list[tuple[GraphNode, float]]] = []
    assigned: set[tuple[str, str]] = set()  # (graph_id, node_id)

    for a, b, score in pairs:
        key_a = (a.graph_id, a.node_id)
        key_b = (b.graph_id, b.node_id)

        # Find existing cluster containing a or b
        found = None
        for cluster in clusters:
            cluster_keys = {(n.graph_id, n.node_id) for n, _ in cluster}
            if key_a in cluster_keys or key_b in cluster_keys:
                found = cluster
                break

        if found is not None:
            if key_a not in assigned:
                found.append((a, score))
                assigned.add(key_a)
            if key_b not in assigned:
                found.append((b, score))
                assigned.add(key_b)
        else:
            clusters.append([(a, 1.0), (b, score)])
            assigned.add(key_a)
            assigned.add(key_b)

    return clusters


def main():
    parser = argparse.ArgumentParser(
        description="Find similar nodes across different goal trees"
    )
    parser.add_argument("inputs", nargs="+", help="IGR TOML files (or glob patterns)")
    parser.add_argument(
        "--threshold",
        type=float,
        default=0.75,
        help="Cosine similarity threshold (default: 0.75)",
    )
    parser.add_argument(
        "--top",
        type=int,
        default=20,
        help="Show top N similar pairs (default: 20)",
    )
    parser.add_argument(
        "--clusters", action="store_true", help="Group similar nodes into clusters"
    )
    args = parser.parse_args()

    # Expand globs
    files = []
    for pattern in args.inputs:
        expanded = glob.glob(pattern)
        if expanded:
            files.extend(expanded)
        else:
            files.append(pattern)

    if len(files) < 2:
        print("ERROR: Need at least 2 graph files to compare", file=sys.stderr)
        sys.exit(1)

    # Load nodes from all graphs
    all_nodes = []
    for f in files:
        nodes = load_nodes(f)
        if nodes:
            print(f"  Loaded {len(nodes)} nodes from {f}", file=sys.stderr)
            all_nodes.extend(nodes)

    if not all_nodes:
        print("ERROR: No nodes found in any input file", file=sys.stderr)
        sys.exit(1)

    print(
        f"\nComparing {len(all_nodes)} nodes across {len(files)} graphs...",
        file=sys.stderr,
    )

    pairs = compute_similarities(all_nodes, threshold=args.threshold)
    print(f"Found {len(pairs)} similar pairs (threshold={args.threshold})\n")

    if args.clusters:
        clusters = cluster_similar_nodes(pairs)
        print(f"=== {len(clusters)} CLUSTERS OF SIMILAR NODES ===\n")
        for ci, cluster in enumerate(clusters[:15]):
            steps = f"{min(n.step_range[0] for n,_ in cluster)}-{max(n.step_range[1] for n,_ in cluster)}"
            print(f"Cluster {ci+1} (steps ~{steps}):")
            for node, score in sorted(cluster, key=lambda x: x[0].graph_id):
                print(
                    f"  [{node.graph_id}] {node.node_id:6s} ({node.status:7s}) {node.label[:50]}"
                )
            print()
    else:
        print(f"=== TOP {min(args.top, len(pairs))} SIMILAR PAIRS ===\n")
        for a, b, score in pairs[: args.top]:
            print(f"  {score:.3f}  [{a.graph_id}] {a.node_id}: {a.label[:40]}")
            print(f"         [{b.graph_id}] {b.node_id}: {b.label[:40]}")
            print()


if __name__ == "__main__":
    main()
