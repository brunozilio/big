mod cleanup;
mod filters;
mod invoked;
mod limit;
mod savings;

pub use cleanup::{PLAIN_ENV, clean};
pub use filters::{Filtered, filter};
pub use invoked::program;
pub use limit::{CUT_NOTE, DEFAULT_LIMIT, LARGEST_LIMIT, OutputLimit, SMALLEST_LIMIT};
pub use savings::{Saving, report};

use brand::features as flags;

use crate::features::on;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Shaping {
    pub cleanup: bool,
    pub filters: bool,
    pub savings: bool,
}

impl Shaping {
    pub fn configured() -> Self {
        Self { cleanup: on(flags::OUTPUT_CLEANUP), filters: on(flags::OUTPUT_FILTERS), savings: on(flags::OUTPUT_SAVINGS) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_step_follows_its_own_flag_in_the_config() {
        assert_eq!(Shaping::default(), Shaping { cleanup: false, filters: false, savings: false });
        let configured = Shaping::configured();
        assert_eq!(configured.cleanup, on(flags::OUTPUT_CLEANUP));
        assert_eq!(configured.filters, on(flags::OUTPUT_FILTERS));
        assert_eq!(configured.savings, on(flags::OUTPUT_SAVINGS));
    }
}
