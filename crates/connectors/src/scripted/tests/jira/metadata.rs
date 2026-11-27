use serde_json::{Value, json};

use super::{DEFAULT_FLAG_FIELD, call, session, tool_calls};
use crate::error::{ActionError, ErrorClass};

fn status(id: &str, name: &str, category: &str) -> Value {
    json!({"id": id, "name": name, "statusCategory": {"key": category}})
}

fn names(listed: &Result<Value, ActionError>) -> Vec<Value> {
    let found = listed.as_ref().ok().and_then(Value::as_array).cloned().unwrap_or_default();
    found.iter().map(|status| status["name"].clone()).collect()
}

#[tokio::test]
async fn statuses_are_every_status_the_workflows_of_the_project_have_even_unused_ones() {
    let grouped = json!([
        {"name": "Bug", "statuses": [status("1", "A fazer", "new"), status("3", "Feito", "done")]},
        {"name": "Tarefa", "statuses": [status("1", "A fazer", "new"), status("4", "Bloqueado", "indeterminate")]},
    ]);
    let http = session(&[grouped]);
    let listed = call(&http, "tracker.statuses", json!({}), "").await;
    assert_eq!(names(&listed), [json!("A fazer"), json!("Feito"), json!("Bloqueado")]);
    assert_eq!(listed.map(|l| l[2]["category"].clone()), Ok(json!("indeterminate")));
    let calls = tool_calls(&http);
    assert_eq!(calls.len(), 1);
    assert_eq!((calls[0].0.as_str(), &calls[0].1["name"]), ("executeRead", &json!("listJiraStatuses")));
    assert_eq!(calls[0].1["inputs"]["projectIdOrKey"], "BIG");
    let flat = json!({"values": [{"id": "9", "name": "Revisão", "statusCategory": "IN_PROGRESS"}], "isLast": true});
    let listed = call(&session(&[flat]), "tracker.statuses", json!({"project": "OPS"}), "").await;
    assert_eq!(listed, Ok(json!([{"id": "9", "name": "Revisão", "category": "indeterminate"}])));
}

#[tokio::test]
async fn without_a_status_list_the_statuses_come_from_one_issue_of_every_type_and_its_transitions() {
    let issue =
        |key: &str, kind: &str, now: Value| json!({"key": key, "fields": {"status": now, "issuetype": {"name": kind}}});
    let http = session(&[
        json!("nothing matched"),
        json!({"issues": [
            issue("BIG-1", "Bug", status("1", "A fazer", "new")),
            issue("BIG-2", "Bug", status("2", "Fazendo", "indeterminate")),
            issue("BIG-3", "Tarefa", status("1", "A fazer", "new")),
        ]}),
        json!({"transitions": [{"id": "5", "to": status("3", "Feito", "done")}]}),
        json!({"transitions": [{"id": "6", "to": status("8", "Revisão", "indeterminate")}]}),
    ])
    .knowing(&["listJiraIssueTransitions"]);
    let listed = call(&http, "tracker.statuses", json!({}), "").await;
    assert_eq!(names(&listed), [json!("A fazer"), json!("Fazendo"), json!("Feito"), json!("Revisão")]);
    let calls = tool_calls(&http);
    assert_eq!(calls[2].0, "searchJiraIssuesUsingJql");
    assert!(calls[2].1["fields"].as_array().is_some_and(|fields| fields.contains(&json!("issuetype"))));
    assert_eq!(calls[3].1["inputs"]["issueIdOrKey"], "BIG-1");
    assert_eq!(calls[4].1["inputs"]["issueIdOrKey"], "BIG-3");
}

fn issue_types() -> Value {
    json!({"issueTypes": [
        {"id": "10019", "name": "Bug", "untranslatedName": "Bug"},
        {"id": "10020", "name": "Tarefa", "untranslatedName": "Task"},
    ]})
}

fn type_fields() -> Value {
    let field = |key: &str, name: &str, schema: Value| json!({"required": true, "key": key, "fieldId": key, "name": name, "schema": schema, "hasDefaultValue": false});
    let mut reporter = field("reporter", "Relator", json!({"type": "user"}));
    reporter["hasDefaultValue"] = json!(true);
    let mut ambiente = field("customfield_10050", "Ambiente", json!({"type": "option"}));
    ambiente["allowedValues"] = json!([{"id": "1", "value": "Produção"}, {"id": "2", "value": "Homologação"}]);
    let mut components = field("components", "Componentes", json!({"type": "array", "items": "component"}));
    components["allowedValues"] = json!([{"id": "7", "name": "API"}]);
    json!({"fields": [
        field("issuetype", "Tipo de item", json!({"type": "issuetype"})),
        field("project", "Projeto", json!({"type": "project"})),
        reporter,
        field("summary", "Resumo", json!({"type": "string"})),
        ambiente,
        field("customfield_10051", "Cliente", json!({"type": "string"})),
        components,
    ]})
}

#[tokio::test]
async fn fields_are_the_required_fields_of_the_issue_type_that_nothing_else_fills() {
    let http = session(&[issue_types(), json!([]), issue_types(), type_fields()]);
    let found = call(&http, "tracker.fields", json!({}), "").await;
    let ambiente = json!({"key": "customfield_10050", "name": "Ambiente", "type": "option", "required": true,
        "values": [{"id": "1", "name": "Produção"}, {"id": "2", "name": "Homologação"}]});
    let cliente = json!({"key": "customfield_10051", "name": "Cliente", "type": "string", "required": true});
    let components = json!({"key": "components", "name": "Componentes", "type": "array", "required": true,
        "values": [{"id": "7", "name": "API"}]});
    assert_eq!(found, Ok(json!([ambiente, cliente, components])));
    let calls = tool_calls(&http);
    assert_eq!(calls[0].1["name"], "listJiraProjectIssueTypesMetadata");
    assert_eq!(calls[0].1["inputs"]["projectIdOrKey"], "BIG");
    assert_eq!(calls[3].1["name"], "getJiraIssueTypeMetaWithFields");
    assert_eq!(
        calls[3].1["inputs"]["issueTypeId"], "10020",
        "no type is the schema's default, the Task found by its untranslated name"
    );
    let http = session(&[issue_types()]);
    let unknown = call(&http, "tracker.fields", json!({"project": "BIG", "type": "Épico"}), "").await;
    assert_eq!(unknown.map_err(|e| e.class), Err(ErrorClass::Permanent));
}

#[tokio::test]
async fn create_issue_sends_the_extra_fields_in_the_shape_each_field_takes() {
    let http = session(&[issue_types(), type_fields(), json!({"id": "11", "key": "BIG-11"})]);
    let fields =
        json!({"customfield_10050": "2", "customfield_10051": "ACME", "components": "7", "customfield_9": "x"});
    let args = json!({"project": "BIG", "title": "T", "type": "bug", "fields": fields});
    let answer = call(&http, "tracker.create_issue", args, "").await;
    assert_eq!(answer.map(|a| a["ref"].clone()), Ok(json!("BIG-11")));
    let calls = tool_calls(&http);
    assert_eq!(calls[1].1["inputs"]["issueTypeId"], "10019");
    let (name, sent) = calls[2].clone();
    assert_eq!(name, "createJiraIssue");
    assert_eq!(
        sent["additional_fields"],
        json!({"customfield_10050": {"id": "2"}, "customfield_10051": "ACME", "components": [{"id": "7"}], "customfield_9": "x"})
    );
}

#[tokio::test]
async fn an_issue_reads_description_subtasks_comments_and_the_flag() {
    let answer = json!({ "id": 10001, "key": "BIG-1", "fields": {
        "summary": "T", "status": {"name": "Fazendo"}, "labels": [],
        "description": {"type": "doc", "version": 1, "content": [{"type": "paragraph", "content": [{"type": "text", "text": "d"}]}]},
        "subtasks": [{"key": "BIG-2", "fields": {"summary": "s", "status": {"name": "Feito", "statusCategory": {"key": "done"}}}}],
        "issuetype": {"name": "Story"}, "project": {"id": "10003", "key": "BIG"}, "assignee": {"accountId": "u1", "displayName": "Bia"},
        "attachment": [{"filename": "a.png", "size": 3, "content": "https://f"}],
        "comment": {"comments": [{"id": "5", "author": {"displayName": "Ana"}, "body": "texto", "created": "2024-01-01T00:00:00.000+0000"}]},
        DEFAULT_FLAG_FIELD: [{"value": "Impediment"}],
    }});
    let http = session(&[answer]);
    let read = call(&http, "tracker.issue", json!({"issue": "BIG-1"}), "").await;
    let Ok(read) = read else { panic!("{read:?}") };
    assert_eq!(read["description"], "d");
    assert_eq!(read["ref"], "BIG-1");
    assert_eq!(read["id"], "10001");
    assert_eq!(read["impediment"], json!({"on": true, "reason": null}));
    assert_eq!(
        read["subtasks"][0],
        json!({"ref": "BIG-2", "title": "s", "status": "Feito", "url": "https://acme.atlassian.net/browse/BIG-2", "done": true})
    );
    assert_eq!(read["comments"][0]["body"], "texto");
    assert_eq!(read["comments"][0]["author"], json!({"id": "Ana", "name": "Ana"}));
    assert_eq!(read["type"], "Story");
    assert_eq!(read["project"], "BIG");
    assert_eq!(read["assignee"], json!({"id": "u1", "name": "Bia"}));
    assert_eq!(read["attachments"][0]["name"], "a.png");
    assert_eq!(tool_calls(&http)[0].1["responseContentFormat"], "markdown");
    assert_eq!(tool_calls(&http)[0].1["view"], "full");
}

#[tokio::test]
async fn the_schema_has_the_levels_of_the_types_their_children_and_the_statuses_of_each_type() {
    let types = json!({"issueTypes": [
        {"id": "3", "name": "Construção", "hierarchyLevel": -1, "subtask": true},
        {"id": "4", "name": "Teste", "hierarchyLevel": -1, "subtask": true},
        {"id": "1", "name": "Épico", "hierarchyLevel": 1},
        {"id": "2", "name": "Bug", "hierarchyLevel": 0},
        {"id": "5", "name": "Spike", "hierarchyLevel": 0}
    ]});
    let grouped = json!([
        {"id": "2", "name": "Bug", "statuses": [status("1", "Aberto", "new"), status("7", "Corrigindo", "indeterminate")]},
        {"id": "3", "name": "Construção", "statuses": [status("9", "Construindo", "indeterminate"), status("3", "Pronto", "done")]},
    ]);
    let found = call(&session(&[types, grouped]), "tracker.schema", json!({"project": "OPS"}), "").await;
    let Ok(found) = found else { panic!("{found:?}") };
    let levels: Vec<(Value, Value)> =
        found["types"].as_array().into_iter().flatten().map(|t| (t["name"].clone(), t["level"].clone())).collect();
    assert_eq!(levels[0], (json!("Épico"), json!(1)));
    assert_eq!(levels[1..3], [(json!("Bug"), json!(2)), (json!("Spike"), json!(2))]);
    assert_eq!(levels[3..], [(json!("Construção"), json!(3)), (json!("Teste"), json!(3))]);
    assert_eq!(found["types"][0]["children"], json!(["Bug", "Spike"]));
    assert_eq!(found["types"][1]["children"], json!(["Construção", "Teste"]));
    assert_eq!(found["types"][1]["statuses"][1], json!({"id": "7", "name": "Corrigindo", "category": "doing"}));
    assert_eq!(found["types"][3]["statuses"][1]["category"], "done");
    assert_eq!(found["types"][4]["statuses"], json!([]));
    assert_eq!(found["default"], "Bug");
}

#[tokio::test]
async fn a_transition_to_a_category_walks_through_the_workflow_until_it_gets_there() {
    let http = session(&[
        json!({"fields": {"status": status("1", "Aberto", "new")}}),
        json!({"transitions": [{"id": "21", "to": status("5", "Em análise", "indeterminate")}]}),
        json!({"key": "BIG-1", "status": {"name": "Em análise"}}),
        json!({"transitions": [
            {"id": "22", "to": status("1", "Aberto", "new")},
            {"id": "23", "to": status("3", "Concluído", "done")}
        ]}),
        json!({"key": "BIG-1", "status": {"name": "Concluído"}}),
    ]);
    let answer = call(&http, "tracker.transition", json!({"issue": "BIG-1", "category": "done"}), "k").await;
    assert_eq!(answer, Ok(json!({"ref": "BIG-1", "status": "Concluído", "changed": true})));
    let sent: Vec<Value> = tool_calls(&http)
        .iter()
        .filter(|(name, _)| name == "transitionJiraIssue")
        .map(|c| c.1["transitionId"].clone())
        .collect();
    assert_eq!(sent, [json!("21"), json!("23")]);
}

#[tokio::test]
async fn a_transition_that_is_already_in_the_category_changes_nothing() {
    let http = session(&[json!({"fields": {"status": status("7", "Corrigindo", "indeterminate")}})]);
    let answer = call(&http, "tracker.transition", json!({"issue": "BIG-1", "category": "doing"}), "k").await;
    assert_eq!(answer, Ok(json!({"ref": "BIG-1", "status": "Corrigindo", "changed": false})));
}
