"""Tests for the TrajLens Python wrapper."""

import json
import pytest
import trajlens


def test_parse_log_auto_detection():
    """Test log parsing with auto format detection."""
    # Minimal Claude Code log (JSONL format)
    log_content = """{"type":"session_start","timestamp":"2024-01-01T00:00:00Z"}
{"type":"tool_use","name":"Read","args":"{\\"file_path\\":\\"/test.rs\\"}"}
{"type":"tool_result","output":"fn main() {}"}
{"type":"session_end","outcome":"success"}
"""

    trajectory_json = trajlens.parse_log("auto", log_content)
    trajectory = json.loads(trajectory_json)

    assert "turns" in trajectory
    assert "outcome" in trajectory
    assert isinstance(trajectory["turns"], list)


def test_parse_log_explicit_format():
    """Test log parsing with explicit format."""
    log_content = """{"type":"session_start","timestamp":"2024-01-01T00:00:00Z"}
{"type":"session_end","outcome":"success"}
"""

    trajectory_json = trajlens.parse_log("claude-code", log_content)
    trajectory = json.loads(trajectory_json)

    assert trajectory["outcome"] in ["success", "failed"]


def test_parse_log_invalid_format():
    """Test that invalid format raises error."""
    with pytest.raises(ValueError, match="Unknown format"):
        trajlens.parse_log("invalid-format", "content")


def test_build_activity_graph():
    """Test Activity Graph building."""
    # Create a minimal trajectory
    trajectory = {
        "turns": [
            {
                "turn_id": "0",
                "items": [
                    {
                        "item_id": "0",
                        "category": "read",
                        "detail": "test.rs",
                        "primary_object": "/test.rs",
                        "cost": {
                            "input_tokens": 10,
                            "output_tokens": 5,
                            "dollar_cost": 0.0001,
                        },
                    }
                ],
            }
        ],
        "outcome": "success",
        "total_cost": {"input_tokens": 10, "output_tokens": 5, "dollar_cost": 0.0001},
    }
    trajectory_json = json.dumps(trajectory)

    graph_json = trajlens.build_activity_graph(trajectory_json)
    graph = json.loads(graph_json)

    assert "ActivityGraph" in str(graph) or "nodes" in graph
    assert "edges" in graph


def test_build_cost_map():
    """Test Cost Map building."""
    trajectory = {
        "turns": [
            {
                "turn_id": "0",
                "items": [
                    {
                        "item_id": "0",
                        "category": "read",
                        "detail": "test.rs",
                        "primary_object": "/test.rs",
                        "cost": {
                            "input_tokens": 10,
                            "output_tokens": 5,
                            "dollar_cost": 0.0001,
                        },
                    }
                ],
            }
        ],
        "outcome": "success",
        "total_cost": {"input_tokens": 10, "output_tokens": 5, "dollar_cost": 0.0001},
    }
    trajectory_json = json.dumps(trajectory)

    cost_map_json = trajlens.build_cost_map(trajectory_json, None)
    cost_map = json.loads(cost_map_json)

    assert "CostMap" in str(cost_map) or "root" in cost_map


def test_igr_roundtrip():
    """Test IGR serialization and deserialization."""
    # Build a graph
    trajectory = {
        "turns": [],
        "outcome": "success",
        "total_cost": {"input_tokens": 0, "output_tokens": 0, "dollar_cost": 0.0},
    }
    trajectory_json = json.dumps(trajectory)

    graph_json = trajlens.build_activity_graph(trajectory_json)

    # Convert to IGR TOML
    igr_toml = trajlens.to_igr_toml(graph_json)
    assert "graph_type" in igr_toml
    assert "[nodes]" in igr_toml or "[[nodes]]" in igr_toml

    # Convert back to JSON
    graph_json_back = trajlens.from_igr_toml(igr_toml)
    graph_back = json.loads(graph_json_back)

    assert "ActivityGraph" in str(graph_back) or "nodes" in graph_back


def test_render_svg():
    """Test SVG rendering."""
    trajectory = {
        "turns": [],
        "outcome": "success",
        "total_cost": {"input_tokens": 0, "output_tokens": 0, "dollar_cost": 0.0},
    }
    trajectory_json = json.dumps(trajectory)

    graph_json = trajlens.build_activity_graph(trajectory_json)
    svg = trajlens.render_svg(graph_json)

    assert svg.startswith("<?xml") or svg.startswith("<svg")
    assert "</svg>" in svg


def test_invalid_json_handling():
    """Test that invalid JSON raises appropriate errors."""
    with pytest.raises(ValueError, match="Invalid"):
        trajlens.build_activity_graph("not valid json")

    with pytest.raises(ValueError, match="Invalid"):
        trajlens.to_igr_toml("not valid json")

    with pytest.raises(ValueError, match="Invalid"):
        trajlens.from_igr_toml("not valid toml [[[")
