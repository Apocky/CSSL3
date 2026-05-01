// § tests/economy_market.rs — supply/demand curve + player-injection
// ════════════════════════════════════════════════════════════════════
// § I> 3 tests : excess-supply price-down · player-injection effect · price-floor-clamp
// ════════════════════════════════════════════════════════════════════

use cssl_host_npc_bt::economy::{
    MarketPrice, PlayerTrade, apply_player_trade, tick_market,
};

#[test]
fn excess_supply_pushes_price_down() {
    let mut p = vec![MarketPrice::new(1, 10.0)];
    p[0].supply = 50;
    p[0].demand = 5;
    tick_market(&mut p, 1.0);
    assert!(
        p[0].price < 10.0,
        "expected price-decrease ; got {}",
        p[0].price
    );
}

#[test]
fn player_buy_increases_demand_then_price() {
    let mut p = vec![MarketPrice::new(2, 10.0)];
    apply_player_trade(
        &mut p,
        PlayerTrade {
            good_id: 2,
            qty: 100,
            player_sells: false,
        },
    );
    assert_eq!(p[0].demand, 100);
    p[0].supply = 1;
    tick_market(&mut p, 1.0);
    assert!(p[0].price > 10.0);
}

#[test]
fn price_clamps_to_floor() {
    let mut p = vec![MarketPrice::new(3, 0.05)];
    p[0].supply = 1000;
    p[0].demand = 1;
    tick_market(&mut p, 10.0);
    // After massive supply-pressure, price is clamped to floor (0.10).
    assert!(p[0].price >= 0.10);
}
