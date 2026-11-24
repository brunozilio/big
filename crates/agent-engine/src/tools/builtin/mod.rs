pub mod code_review;
pub mod follow_through;
pub mod schedules;
pub mod shell;
pub mod web;

use crate::agent::Review;

pub fn reviewers() -> Vec<Box<dyn Review>> {
    [code_review::review::reviewers(), follow_through::review::reviewers()].into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reviewers_come_from_these_folders_in_this_order() {
        let folders = [
            ("code_review", code_review::review::reviewers().len()),
            ("follow_through", follow_through::review::reviewers().len()),
        ];
        assert_eq!(folders, [("code_review", 1), ("follow_through", 1)]);
        assert_eq!(reviewers().len(), 2);
    }
}
