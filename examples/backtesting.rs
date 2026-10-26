//! A host-run buy-and-hold backtest and the library's strategy report metrics.
//! Buys two units at the first open and exits at the last close, with one currency unit
//! of commission on each fill. Equity is marked at every bar close after incurred fees.
//!
//! Run headlessly with `cargo run --example backtesting --no-default-features`.

mod support;

use chrono::{TimeDelta, TimeZone, Utc};
use egui_charts::model::{
    Bar, StrategyExit, StrategyReport, StrategySide, StrategyTrade, Timeframe,
};

fn buy_and_hold(bars: &[Bar]) -> StrategyReport {
    let mut report = StrategyReport::new("Buy and hold", "SAMPLE", Timeframe::Hour1, 10_000.0);
    report.currency = Some("USD".into());
    let (Some(first), Some(last)) = (bars.first(), bars.last()) else {
        return report;
    };
    let quantity = 2.0;
    let commission = 1.0;
    let profit = (last.close - first.open) * quantity - 2.0 * commission;
    let mut trade = StrategyTrade::new(StrategySide::Long, quantity, first.time, first.open);
    trade.fees = 2.0 * commission;
    trade.exit = Some(StrategyExit::new(
        last.time + TimeDelta::hours(1),
        last.close,
        profit,
    ));
    report.trades.push(trade);
    // Include starting capital so the first bar's loss and entry fee enter drawdown metrics.
    report.equity.push((first.time, report.initial_capital));
    report
        .equity
        .extend(bars.iter().enumerate().map(|(i, bar)| {
            let fees = commission + if i + 1 == bars.len() { commission } else { 0.0 };
            (
                bar.time + TimeDelta::hours(1),
                report.initial_capital + (bar.close - first.open) * quantity - fees,
            )
        }));
    report
}

fn main() {
    let end = Utc.with_ymd_and_hms(2026, 1, 19, 0, 0, 0).unwrap();
    let bars = support::sample_bars(600, 100.0, TimeDelta::hours(1), end);
    let report = buy_and_hold(&bars);
    let metrics = report.performance();
    println!(
        "{} on {} (generated sample data)",
        report.name, report.symbol
    );
    println!("Closed trades: {}", metrics.closed_trades);
    println!("Net profit after fees: {:.2} USD", metrics.net_profit);
    println!(
        "Maximum drawdown: {:.2} USD",
        metrics
            .max_drawdown
            .expect("sample has a valid equity curve")
    );
    println!("{}", report.trades_csv());
}
