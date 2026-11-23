use std::collections::HashSet;

use super::Standing;

#[derive(Debug, Default, Clone)]
pub struct Remembered {
    answers: HashSet<String>,
}

impl Remembered {
    fn answer(tool: &str, standing: Standing, subject: &str) -> String {
        match standing {
            Standing::Edits => tool.to_owned(),
            Standing::Reads | Standing::Acts | Standing::ReadsOutside | Standing::EditsGuarded => {
                format!("{tool}({subject})")
            }
        }
    }

    pub fn remember(&mut self, tool: &str, standing: Standing, subject: &str) {
        self.answers.insert(Self::answer(tool, standing, subject));
    }

    pub fn allows(&self, tool: &str, standing: Standing, subject: &str) -> bool {
        self.answers.contains(&Self::answer(tool, standing, subject))
    }
}
