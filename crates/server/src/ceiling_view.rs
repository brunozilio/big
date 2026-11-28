use domain::Language;
use domain::quota::Scope;
use orchestrator::ceiling::Reached;
use serde_json::{Value, json};

fn usd(amount: f64) -> String {
    format!("${amount:.2}")
}

fn whose(scope: Scope, language: Language) -> &'static str {
    match scope {
        Scope::Run => language.pick("Run", "Ejecución", "Execução"),
        Scope::Agent => language.pick("Agent", "Agente", "Agente"),
        Scope::Application => language.pick("Application", "Aplicación", "Aplicação"),
        Scope::Person => language.pick("Person", "Persona", "Pessoa"),
        Scope::Session | Scope::Item | Scope::Account | Scope::Workspace => {
            language.pick("Budget", "Presupuesto", "Orçamento")
        }
    }
}

pub fn headline(reached: &Reached, language: Language) -> String {
    let (spent, limit) = (usd(reached.spent_usd), usd(reached.limit_usd));
    let scope = whose(reached.scope, language);
    language.pick(
        format!("{scope} ceiling: {spent} of {limit}"),
        format!("Tope de {}: {spent} de {limit}", scope.to_lowercase()),
        format!("Teto de {}: {spent} de {limit}", scope.to_lowercase()),
    )
}

pub fn said(reached: &Reached) -> Value {
    json!({
        "key": format!("budget_{}", reached.scope.word()),
        "vars": { "spent": usd(reached.spent_usd), "limit": usd(reached.limit_usd) },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reached_ceiling_is_told_short_with_what_was_spent_and_the_ceiling() {
        let reached = Reached { scope: Scope::Run, spent_usd: 0.52, limit_usd: 0.5 };
        assert_eq!(headline(&reached, Language::Portuguese), "Teto de execução: $0.52 de $0.50");
        assert_eq!(headline(&reached, Language::English), "Run ceiling: $0.52 of $0.50");
        assert_eq!(said(&reached), json!({ "key": "budget_run", "vars": { "spent": "$0.52", "limit": "$0.50" } }));
    }
}
