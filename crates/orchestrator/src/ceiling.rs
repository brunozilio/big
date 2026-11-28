use domain::quota::Scope;

pub const BUDGET_REACHED: &str = "cost ceiling reached: ";

#[derive(Debug, Clone, PartialEq)]
pub struct Reached {
    pub scope: Scope,
    pub spent_usd: f64,
    pub limit_usd: f64,
}

fn usd(micros: u64) -> f64 {
    micros as f64 / 1_000_000.0
}

pub fn reached(scope: Scope, counted_micros: u64, limit_micros: u64) -> String {
    format!("{BUDGET_REACHED}{} spent ${:.2} of ${:.2}", scope.word(), usd(counted_micros), usd(limit_micros))
}

pub fn budget_reached(reason: &str) -> Option<Reached> {
    let rest = reason.strip_prefix(BUDGET_REACHED)?;
    let (scope, amounts) = rest.split_once(" spent $")?;
    let (spent, limit) = amounts.split_once(" of $")?;
    Some(Reached {
        scope: Scope::named(scope)?,
        spent_usd: spent.trim().parse().ok()?,
        limit_usd: limit.trim().parse().ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reached_ceiling_reads_back_from_the_blocked_reason() {
        let reason = reached(Scope::Run, 520_000, 500_000);
        assert_eq!(reason, "cost ceiling reached: run spent $0.52 of $0.50");
        assert_eq!(budget_reached(&reason), Some(Reached { scope: Scope::Run, spent_usd: 0.52, limit_usd: 0.5 }));
        let person = budget_reached(&reached(Scope::Person, 31_400_000, 30_000_000)).expect("person");
        assert_eq!((person.scope, person.limit_usd), (Scope::Person, 30.0));
        assert_eq!(budget_reached("connect an account for tracker"), None);
        assert_eq!(budget_reached("cost ceiling reached: galaxy spent $1 of $2"), None);
    }
}
