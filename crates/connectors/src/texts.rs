use domain::Language;

pub const NO_SITE: &str = "Reconnect Jira and choose the site.";
pub const NO_PROJECT: &str = "Choose the project.";
pub const NO_SITES: &str =
    "No Jira site was found in this Atlassian account. Check that the account has access to Jira and connect again.";

pub fn connect(language: Language) -> &'static str {
    language.pick("Connect", "Conectar", "Conectar")
}

pub fn proceed(language: Language) -> &'static str {
    language.pick("Continue", "Continuar", "Continuar")
}

pub fn use_api_token(language: Language) -> &'static str {
    language.pick("Use an API token", "Usar un API token", "Usar API token")
}

pub fn missing_field(language: Language, label: &str) -> String {
    language.pick(format!("Enter the {label}."), format!("Indica el {label}."), format!("Informe o {label}."))
}

pub fn projects(language: Language, keys: &[String]) -> String {
    match keys {
        [] => language.pick("No project", "Ningún proyecto", "Nenhum projeto").to_owned(),
        [only] => language.pick(format!("Project {only}"), format!("Proyecto {only}"), format!("Projeto {only}")),
        many => {
            let joined = many.join(", ");
            language.pick(format!("Projects {joined}"), format!("Proyectos {joined}"), format!("Projetos {joined}"))
        }
    }
}

pub fn tools(language: Language, count: u64) -> String {
    match count {
        0 => language.pick("No tools", "Ninguna herramienta", "Nenhuma ferramenta").to_owned(),
        1 => language.pick("1 tool", "1 herramienta", "1 ferramenta").to_owned(),
        n => language.pick(format!("{n} tools"), format!("{n} herramientas"), format!("{n} ferramentas")),
    }
}

pub fn detail(language: Language, settings: &serde_json::Value) -> Option<String> {
    let shown = settings.get("shown")?;
    if let Some(keys) = shown["projects"].as_array() {
        let keys: Vec<String> = keys.iter().filter_map(serde_json::Value::as_str).map(str::to_owned).collect();
        return Some(projects(language, &keys));
    }
    let count = shown["tools"].as_u64()?;
    let host = shown["host"].as_str().unwrap_or_default();
    Some(format!("{} · {host}", tools(language, count)))
}

pub fn blocker(language: Language, reason: &str) -> String {
    let told = match reason {
        NO_SITE => language.pick(
            NO_SITE,
            "Vuelve a conectar Jira y elige el sitio.",
            "Reconecte o Jira e escolha o site.",
        ),
        NO_PROJECT => language.pick(NO_PROJECT, "Elige el proyecto.", "Escolha o projeto."),
        NO_SITES => language.pick(
            NO_SITES,
            "No se encontró ningún sitio de Jira en esta cuenta de Atlassian. Comprueba que la cuenta tiene acceso a Jira y vuelve a conectar.",
            "Nenhum site do Jira foi encontrado nesta conta Atlassian. Confira se a conta tem acesso ao Jira e conecte de novo.",
        ),
        other => other,
    };
    told.to_owned()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn details_and_blockers_read_in_the_chosen_language() {
        let keys = ["APP".to_owned(), "OPS".to_owned()];
        assert_eq!(projects(Language::English, &keys), "Projects APP, OPS");
        assert_eq!(projects(Language::Portuguese, &keys[..1]), "Projeto APP");
        assert_eq!(
            detail(Language::Spanish, &json!({ "shown": { "tools": 2, "host": "a.test" } })).as_deref(),
            Some("2 herramientas · a.test")
        );
        assert_eq!(detail(Language::English, &json!({ "detail": "x" })), None);
        assert_eq!(blocker(Language::English, NO_PROJECT), "Choose the project.");
        assert_eq!(blocker(Language::Portuguese, NO_SITE), "Reconecte o Jira e escolha o site.");
        assert_eq!(blocker(Language::English, "something else"), "something else");
    }
}
