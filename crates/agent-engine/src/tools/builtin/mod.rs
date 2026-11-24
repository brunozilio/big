pub mod code_review;
pub mod files;
pub mod follow_through;
pub mod schedules;
pub mod shell;
pub mod web;

use crate::agent::Review;
use crate::tools::Tool;

pub fn tools() -> Vec<Box<dyn Tool>> {
    [files::tools(), schedules::tools(), shell::tools(), web::tools()].into_iter().flatten().collect()
}

pub fn reviewers() -> Vec<Box<dyn Review>> {
    [code_review::review::reviewers(), follow_through::review::reviewers()].into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(tools: Vec<Box<dyn Tool>>) -> Vec<String> {
        tools.iter().map(|tool| tool.spec().name).collect()
    }

    #[test]
    fn each_tool_folder_registers_these_tools_in_this_order() {
        let folders = [
            ("files", names(files::tools())),
            ("schedules", names(schedules::tools())),
            ("shell", names(shell::tools())),
            ("web", names(web::tools())),
        ];
        let listed: Vec<String> =
            folders.iter().map(|(folder, tools)| format!("{folder}: {}", tools.join(" "))).collect();
        assert_eq!(
            listed,
            [
                "files: Read Write Edit Glob Grep NotebookEdit",
                "schedules: ScheduleWakeup",
                "shell: Bash BashOutput KillShell Monitor",
                "web: WebFetch WebSearch",
            ]
        );
        assert_eq!(names(tools()), folders.into_iter().flat_map(|(_, tools)| tools).collect::<Vec<String>>());
    }

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
