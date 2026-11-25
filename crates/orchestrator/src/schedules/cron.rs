use jiff::civil::DateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cron {
    minutes: u64,
    hours: u64,
    days: u64,
    months: u64,
    weekdays: u64,
    any_day: bool,
    any_weekday: bool,
}

const FIELDS: [(u8, u8); 5] = [(0, 59), (0, 23), (1, 31), (1, 12), (0, 7)];

fn number(text: &str, low: u8, high: u8) -> Result<u8, String> {
    let value: u8 = text.parse().map_err(|_| format!("{text:?} is not a number"))?;
    if value < low || value > high {
        return Err(format!("{value} is outside {low}-{high}"));
    }
    Ok(value)
}

fn range(text: &str, low: u8, high: u8) -> Result<(u8, u8), String> {
    if text == "*" {
        return Ok((low, high));
    }
    match text.split_once('-') {
        Some((from, to)) => {
            let (from, to) = (number(from, low, high)?, number(to, low, high)?);
            if from > to { Err(format!("{from}-{to} goes backwards")) } else { Ok((from, to)) }
        }
        None => {
            let at = number(text, low, high)?;
            Ok((at, at))
        }
    }
}

fn part(text: &str, low: u8, high: u8) -> Result<u64, String> {
    let (span, every) = match text.split_once('/') {
        Some((span, every)) => (span, number(every, 1, high.max(1))?),
        None => (text, 1),
    };
    let (from, to) = range(span, low, high)?;
    let to = if every > 1 && span != "*" && !span.contains('-') { high } else { to };
    Ok((from..=to).step_by(usize::from(every)).fold(0, |bits, at| bits | (1 << at)))
}

fn field(text: &str, (low, high): (u8, u8)) -> Result<u64, String> {
    text.split(',').try_fold(0, |bits, piece| Ok(bits | part(piece, low, high)?))
}

impl Cron {
    pub fn parse(text: &str) -> Result<Cron, String> {
        let pieces: Vec<&str> = text.split_whitespace().collect();
        let [minute, hour, day, month, weekday] = pieces.as_slice() else {
            return Err(format!("{text:?} needs five fields: minute hour day month weekday"));
        };
        let weekdays = field(weekday, FIELDS[4])?;
        Ok(Cron {
            minutes: field(minute, FIELDS[0])?,
            hours: field(hour, FIELDS[1])?,
            days: field(day, FIELDS[2])?,
            months: field(month, FIELDS[3])?,
            weekdays: if weekdays & (1 << 7) == 0 { weekdays } else { weekdays | 1 },
            any_day: day.starts_with('*'),
            any_weekday: weekday.starts_with('*'),
        })
    }

    pub fn matches(&self, at: DateTime) -> bool {
        let has = |bits: u64, value: i8| u32::try_from(value).is_ok_and(|value| bits & (1 << value) != 0);
        let day = has(self.days, at.day());
        let weekday = has(self.weekdays, at.weekday().to_sunday_zero_offset());
        let calendar = match (self.any_day, self.any_weekday) {
            (true, true) => true,
            (true, false) => weekday,
            (false, true) => day,
            (false, false) => day || weekday,
        };
        calendar && has(self.minutes, at.minute()) && has(self.hours, at.hour()) && has(self.months, at.month())
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    fn at(day: i8, hour: i8, minute: i8) -> DateTime {
        date(2026, 11, day).at(hour, minute, 0, 0)
    }

    #[test]
    fn a_daily_time_matches_only_that_minute() {
        let cron = Cron::parse("30 8 * * *").expect("cron");
        assert!(cron.matches(at(24, 8, 30)));
        assert!(!cron.matches(at(24, 8, 31)));
        assert!(!cron.matches(at(24, 9, 30)));
    }

    #[test]
    fn weekdays_ranges_lists_and_steps_are_understood() {
        let cron = Cron::parse("0 9,18 * * 1-5").expect("cron");
        assert!(cron.matches(at(24, 18, 0)));
        assert!(!cron.matches(at(22, 9, 0)));
        let every = Cron::parse("*/15 * * * *").expect("cron");
        assert!(every.matches(at(24, 3, 45)));
        assert!(!every.matches(at(24, 3, 50)));
        let sunday = Cron::parse("0 0 * * 7").expect("cron");
        assert!(sunday.matches(at(22, 0, 0)));
    }

    #[test]
    fn day_of_month_or_weekday_when_both_are_restricted() {
        let cron = Cron::parse("0 6 1 * 2").expect("cron");
        assert!(cron.matches(at(24, 6, 0)));
        assert!(cron.matches(date(2026, 12, 1).at(6, 0, 0, 0)));
        assert!(!cron.matches(at(25, 6, 0)));
    }

    #[test]
    fn malformed_expressions_say_what_is_wrong() {
        assert!(Cron::parse("0 8 * *").is_err());
        assert!(Cron::parse("60 8 * * *").is_err());
        assert!(Cron::parse("0 9-8 * * *").is_err());
        assert!(Cron::parse("x 8 * * *").is_err());
    }
}
