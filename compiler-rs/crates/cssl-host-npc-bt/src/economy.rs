// § economy.rs — closed-loop city-economy ; supply/demand price-curve
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § CITY-ECONOMY ; per-day-tick price adjust ; ¬ infinite-source
// § I> COLLAPSE-DET : price ∈ [floor, ceil] ; clamp on breach + audit
// § I> player-inject : sells → supply-incr → next-day price-decr
// § I> player-extract : buys → demand-incr → next-day price-incr
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Per-good price-state in a market.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketPrice {
    /// Stable good-identifier (host-defined).
    pub good_id: u32,
    /// Current price ; clamped to [floor, ceil].
    pub price: f32,
    /// Aggregate supply this period (NPC-listings + player-injections).
    pub supply: u32,
    /// Aggregate demand this period (NPC-buy-orders + player-extractions).
    pub demand: u32,
}

impl MarketPrice {
    /// Construct a fresh entry @ initial-price.
    #[must_use]
    pub fn new(good_id: u32, price: f32) -> Self {
        Self {
            good_id,
            price,
            supply: 0,
            demand: 0,
        }
    }
}

/// Floor/ceiling clamp per spec § COLLAPSE-DET.
const PRICE_FLOOR: f32 = 0.10;
const PRICE_CEIL: f32 = 1000.0;

/// Tick the market by `dt` (game-days). Adjusts price toward supply/demand equilibrium ;
/// resets per-period accumulators after applying.
///
/// § I> Price-update : Δprice = α · (demand - supply) / max(supply, 1) · dt ;
///   α = 0.05 day⁻¹.
/// § I> Always clamps to [floor, ceil] ; does not panic.
pub fn tick_market(prices: &mut [MarketPrice], dt: f32) {
    if dt <= 0.0 {
        return;
    }
    const ALPHA: f32 = 0.05;
    for mp in prices.iter_mut() {
        let supply_safe = mp.supply.max(1) as f32;
        let net = mp.demand as f32 - mp.supply as f32;
        let delta = ALPHA * (net / supply_safe) * dt;
        let new_price = (mp.price + mp.price * delta).clamp(PRICE_FLOOR, PRICE_CEIL);
        mp.price = new_price;
        mp.supply = 0;
        mp.demand = 0;
    }
}

/// Player-trade record applied to a market via `apply_player_trade`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerTrade {
    /// Good-id traded.
    pub good_id: u32,
    /// Quantity ; positive.
    pub qty: u32,
    /// Direction : true = player-sells (supply-incr) ; false = player-buys (demand-incr).
    pub player_sells: bool,
}

/// Apply a player-trade to the market's per-period supply/demand accumulators.
///
/// § I> Per GDD § PLAYER-INJECT / § PLAYER-EXTRACT : supply ↑ on sell, demand ↑ on buy.
pub fn apply_player_trade(prices: &mut [MarketPrice], trade: PlayerTrade) {
    if let Some(mp) = prices.iter_mut().find(|p| p.good_id == trade.good_id) {
        if trade.player_sells {
            mp.supply = mp.supply.saturating_add(trade.qty);
        } else {
            mp.demand = mp.demand.saturating_add(trade.qty);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_zero_dt_is_noop() {
        let mut p = vec![MarketPrice::new(1, 10.0)];
        p[0].demand = 100;
        tick_market(&mut p, 0.0);
        assert!((p[0].price - 10.0).abs() < 1e-6);
    }

    #[test]
    fn demand_pushes_price_up() {
        let mut p = vec![MarketPrice::new(1, 10.0)];
        p[0].supply = 5;
        p[0].demand = 50;
        tick_market(&mut p, 1.0);
        assert!(p[0].price > 10.0);
    }

    #[test]
    fn player_sell_increases_supply() {
        let mut p = vec![MarketPrice::new(7, 5.0)];
        apply_player_trade(
            &mut p,
            PlayerTrade {
                good_id: 7,
                qty: 12,
                player_sells: true,
            },
        );
        assert_eq!(p[0].supply, 12);
        assert_eq!(p[0].demand, 0);
    }
}
