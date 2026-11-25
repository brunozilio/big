use std::fmt::Write as _;
use std::sync::Arc;

use agent_engine::tools::{self, Workspace};
use serde_json::Value;
use store::{Application, CodeChoice, Repository, Store};

pub const ADD_REPOSITORY: &str = domain::ToolName::AddRepository.as_str();

pub fn id_of(input: &Value) -> Option<String> {
    input["application"]["id"].as_str().filter(|id| !id.is_empty()).map(str::to_owned)
}

pub fn live(store: &Store, input: &Value) -> Value {
    let found = id_of(input).and_then(|id| store.application(&id).ok().flatten());
    let Some(application) = found else { return input.clone() };
    let Ok(repositories) = serde_json::to_value(&application.repositories) else { return input.clone() };
    let mut input = input.clone();
    input["application"]["repositories"] = repositories;
    input
}

pub fn offer(client: u64, store: &Arc<Store>, application: String) {
    let store = Arc::clone(store);
    tools::hosted::offer(
        client,
        ADD_REPOSITORY,
        Arc::new(move |input, workspace| save(&store, &application, input, workspace).map(Value::String)),
    );
}

#[derive(Debug, Default)]
struct Wanted {
    directory: String,
    repo: Option<String>,
    run: Option<Vec<String>>,
    verify: Option<Vec<String>>,
}

fn text(input: &Value, name: &str) -> Option<String> {
    input[name].as_str().map(str::trim).map(str::to_owned)
}

fn lines_of(found: &[String]) -> Vec<String> {
    found.iter().map(|line| line.trim()).filter(|line| !line.is_empty()).map(str::to_owned).collect()
}

fn lines(input: &Value, name: &str) -> Option<Vec<String>> {
    input[name].as_array().map(|lines| {
        lines
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()
    })
}

fn wanted(input: &Value, workspace: &Workspace) -> Result<Wanted, String> {
    let directory = text(input, "directory").filter(|dir| !dir.is_empty()).ok_or("Give the repository `directory`.")?;
    let directory = workspace.resolve(&directory);
    if !directory.is_dir() {
        return Err(format!("{} is not a folder.", directory.display()));
    }
    Ok(Wanted {
        directory: directory.to_string_lossy().into_owned(),
        repo: text(input, "repo").filter(|repo| !repo.is_empty()),
        run: text(input, "run").map(|line| lines_of(&[line])).or_else(|| lines(input, "run")),
        verify: lines(input, "verify"),
    })
}

fn connection(application: &Application) -> Option<String> {
    application.repositories.iter().find_map(|repository| repository.code.as_ref().map(|code| code.connection.clone()))
}

fn original(application: &Application, directory: &str) -> Option<String> {
    let path = std::path::Path::new(directory);
    let name = path.file_name()?.to_str()?;
    if path.parent()?.file_name()? != "worktrees" {
        return None;
    }
    let mine = |repository: &&Repository| name.ends_with(&format!("--{}", crate::git::folder_for(&repository.id)));
    application.repositories.iter().find(mine).map(|repository| repository.directory.clone())
}

fn apply(application: &mut Application, mut wanted: Wanted) -> bool {
    if let Some(directory) = original(application, &wanted.directory) {
        wanted.directory = directory;
    }
    let connection = connection(application);
    let at = application.repositories.iter().position(|repository| repository.directory == wanted.directory);
    let added = at.is_none();
    let index = at.unwrap_or_else(|| {
        let id = uuid::Uuid::new_v4().simple().to_string();
        application.repositories.push(Repository { id, directory: wanted.directory.clone(), ..Repository::default() });
        application.repositories.len() - 1
    });
    let Some(repository) = application.repositories.get_mut(index) else { return added };
    if let Some(repo) = wanted.repo {
        let connection = repository.code.as_ref().map(|code| code.connection.clone()).or(connection);
        repository.code = connection.map(|connection| CodeChoice { connection, repo });
    }
    if let Some(run) = wanted.run {
        repository.run = run;
    }
    if let Some(verify) = wanted.verify {
        repository.verify = verify;
    }
    added
}

fn described(application: &Application, directory: &str, added: bool) -> String {
    let verb = if added { "Added" } else { "Updated" };
    let mut text = format!("{verb} {directory} in the application {}.", application.name);
    let found = application.repositories.iter().find(|repository| repository.directory == directory);
    if let Some(repository) = found {
        if !repository.run.is_empty() {
            let _ = write!(text, "\nTo run: {}", repository.run.join(", "));
        }
        if !repository.verify.is_empty() {
            let _ = write!(text, "\nTo verify: {}", repository.verify.join(", "));
        }
    }
    text.push_str(
        "\nThe next steps of this task already get a worktree of it; in this step, work in the folder itself.",
    );
    text
}

fn save(store: &Store, application: &str, input: &Value, workspace: &Workspace) -> Result<String, String> {
    let wanted = wanted(input, workspace)?;
    let found = store.application(application).map_err(|error| error.to_string())?;
    let mut application = found.ok_or("The application of this task no longer exists.")?;
    let directory = original(&application, &wanted.directory).unwrap_or_else(|| wanted.directory.clone());
    let added = apply(&mut application, wanted);
    store.save_application(&application).map_err(|error| error.to_string())?;
    Ok(described(&application, &directory, added))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_s_worktree_updates_the_repository_it_was_made_from() {
        let mut application = Application {
            repositories: vec![Repository {
                id: "565e".into(),
                directory: "/code/demo".into(),
                ..Repository::default()
            }],
            ..Application::default()
        };
        let worktree = "/data/worktrees/big_92ec--565e";
        let wanted = Wanted { directory: worktree.into(), run: Some(vec!["npm start".into()]), ..Wanted::default() };

        let added = apply(&mut application, wanted);

        assert!(!added);
        assert_eq!(application.repositories.len(), 1);
        assert_eq!(application.repositories[0].directory, "/code/demo");
        assert_eq!(application.repositories[0].run, ["npm start"]);
    }
}
