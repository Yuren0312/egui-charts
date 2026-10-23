//! Trading session management and break detection.
//!
//! Financial markets operate in defined sessions (e.g., NYSE 9:30 AM--4:00 PM
//! ET). This module provides a pluggable [`SessionProvider`] trait for
//! detecting session boundaries (daily, weekly, monthly, or custom) within
//! bar data, plus concrete implementations:
//!
//! | Provider | Detects |
//! |----------|---------|
//! | [`DailySessionProvider`] | Day boundaries (configurable break hour) |
//! | [`ExchangeSessionProvider`] | Session opens of an instrument's trading hours |
//! | [`WeeklySessionProvider`] | Week boundaries (configurable break day) |
//! | [`MonthlySessionProvider`] | Month boundaries |
//! | [`CompositeSessionProvider`] | Combines multiple providers |
//!
//! Use [`find_session_breaks`] to scan a bar slice for all breaks.

use crate::model::{Bar, Session, TimezoneId};
use chrono::{
    DateTime, Datelike, Days, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc, Weekday,
};

/// Trait for defining trading session boundaries.
///
/// Implement this to create custom session-break logic (e.g., exchange-
/// specific holidays, pre-market/after-hours splits, or futures roll
/// dates). All built-in providers implement this trait.
pub trait SessionProvider: Send + Sync {
    /// Check if a session break occurs between two ts
    /// Returns Some(SessionBreak) if there's a break, None otherwise
    fn session_break_between(
        &self,
        from: &DateTime<Utc>,
        to: &DateTime<Utc>,
    ) -> Option<SessionBreak>;

    /// Get all session breaks within a time range
    fn session_breaks_in_range(
        &self,
        start: &DateTime<Utc>,
        end: &DateTime<Utc>,
    ) -> Vec<SessionBreak> {
        let mut breaks = Vec::new();
        let mut current = *start;

        while current < *end {
            let next = current + Duration::minutes(1);
            if let Some(session_break) = self.session_break_between(&current, &next) {
                breaks.push(session_break);
            }
            current = next;
        }

        breaks
    }

    /// The breaks between consecutive `bars`, as [`find_session_breaks`] returns them. The
    /// default asks [`session_break_between`](Self::session_break_between) for each pair; a
    /// provider can find them in one pass instead.
    fn session_breaks(&self, bars: &[Bar]) -> Vec<(usize, SessionBreak)> {
        let pairs = bars.windows(2).enumerate();
        pairs
            .filter_map(|(i, pair)| {
                let session_break = self.session_break_between(&pair[0].time, &pair[1].time)?;
                Some((i + 1, session_break))
            })
            .collect()
    }

    /// Get display name for this session provider
    fn name(&self) -> &str;
}

/// A detected session break with metadata.
///
/// Produced by [`SessionProvider::session_break_between`] when a session
/// boundary falls between two timestamps. Contains the break time, type,
/// and an optional human-readable label for rendering on the time axis.
#[derive(Debug, Clone)]
pub struct SessionBreak {
    /// Ts of the break
    pub ts: DateTime<Utc>,
    /// Type of session break
    pub break_type: SessionBreakType,
    /// Optional label to display
    pub label: Option<String>,
}

/// Classification of a session break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionBreakType {
    /// Daily session break (end of trading day)
    Daily,
    /// Weekly session break (end of trading week)
    Weekly,
    /// Monthly session break (end of trading month)
    Monthly,
    /// Custom session break
    Custom,
}

/// Daily session provider with a configurable break hour.
///
/// Detects day boundaries at the break hour and minute, in UTC. Use [`nyse`](Self::nyse) for a preset
/// matching NYSE regular-session close, or [`continuous`](Self::continuous)
/// for 24/7 markets (midnight UTC).
#[derive(Debug, Clone, Default)]
pub struct DailySessionProvider {
    /// Hour of day for session break (0-23)
    pub break_hour: u32,
    /// Minute of hour for session break (0-59)
    pub break_minute: u32,
}

impl DailySessionProvider {
    /// Create a daily provider that breaks at the given UTC hour and minute.
    pub fn new(hour: u32, minute: u32) -> Self {
        Self {
            break_hour: hour.min(23),
            break_minute: minute.min(59),
        }
    }

    /// Create provider for NYSE regular session (9:30 AM - 4:00 PM ET)
    pub fn nyse() -> Self {
        Self::new(21, 0) // 4:00 PM ET = 21:00 UTC (during standard time)
    }

    /// Create provider for 24/7 markets (midnight UTC)
    pub fn continuous() -> Self {
        Self::default()
    }
}

impl SessionProvider for DailySessionProvider {
    fn session_break_between(
        &self,
        from: &DateTime<Utc>,
        to: &DateTime<Utc>,
    ) -> Option<SessionBreak> {
        // A day runs from one break time to the next.
        let offset =
            Duration::hours(self.break_hour.into()) + Duration::minutes(self.break_minute.into());
        let day = |time: &DateTime<Utc>| (*time - offset).date_naive();

        if day(from) != day(to) {
            Some(SessionBreak {
                ts: *to,
                break_type: SessionBreakType::Daily,
                label: Some(format!("{}", to.format("%Y-%m-%d"))),
            })
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "Daily Sessions"
    }
}

/// Daily session provider for an exchange: a day starts at each session open of its trading
/// hours, in its time zone, so bars outside the hours stay with the session before them.
/// Without trading hours a day starts at midnight in the time zone.
#[derive(Debug, Clone)]
pub struct ExchangeSessionProvider {
    time_zone: TimezoneId,
    session: Option<Session>,
}

impl ExchangeSessionProvider {
    /// Days of `session`, or calendar days without one, in `time_zone`.
    pub fn new(time_zone: TimezoneId, session: Option<Session>) -> Self {
        Self { time_zone, session }
    }

    /// The instants trading days start around `times`, sorted, each with its trading date: the
    /// session opens whose windows close on dates from 9 days before the UTC date of a time to
    /// 2 days after it (the local date is within a day of the UTC one, a weekly schedule opens
    /// at least once a week, and a window open since the evening closes the next day), or local
    /// midnights without trading hours. Days
    /// far from every time are skipped, so the work follows the times, not the span between
    /// them. An open that falls in a repeated hour is its first instant, and one that falls in
    /// a skipped hour is the instant the clocks jump.
    fn opens(
        &self,
        times: impl IntoIterator<Item = DateTime<Utc>>,
    ) -> Vec<(DateTime<Utc>, NaiveDate)> {
        let tz = self.time_zone.tz();
        // UTC days since the epoch: whole numbers, cheaper per bar than calendar dates.
        let mut days: Vec<i64> = Vec::new();
        for day in times
            .into_iter()
            .map(|time| time.timestamp().div_euclid(86_400))
        {
            if days.last() != Some(&day) {
                days.push(day);
            }
        }
        days.sort_unstable();
        days.dedup();
        let dates = days
            .into_iter()
            .filter_map(|day| DateTime::from_timestamp(day * 86_400, 0))
            .map(|start| start.date_naive());
        // Disjoint, sorted ranges of days, so that each day is visited once.
        let mut ranges: Vec<(NaiveDate, NaiveDate)> = Vec::new();
        for date in dates {
            let first = date.checked_sub_days(Days::new(9)).unwrap_or(date);
            let last = date.checked_add_days(Days::new(2)).unwrap_or(date);
            match ranges.last_mut() {
                Some((_, end)) if first <= *end => *end = last.max(*end),
                _ => ranges.push((first, last)),
            }
        }
        let instant = |local: NaiveDateTime| {
            // A skipped local time opens at the first minute after it that exists.
            (0..=180).find_map(|minute| {
                let local = local.checked_add_signed(Duration::minutes(minute))?;
                tz.from_local_datetime(&local)
                    .earliest()
                    .map(|open| open.to_utc())
            })
        };
        let days = ranges
            .into_iter()
            .flat_map(|(first, last)| first.iter_days().take_while(move |date| *date <= last));
        let mut opens: Vec<_> = match &self.session {
            None => days
                .filter_map(|date| Some((instant(date.and_time(NaiveTime::MIN))?, date)))
                .collect(),
            Some(session) => days
                .flat_map(|close_date| {
                    session.windows.iter().filter_map(move |window| {
                        if !window.days.contains(&close_date.weekday()) {
                            return None;
                        }
                        // A window that closes at or before its open spans midnight.
                        let open_date = if window.close <= window.open {
                            close_date.pred_opt()?
                        } else {
                            close_date
                        };
                        Some((NaiveDateTime::new(open_date, window.open), close_date))
                    })
                })
                .filter_map(|(open, close_date)| Some((instant(open)?, close_date)))
                .collect(),
        };
        opens.sort_by_key(|(open, _)| *open);
        opens
    }

    /// The trading date of `time` among `opens`: that of the last open at or before it, or the
    /// calendar date in the time zone before the first.
    fn trading_date(&self, opens: &[(DateTime<Utc>, NaiveDate)], time: DateTime<Utc>) -> NaiveDate {
        match opens.partition_point(|(open, _)| *open <= time) {
            0 => time.with_timezone(&self.time_zone.tz()).date_naive(),
            n => opens[n - 1].1,
        }
    }

    fn session_break(date: NaiveDate, ts: DateTime<Utc>) -> SessionBreak {
        SessionBreak {
            ts,
            break_type: SessionBreakType::Daily,
            label: Some(date.format("%Y-%m-%d").to_string()),
        }
    }
}

impl SessionProvider for ExchangeSessionProvider {
    fn session_break_between(
        &self,
        from: &DateTime<Utc>,
        to: &DateTime<Utc>,
    ) -> Option<SessionBreak> {
        let opens = self.opens([*from, *to]);
        let date = self.trading_date(&opens, *to);
        (self.trading_date(&opens, *from) != date).then(|| Self::session_break(date, *to))
    }

    /// Finds the session opens around the bars' days once and places each bar among them.
    fn session_breaks(&self, bars: &[Bar]) -> Vec<(usize, SessionBreak)> {
        let opens = self.opens(bars.iter().map(|bar| bar.time));
        let dates: Vec<_> = bars
            .iter()
            .map(|bar| self.trading_date(&opens, bar.time))
            .collect();
        (1..bars.len())
            .filter(|&i| dates[i] != dates[i - 1])
            .map(|i| (i, Self::session_break(dates[i], bars[i].time)))
            .collect()
    }

    fn name(&self) -> &str {
        "Exchange Sessions"
    }
}

/// Weekly session provider that detects ISO-week boundaries.
///
/// Defaults to Friday (typical equity-market week end). Configure with
/// [`new`](Self::new) to use a different day.
#[derive(Debug, Clone)]
pub struct WeeklySessionProvider {
    /// Day of week for session break
    pub break_day: Weekday,
}

impl Default for WeeklySessionProvider {
    fn default() -> Self {
        Self {
            break_day: Weekday::Fri, // Friday close
        }
    }
}

impl WeeklySessionProvider {
    /// Create a weekly provider that breaks when the given weekday changes.
    pub fn new(break_day: Weekday) -> Self {
        Self { break_day }
    }
}

impl SessionProvider for WeeklySessionProvider {
    fn session_break_between(
        &self,
        from: &DateTime<Utc>,
        to: &DateTime<Utc>,
    ) -> Option<SessionBreak> {
        let from_week = from.iso_week().week();
        let to_week = to.iso_week().week();

        if from_week != to_week {
            Some(SessionBreak {
                ts: *to,
                break_type: SessionBreakType::Weekly,
                label: Some(format!("Week {to_week}")),
            })
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "Weekly Sessions"
    }
}

/// Monthly session provider that detects calendar-month boundaries.
#[derive(Debug, Clone, Default)]
pub struct MonthlySessionProvider;

impl SessionProvider for MonthlySessionProvider {
    fn session_break_between(
        &self,
        from: &DateTime<Utc>,
        to: &DateTime<Utc>,
    ) -> Option<SessionBreak> {
        if from.month() != to.month() || from.year() != to.year() {
            Some(SessionBreak {
                ts: *to,
                break_type: SessionBreakType::Monthly,
                label: Some(format!("{}", to.format("%B %Y"))),
            })
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "Monthly Sessions"
    }
}

/// Composite session provider that combines multiple providers.
///
/// When checking for a break, the first provider that reports one wins
/// (short-circuit). Use [`standard`](Self::standard) for the common
/// daily + weekly + monthly combination.
pub struct CompositeSessionProvider {
    providers: Vec<Box<dyn SessionProvider>>,
}

impl CompositeSessionProvider {
    /// Create an empty composite (no providers yet).
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Add a session provider to the composite.
    pub fn add_provider(&mut self, provider: Box<dyn SessionProvider>) {
        self.providers.push(provider);
    }

    /// Create standard provider with daily, weekly, and monthly breaks
    pub fn standard() -> Self {
        let mut composite = Self::new();
        composite.add_provider(Box::new(DailySessionProvider::default()));
        composite.add_provider(Box::new(WeeklySessionProvider::default()));
        composite.add_provider(Box::new(MonthlySessionProvider));
        composite
    }
}

impl Default for CompositeSessionProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionProvider for CompositeSessionProvider {
    fn session_break_between(
        &self,
        from: &DateTime<Utc>,
        to: &DateTime<Utc>,
    ) -> Option<SessionBreak> {
        // Return the highest priority break (Monthly > Weekly > Daily)
        for provider in &self.providers {
            if let Some(break_info) = provider.session_break_between(from, to) {
                return Some(break_info);
            }
        }
        None
    }

    fn name(&self) -> &str {
        "Composite Sessions"
    }
}

/// Scan a bar slice for session breaks.
///
/// Returns a `Vec` of `(bar_index, SessionBreak)` pairs indicating
/// where each break falls. The `bar_index` is the index of the bar
/// *after* the break (i.e., the first bar of the new session).
pub fn find_session_breaks(
    bars: &[Bar],
    provider: &dyn SessionProvider,
) -> Vec<(usize, SessionBreak)> {
    provider.session_breaks(bars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn create_test_bars() -> Vec<Bar> {
        vec![
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap(),
                open: 100.0,
                high: 105.0,
                low: 99.0,
                close: 102.0,
                volume: 1000.0,
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 2, 12, 0, 0).unwrap(),
                open: 102.0,
                high: 107.0,
                low: 101.0,
                close: 105.0,
                volume: 1200.0,
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 2, 1, 12, 0, 0).unwrap(),
                open: 105.0,
                high: 110.0,
                low: 104.0,
                close: 108.0,
                volume: 1500.0,
            },
        ]
    }

    /// A day starts at the provider's break time, or at each session open of an exchange's
    /// trading hours in its time zone, or at midnight there without hours; never at midnight
    /// UTC otherwise.
    #[test]
    fn days_start_at_the_break_time_or_the_session_open() {
        use crate::model::SessionWindow;
        use Weekday::*;
        use chrono::NaiveTime;

        let bars = |first: &str, count: usize, step: Duration| -> Vec<Bar> {
            let first: DateTime<Utc> = first.parse().unwrap();
            (0..count)
                .map(|i| Bar::new(first + step * i as i32, 1.0, 1.0, 1.0, 1.0, 1.0))
                .collect()
        };
        let hourly = |first: &str, count: usize| bars(first, count, Duration::hours(1));
        let at = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        let new_york = || TimezoneId::named("America/New_York").unwrap();
        let weekdays = [Mon, Tue, Wed, Thu, Fri];
        let regular = Session::new([SessionWindow::new(weekdays, at(9, 30), at(16, 0))]);
        // Opens at 18:00 on the evening before the day it closes on.
        let overnight = Session::new([SessionWindow::new(weekdays, at(18, 0), at(17, 0))]);
        let exchange = |session| ExchangeSessionProvider::new(new_york(), session);
        let every_day = [Mon, Tue, Wed, Thu, Fri, Sat, Sun];
        let london = ExchangeSessionProvider::new(
            TimezoneId::named("Europe/London").unwrap(),
            Some(Session::new([SessionWindow::new(
                every_day,
                at(1, 30),
                at(23, 0),
            )])),
        );

        // New York is 4 hours behind UTC in March 2024 after 10 March.
        type Case<'a> = (&'a str, &'a dyn SessionProvider, Vec<Bar>, &'a [usize]);
        let mut stray = bars("1970-01-02T15:00:00Z", 1, Duration::hours(1));
        stray.extend(hourly("2024-03-14T13:00:00Z", 27));
        let cases: [Case; 6] = [
            // 19:00 to 02:00 UTC: the day starts at 21:00.
            (
                "21:00 UTC",
                &DailySessionProvider::new(21, 0),
                hourly("2024-03-14T19:00:00Z", 8),
                &[2],
            ),
            // 09:00 EDT on Thursday to 11:00 EDT on Friday: each day starts with the first bar
            // after the 09:30 open, at 10:00.
            (
                "regular hours",
                &exchange(Some(regular.clone())),
                hourly("2024-03-14T13:00:00Z", 27),
                &[1, 25],
            ),
            // The same bars after one at 10:00 EST on 2 January 1970: the days between them have
            // no bars, and their opens are not needed.
            (
                "stray bar",
                &exchange(Some(regular.clone())),
                stray.clone(),
                &[1, 2, 26],
            ),
            // 22:00 to 01:00 EDT: midnight in New York.
            (
                "time zone",
                &exchange(None),
                hourly("2024-03-14T02:00:00Z", 4),
                &[2],
            ),
            // 16:00 to 19:00 EDT on Thursday: Friday's session opens at 18:00.
            (
                "overnight",
                &exchange(Some(overnight)),
                hourly("2024-03-14T20:00:00Z", 4),
                &[2],
            ),
            // Half-hourly from 00:00 BST on 27 October 2024, when 01:00 to 02:00 occurs twice:
            // the day starts at the first 01:30, 00:30 UTC, only.
            (
                "repeated hour",
                &london,
                bars("2024-10-26T23:00:00Z", 7, Duration::minutes(30)),
                &[3],
            ),
        ];
        for (case, provider, bars, want) in cases {
            let got: Vec<usize> = find_session_breaks(&bars, provider)
                .into_iter()
                .map(|(i, _)| i)
                .collect();
            assert_eq!(got, want, "{case}");
        }
        // Opens are found only for the ten days around each bar's date, not for the 54 years
        // between the stray bar and the others.
        let opens = exchange(Some(regular)).opens(stray.iter().map(|bar| bar.time));
        assert!(opens.len() <= 20, "{} opens", opens.len());
    }

    #[test]
    fn test_daily_session_break() {
        let provider = DailySessionProvider::default();
        let bars = create_test_bars();

        let breaks = find_session_breaks(&bars, &provider);
        assert_eq!(breaks.len(), 2); // 2 day changes
    }

    #[test]
    fn test_monthly_session_break() {
        let provider = MonthlySessionProvider;
        let bars = create_test_bars();

        let breaks = find_session_breaks(&bars, &provider);
        assert_eq!(breaks.len(), 1); // 1 month change (Jan -> Feb)

        assert_eq!(breaks[0].0, 2); // Break at index 2
        assert_eq!(breaks[0].1.break_type, SessionBreakType::Monthly);
    }

    #[test]
    fn test_composite_provider() {
        let mut composite = CompositeSessionProvider::new();
        composite.add_provider(Box::new(DailySessionProvider::default()));
        composite.add_provider(Box::new(MonthlySessionProvider));

        let bars = create_test_bars();
        let breaks = find_session_breaks(&bars, &composite);

        // Should detect both daily and monthly breaks
        assert!(!breaks.is_empty());
    }
}
