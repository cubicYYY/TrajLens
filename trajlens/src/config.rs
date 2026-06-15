/// Configuration loading and management.
///
/// Loads config.toml from project root or ~/.config/trajlens/config.toml.
/// Falls back to hardcoded defaults if file not found.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;

static GLOBAL_CONFIG: OnceLock<Config> = OnceLock::new();

/// Get the global configuration instance.
///
/// Loads config.toml on first call, then caches the result.
/// Falls back to defaults if config file not found.
pub fn get_config() -> &'static Config {
    GLOBAL_CONFIG.get_or_init(|| Config::load().unwrap_or_else(|_| Config::default()))
}

/// Top-level configuration structure matching config.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub llm: LLMConfig,
    pub cost: CostConfig,
    pub rendering: RenderingConfig,
    pub parsing: ParsingConfig,
    pub cli: CliConfig,
    pub graph: GraphConfig,
    pub igr: IgrConfig,
    pub web: WebConfig,
    pub logging: LoggingConfig,
    pub performance: PerformanceConfig,
    pub validation: ValidationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMConfig {
    pub anthropic: AnthropicConfig,
    pub bedrock: BedrockConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    pub api_url: String,
    pub api_version: String,
    pub default_model: String,
    pub max_tokens: u32,
    pub temperature: f64,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BedrockConfig {
    pub default_region: String,
    pub haiku_model_id: String,
    pub sonnet_model_id: String,
    pub opus_model_id: String,
    pub max_tokens: u32,
    pub temperature: f64,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostConfig {
    pub models: ModelsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsConfig {
    #[serde(rename = "claude-3-5-haiku-20241022")]
    pub haiku: ModelPricing,
    #[serde(rename = "claude-3-5-sonnet-20241022")]
    pub sonnet: ModelPricing,
    #[serde(rename = "claude-opus-4-20250514")]
    pub opus: ModelPricing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub input_per_million: f64,
    pub output_per_million: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderingConfig {
    pub svg: SvgConfig,
    pub layout: LayoutConfig,
    pub cost_map: CostMapConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SvgConfig {
    pub margin: f64,
    pub node_width: f64,
    pub node_height_base: f64,
    pub node_height_per_operation: f64,
    pub font_size_label: f64,
    pub font_size_detail: f64,
    pub font_family: String,
    pub text_wrap_max_chars: u32,
    pub stroke_width: f64,
    pub edge_stroke_width: f64,
    pub arrow_head_size: f64,
    pub colors: ColorConfig,
    pub goal_tree: GoalTreeSvgConfig,
    pub reasoning_dag: ReasoningDagSvgConfig,
}

/// SVG rendering configuration for Goal Transition Tree (G1).
///
/// Controls node dimensions, spacing, fonts, and semantic colors
/// for the hierarchical goal tree visualization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalTreeSvgConfig {
    /// Width of each goal node (pixels).
    pub node_width: f64,
    /// Height of each goal node (pixels).
    pub node_height: f64,
    /// Vertical spacing between hierarchy levels (pixels).
    pub level_spacing: f64,
    /// Horizontal spacing between sibling nodes (pixels).
    pub node_spacing: f64,
    /// Canvas margin around the entire tree (pixels).
    pub margin: f64,
    /// Reserved height for legend area at bottom (pixels).
    pub legend_height: f64,
    /// Stroke width for node borders (pixels).
    pub node_stroke_width: f64,
    /// Stroke width for edge lines (pixels).
    pub edge_stroke_width: f64,
    /// Corner radius for node rectangles (pixels).
    pub node_corner_radius: f64,
    /// Font size for hierarchical ID labels (pixels).
    pub font_size_id: f64,
    /// Font size for node text labels (pixels).
    pub font_size_label: f64,
    /// Font size for edge labels and step ranges (pixels).
    pub font_size_detail: f64,
    /// Font size for legend title (pixels).
    pub font_size_legend_title: f64,
    /// Maximum characters per line for text wrapping in nodes.
    pub text_wrap_max_chars: usize,
    /// Maximum number of text lines displayed in a node.
    pub max_text_lines: usize,
    /// Line height for wrapped text (pixels).
    pub line_height: f64,
    /// Goal tree semantic colors.
    pub colors: GoalTreeColors,
}

/// Semantic colors for Goal Transition Tree nodes, edges, and UI elements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalTreeColors {
    /// Fill color for Done/completed goal nodes.
    pub status_done: String,
    /// Fill color for Failed goal nodes.
    pub status_failed: String,
    /// Fill color for Abandoned goal nodes.
    pub status_abandoned: String,
    /// Fill color for Work-in-Progress goal nodes.
    pub status_partial: String,
    /// Edge color for Sub (parent→child) edges.
    pub edge_sub: String,
    /// Edge color for Next (sibling→sibling) edges.
    pub edge_next: String,
    /// Edge color for Backtrack (child→parent) edges.
    pub edge_backtrack: String,
    /// Edge color for success backtrack (completed→parent).
    pub edge_success: String,
    /// Node border stroke color.
    pub node_border: String,
    /// Primary text color for node labels.
    pub text_primary: String,
    /// Secondary text color for IDs and step ranges.
    pub text_secondary: String,
    /// Legend border/outline color.
    pub legend_border: String,
}

/// SVG rendering configuration for Reasoning Artifact DAG (G2).
///
/// Controls node dimensions, spacing, fonts, and semantic colors
/// for the directed acyclic graph of reasoning artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningDagSvgConfig {
    /// Width of each reasoning node (pixels).
    pub node_width: f64,
    /// Height of each reasoning node (pixels).
    pub node_height: f64,
    /// Stroke width for node borders (pixels).
    pub node_stroke_width: f64,
    /// Stroke width for edge lines (pixels).
    pub edge_stroke_width: f64,
    /// Corner radius for node rectangles (pixels).
    pub node_corner_radius: f64,
    /// Font size for node content text (pixels).
    pub font_size_content: f64,
    /// Font size for metadata (turn, confidence) (pixels).
    pub font_size_detail: f64,
    /// Font size for legend title (pixels).
    pub font_size_legend_title: f64,
    /// Font size for legend items (pixels).
    pub font_size_legend_item: f64,
    /// Maximum characters per line for text wrapping in nodes.
    pub text_wrap_max_chars: usize,
    /// Maximum number of text lines displayed in a node.
    pub max_text_lines: usize,
    /// Line height for wrapped text (pixels).
    pub line_height: f64,
    /// Reasoning DAG semantic colors.
    pub colors: ReasoningDagColors,
}

/// Semantic colors for Reasoning DAG nodes, edges, and UI elements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningDagColors {
    /// Fill color for Ground Truth nodes.
    pub node_ground_truth: String,
    /// Fill color for Insight nodes.
    pub node_insight: String,
    /// Edge color for Infers relationships.
    pub edge_infers: String,
    /// Edge color for Contradicts relationships.
    pub edge_contradicts: String,
    /// Edge color for Supersedes relationships.
    pub edge_supersedes: String,
    /// Node border stroke color.
    pub node_border: String,
    /// Primary text color for node content.
    pub text_primary: String,
    /// Secondary text color for metadata.
    pub text_secondary: String,
    /// Legend border/outline color.
    pub legend_border: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConfig {
    pub read: String,
    pub write: String,
    pub edit: String,
    pub run: String,
    pub list: String,
    pub other: String,
    pub edge: String,
    pub text: String,
    pub border: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConfig {
    pub x_spacing: f64,
    pub y_spacing: f64,
    pub min_node_separation: f64,
    pub max_iterations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostMapConfig {
    pub min_node_area: f64,
    pub padding: f64,
    pub label_font_size: f64,
    pub cost_font_size: f64,
    pub color_intensity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsingConfig {
    pub max_line_length: usize,
    pub trim_whitespace: bool,
    pub default_cost: DefaultCostConfig,
    pub estimation: CostEstimationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultCostConfig {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub dollar_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimationConfig {
    pub chars_per_token: i64,
    pub default_model: String, // Which pricing to use: "haiku", "sonnet", "opus"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    pub default_output_dir: String,
    pub default_format: String,
    pub batch_workers: usize,
    pub progress_refresh_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphConfig {
    pub activity_graph: ActivityGraphConfig,
    pub cost_map: CostMapGraphConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityGraphConfig {
    pub merge_same_object: bool,
    pub enable_hierarchy: bool,
    pub max_hierarchy_depth: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostMapGraphConfig {
    pub group_by_goal: bool,
    pub flatten_single_children: bool,
    pub min_cost_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IgrConfig {
    pub pretty_print: bool,
    pub indent_spaces: usize,
    pub include_metadata: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    pub dev_port: u16,
    pub max_upload_size: u64,
    pub wasm_console_log: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub log_to_file: bool,
    pub log_file_path: String,
    pub include_timestamps: bool,
    pub include_module_names: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    pub io_buffer_size: usize,
    pub parser_cache_max_size: u64,
    pub parallel_graph_building: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    pub enable_validation: bool,
    pub strict_validation: bool,
    pub log_validation_errors: bool,
}

impl Config {
    /// Load configuration from config.toml.
    ///
    /// Search order:
    /// 1. ./config.toml (current directory)
    /// 2. ~/.config/trajlens/config.toml
    ///
    /// Returns error if file not found or invalid TOML.
    pub fn load() -> Result<Self, crate::error::TrajLensError> {
        let search_paths = vec![
            PathBuf::from("config.toml"),
            dirs::config_dir()
                .map(|d| d.join("trajlens/config.toml"))
                .unwrap_or_else(|| PathBuf::from("~/.config/trajlens/config.toml")),
        ];

        for path in search_paths {
            if path.exists() {
                let content = std::fs::read_to_string(&path).map_err(|e| {
                    crate::error::TrajLensError::Config(format!(
                        "Failed to read config {:?}: {}",
                        path, e
                    ))
                })?;

                let config: Config = toml::from_str(&content).map_err(|e| {
                    crate::error::TrajLensError::Config(format!(
                        "Failed to parse config {:?}: {}",
                        path, e
                    ))
                })?;

                return Ok(config);
            }
        }

        Err(crate::error::TrajLensError::Config(
            "No config.toml found in search paths".into(),
        ))
    }

    /// Load from specific path.
    pub fn load_from_path(path: &str) -> Result<Self, crate::error::TrajLensError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            crate::error::TrajLensError::Config(format!("Failed to read config {}: {}", path, e))
        })?;

        toml::from_str(&content).map_err(|e| {
            crate::error::TrajLensError::Config(format!("Failed to parse config {}: {}", path, e))
        })
    }
}

impl Default for Config {
    /// Default configuration matching config.toml defaults.
    fn default() -> Self {
        Self {
            llm: LLMConfig {
                anthropic: AnthropicConfig {
                    api_url: "https://api.anthropic.com/v1/messages".to_string(),
                    api_version: "2023-06-01".to_string(),
                    default_model: "claude-3-5-sonnet-20241022".to_string(),
                    max_tokens: 16384,
                    temperature: 0.0,
                    timeout_secs: 300,
                },
                bedrock: BedrockConfig {
                    default_region: "us-west-2".to_string(),
                    haiku_model_id: "anthropic.claude-3-5-haiku-20241022-v1:0".to_string(),
                    sonnet_model_id: "anthropic.claude-3-5-sonnet-20241022-v2:0".to_string(),
                    opus_model_id: "anthropic.claude-opus-4-20250514-v1:0".to_string(),
                    max_tokens: 16384,
                    temperature: 0.0,
                    timeout_secs: 300,
                },
            },
            cost: CostConfig {
                models: ModelsConfig {
                    haiku: ModelPricing {
                        input_per_million: 1.0,
                        output_per_million: 5.0,
                    },
                    sonnet: ModelPricing {
                        input_per_million: 3.0,
                        output_per_million: 15.0,
                    },
                    opus: ModelPricing {
                        input_per_million: 15.0,
                        output_per_million: 75.0,
                    },
                },
            },
            rendering: RenderingConfig {
                svg: SvgConfig {
                    margin: 20.0,
                    node_width: 200.0,
                    node_height_base: 80.0,
                    node_height_per_operation: 20.0,
                    font_size_label: 12.0,
                    font_size_detail: 10.0,
                    font_family: "Arial, sans-serif".to_string(),
                    text_wrap_max_chars: 30,
                    stroke_width: 1.0,
                    edge_stroke_width: 2.0,
                    arrow_head_size: 8.0,
                    colors: ColorConfig {
                        read: "#e3f2fd".to_string(),
                        write: "#fce4ec".to_string(),
                        edit: "#fff3e0".to_string(),
                        run: "#e8f5e9".to_string(),
                        list: "#f3e5f5".to_string(),
                        other: "#f5f5f5".to_string(),
                        edge: "#666666".to_string(),
                        text: "#000000".to_string(),
                        border: "#cccccc".to_string(),
                    },
                    goal_tree: GoalTreeSvgConfig {
                        node_width: 320.0,
                        node_height: 90.0,
                        level_spacing: 150.0,
                        node_spacing: 40.0,
                        margin: 60.0,
                        legend_height: 160.0,
                        node_stroke_width: 1.5,
                        edge_stroke_width: 2.0,
                        node_corner_radius: 5.0,
                        font_size_id: 10.0,
                        font_size_label: 9.0,
                        font_size_detail: 8.0,
                        font_size_legend_title: 12.0,
                        text_wrap_max_chars: 45,
                        max_text_lines: 12,
                        line_height: 12.0,
                        colors: GoalTreeColors {
                            status_done: "#C8E6C9".to_string(),
                            status_failed: "#FFCDD2".to_string(),
                            status_abandoned: "#E0E0E0".to_string(),
                            status_partial: "#FFF9C4".to_string(),
                            edge_sub: "#2196F3".to_string(),
                            edge_next: "#4CAF50".to_string(),
                            edge_backtrack: "#F44336".to_string(),
                            edge_success: "#4CAF50".to_string(),
                            node_border: "#666".to_string(),
                            text_primary: "#000".to_string(),
                            text_secondary: "#666".to_string(),
                            legend_border: "#999".to_string(),
                        },
                    },
                    reasoning_dag: ReasoningDagSvgConfig {
                        node_width: 220.0,
                        node_height: 190.0,
                        node_stroke_width: 1.5,
                        edge_stroke_width: 2.0,
                        node_corner_radius: 5.0,
                        font_size_content: 10.0,
                        font_size_detail: 9.0,
                        font_size_legend_title: 12.0,
                        font_size_legend_item: 10.0,
                        text_wrap_max_chars: 28,
                        max_text_lines: 10,
                        line_height: 14.0,
                        colors: ReasoningDagColors {
                            node_ground_truth: "#E3F2FD".to_string(),
                            node_insight: "#FFF3E0".to_string(),
                            edge_infers: "#2196F3".to_string(),
                            edge_contradicts: "#F44336".to_string(),
                            edge_supersedes: "#FF9800".to_string(),
                            node_border: "#666".to_string(),
                            text_primary: "#000".to_string(),
                            text_secondary: "#666".to_string(),
                            legend_border: "#999".to_string(),
                        },
                    },
                },
                layout: LayoutConfig {
                    x_spacing: 100.0,
                    y_spacing: 150.0,
                    min_node_separation: 20.0,
                    max_iterations: 100,
                },
                cost_map: CostMapConfig {
                    min_node_area: 100.0,
                    padding: 2.0,
                    label_font_size: 10.0,
                    cost_font_size: 8.0,
                    color_intensity: 0.7,
                },
            },
            parsing: ParsingConfig {
                max_line_length: 10000,
                trim_whitespace: true,
                default_cost: DefaultCostConfig {
                    input_tokens: 0,
                    output_tokens: 0,
                    dollar_cost: 0.0,
                },
                estimation: CostEstimationConfig {
                    chars_per_token: 4,
                    default_model: "sonnet".to_string(),
                },
            },
            cli: CliConfig {
                default_output_dir: "./output".to_string(),
                default_format: "auto".to_string(),
                batch_workers: 0,
                progress_refresh_ms: 100,
            },
            graph: GraphConfig {
                activity_graph: ActivityGraphConfig {
                    merge_same_object: true,
                    enable_hierarchy: true,
                    max_hierarchy_depth: 10,
                },
                cost_map: CostMapGraphConfig {
                    group_by_goal: false,
                    flatten_single_children: true,
                    min_cost_threshold: 0.0001,
                },
            },
            igr: IgrConfig {
                pretty_print: true,
                indent_spaces: 2,
                include_metadata: true,
            },
            web: WebConfig {
                dev_port: 5173,
                max_upload_size: 10485760,
                wasm_console_log: false,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                log_to_file: false,
                log_file_path: "./trajlens.log".to_string(),
                include_timestamps: true,
                include_module_names: true,
            },
            performance: PerformanceConfig {
                io_buffer_size: 8192,
                parser_cache_max_size: 104857600,
                parallel_graph_building: true,
            },
            validation: ValidationConfig {
                enable_validation: true,
                strict_validation: false,
                log_validation_errors: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(
            config.llm.anthropic.api_url,
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(config.rendering.svg.margin, 20.0);
        assert_eq!(config.parsing.estimation.chars_per_token, 4);
    }

    #[test]
    fn test_get_config() {
        let config = get_config();
        assert_eq!(config.llm.anthropic.api_version, "2023-06-01");
    }
}
