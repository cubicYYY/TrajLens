/// Post-parse cost estimator for trajectories that lack native cost data.
///
/// Approximates per-item costs by:
/// 1. Counting characters in content and args
/// 2. Converting to estimated token count (chars_per_token ratio from config)
/// 3. Applying configured model pricing (haiku/sonnet/opus)
use crate::config::get_config;
use crate::models::{Cost, Item, ItemCategory, Step, Trajectory};

/// Estimates token costs for trajectories with no native cost data.
/// Only modifies items that currently have zero cost.
pub fn estimate_costs(trajectory: &Trajectory) -> Trajectory {
    let new_steps: Vec<Step> = trajectory
        .steps
        .iter()
        .map(|step| {
            let new_items: Vec<Item> = step
                .items
                .iter()
                .map(|item| {
                    let mut item = item.clone();
                    if item.cost.dollar_cost == 0.0 && item.cost.input_tokens == 0 {
                        item.cost = estimate_item_cost(&item);
                    }
                    item
                })
                .collect();
            Step {
                step_id: step.step_id,
                items: new_items,
                timestamp_start: step.timestamp_start,
                timestamp_end: step.timestamp_end,
                raw_line_range: step.raw_line_range,
            }
        })
        .collect();

    let total_cost = Cost {
        input_tokens: new_steps
            .iter()
            .flat_map(|s| &s.items)
            .map(|i| i.cost.input_tokens)
            .sum(),
        output_tokens: new_steps
            .iter()
            .flat_map(|s| &s.items)
            .map(|i| i.cost.output_tokens)
            .sum(),
        cache_read_tokens: new_steps
            .iter()
            .flat_map(|s| &s.items)
            .map(|i| i.cost.cache_read_tokens)
            .sum(),
        cache_write_tokens: new_steps
            .iter()
            .flat_map(|s| &s.items)
            .map(|i| i.cost.cache_write_tokens)
            .sum(),
        dollar_cost: new_steps
            .iter()
            .flat_map(|s| &s.items)
            .map(|i| i.cost.dollar_cost)
            .sum(),
    };

    Trajectory {
        label: trajectory.label.clone(),
        steps: new_steps,
        total_cost,
        outcome: trajectory.outcome.clone(),
    }
}

fn estimate_item_cost(item: &Item) -> Cost {
    let config = get_config();
    let chars_per_token = config.parsing.estimation.chars_per_token;

    // Select model pricing based on config
    let pricing = match config.parsing.estimation.default_model.as_str() {
        "haiku" => &config.cost.models.haiku,
        "opus" => &config.cost.models.opus,
        _ => &config.cost.models.sonnet, // Default to sonnet
    };

    let pricing_input = pricing.input_per_million / 1_000_000.0;
    let pricing_output = pricing.output_per_million / 1_000_000.0;
    // Cache read is typically 10% of input cost
    let pricing_cache_read = pricing_input * 0.1;

    let content_chars = item.content.len() as i64;
    let args_chars: i64 = item
        .args
        .iter()
        .map(|(k, v)| k.len() as i64 + v.len() as i64)
        .sum();
    let total_chars = content_chars + args_chars;
    let estimated_tokens = (total_chars / chars_per_token).max(1);

    match item.category {
        ItemCategory::Think => Cost {
            input_tokens: 0,
            output_tokens: estimated_tokens,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            dollar_cost: estimated_tokens as f64 * pricing_output,
        },
        ItemCategory::Action => Cost {
            input_tokens: estimated_tokens,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            dollar_cost: estimated_tokens as f64 * pricing_input,
        },
        ItemCategory::Input => Cost {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: estimated_tokens,
            cache_write_tokens: 0,
            dollar_cost: estimated_tokens as f64 * pricing_cache_read,
        },
        _ => Cost {
            input_tokens: estimated_tokens,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            dollar_cost: estimated_tokens as f64 * pricing_input,
        },
    }
}
