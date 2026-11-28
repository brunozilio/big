use domain::Language;
use domain::workflow::ItemId;

use crate::shared::Shared;

pub fn of_item(shared: &Shared, id: &ItemId) -> Language {
    shared.item(id).map(|item| crate::agent::language_of(shared, &item.input)).unwrap_or_default()
}

pub fn waiting_answer(language: Language) -> &'static str {
    language.pick(
        "the agent is waiting for an answer",
        "el agente espera una respuesta",
        "o agente espera uma resposta",
    )
}

pub fn is_waiting_answer(reason: Option<&str>) -> bool {
    Language::ALL.into_iter().any(|language| reason == Some(waiting_answer(language)))
}

pub fn usage_limit(language: Language, message: &str) -> String {
    language.pick(
        format!("the AI provider hit its usage limit: {message}"),
        format!("el proveedor de IA alcanzó su límite de uso: {message}"),
        format!("o provedor de IA atingiu o limite de uso: {message}"),
    )
}

pub fn not_loaded(language: Language, reason: &str) -> String {
    language.pick(
        format!("the workflow could not be loaded: {reason}"),
        format!("no se pudo cargar el workflow: {reason}"),
        format!("o workflow não pôde ser carregado: {reason}"),
    )
}

pub fn failed(language: Language, message: &str) -> String {
    language.pick(
        format!("the workflow failed: {message}"),
        format!("el workflow falló: {message}"),
        format!("o workflow falhou: {message}"),
    )
}

fn changed(language: Language) -> &'static str {
    language.pick(
        "the workflow code changed and does not replay the history",
        "el código del workflow cambió y no reproduce el historial",
        "o código do workflow mudou e não reproduz o histórico",
    )
}

pub fn workflow_changed(language: Language, message: &str) -> String {
    format!("{}: {message}", changed(language))
}

pub fn is_workflow_changed(reason: &str) -> bool {
    Language::ALL.into_iter().any(|language| reason.starts_with(changed(language)))
}

pub fn parent_stopped_waiting(language: Language) -> &'static str {
    language.pick("the parent no longer waits", "el padre ya no espera", "o pai não espera mais")
}

pub fn parent_finished(language: Language) -> &'static str {
    language.pick("the parent finished", "el padre terminó", "o pai terminou")
}

pub fn cancelled_by_watcher(language: Language) -> &'static str {
    language.pick(
        "cancelled by whoever follows the item",
        "cancelado por quien sigue el ítem",
        "cancelado por quem acompanha o item",
    )
}

pub fn killed(language: Language) -> &'static str {
    language.pick(
        "stopped at once by the kill switch",
        "detenido al instante por el corte",
        "parado na hora pelo corte",
    )
}

pub fn escaped(language: Language, gate: &str, step: &str, workflow: &str) -> String {
    language.pick(
        format!("Evaluation: {gate} let a {step} defect through ({workflow})"),
        format!("Evaluación: {gate} dejó pasar un defecto de {step} ({workflow})"),
        format!("Avaliação: {gate} deixou passar um defeito de {step} ({workflow})"),
    )
}

pub fn recurring(language: Language, step: &str, count: u64, workflow: &str) -> String {
    language.pick(
        format!("Factory: {step} failed {count} times ({workflow})"),
        format!("Fábrica: {step} falló {count} veces ({workflow})"),
        format!("Fábrica: {step} falhou {count} vezes ({workflow})"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_text_reads_in_the_chosen_language_and_a_marker_is_known_in_any() {
        assert_eq!(failed(Language::English, "x"), "the workflow failed: x");
        assert_eq!(failed(Language::Portuguese, "x"), "o workflow falhou: x");
        assert_eq!(recurring(Language::Spanish, "develop", 3, "default"), "Fábrica: develop falló 3 veces (default)");
        for language in Language::ALL {
            assert!(is_waiting_answer(Some(waiting_answer(language))));
            assert!(is_workflow_changed(&workflow_changed(language, "seq 3")));
        }
        assert!(!is_waiting_answer(Some("other")) && !is_waiting_answer(None));
    }
}
