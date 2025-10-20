use crate::types::{AggregationMethod, SourcePrice};
use std::error::Error;

/// Calculate aggregated price from multiple source prices
pub fn aggregate_prices(
    prices: &[SourcePrice],
    method: &AggregationMethod,
) -> Result<f64, Box<dyn Error>> {
    if prices.is_empty() {
        return Err("No prices to aggregate".into());
    }

    match method {
        AggregationMethod::Average => calculate_average(prices),
        AggregationMethod::Median => calculate_median(prices),
        AggregationMethod::WeightedAvg => calculate_weighted_average(prices),
    }
}

/// Calculate arithmetic mean
fn calculate_average(prices: &[SourcePrice]) -> Result<f64, Box<dyn Error>> {
    let numbers: Vec<f64> = prices.iter()
        .filter_map(|p| p.value.as_number())
        .collect();

    if numbers.is_empty() {
        return Err("No numeric values to aggregate".into());
    }

    let sum: f64 = numbers.iter().sum();
    Ok(sum / numbers.len() as f64)
}

/// Calculate median (middle value when sorted)
fn calculate_median(prices: &[SourcePrice]) -> Result<f64, Box<dyn Error>> {
    let mut sorted_prices: Vec<f64> = prices.iter()
        .filter_map(|p| p.value.as_number())
        .collect();

    if sorted_prices.is_empty() {
        return Err("No numeric values to aggregate".into());
    }

    sorted_prices.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let len = sorted_prices.len();
    if len % 2 == 0 {
        // Even number of prices: average of two middle values
        Ok((sorted_prices[len / 2 - 1] + sorted_prices[len / 2]) / 2.0)
    } else {
        // Odd number of prices: middle value
        Ok(sorted_prices[len / 2])
    }
}

/// Calculate weighted average (currently using equal weights)
fn calculate_weighted_average(prices: &[SourcePrice]) -> Result<f64, Box<dyn Error>> {
    // For now, use equal weights (same as average)
    // Can be extended with reputation-based weighting
    calculate_average(prices)
}

/// Calculate price deviation percentage between min and max prices
pub fn calculate_price_deviation(prices: &[SourcePrice]) -> f64 {
    let numbers: Vec<f64> = prices.iter()
        .filter_map(|p| p.value.as_number())
        .collect();

    if numbers.len() < 2 {
        return 0.0;
    }

    let mut min_price = f64::MAX;
    let mut max_price = f64::MIN;

    for &price in &numbers {
        if price < min_price {
            min_price = price;
        }
        if price > max_price {
            max_price = price;
        }
    }

    if min_price == 0.0 {
        return 100.0;
    }

    ((max_price - min_price) / min_price) * 100.0
}
