use domain::permission::hard::Category;

use super::*;

fn shipped(told: &Told<'_>) -> String {
    told_in(None, told)
}

#[test]
fn a_chat_refusal_tells_the_model_what_the_user_said() {
    let declined = Told::new(AskOutcome::Declined, Surface::Chat, "Bash");
    assert_eq!(
        shipped(&declined),
        "The user did not allow Bash here. Do not retry it; ask them or take another approach."
    );
    assert_eq!(
        shipped(&declined.note("use notes.txt instead")),
        "The user did not allow Bash here and said what to do instead: use notes.txt instead"
    );
    let workflow = Told::new(AskOutcome::Declined, Surface::Workflow, "x_tracker_link");
    assert_eq!(shipped(&workflow), "x_tracker_link was refused by the person asked.");
}

#[test]
fn an_ask_whose_wait_ended_without_an_answer_expires() {
    let chat = shipped(&Told::new(AskOutcome::Expired, Surface::Chat, "Bash"));
    assert!(chat.contains("waiting for the user's answer") && chat.contains("Bash did not run"), "{chat}");
    let workflow = shipped(&Told::new(AskOutcome::Expired, Surface::Workflow, "x_tracker_link"));
    assert_eq!(workflow, "x_tracker_link was waiting for a person, and the item stopped first.");
}

#[test]
fn a_policy_block_passes_its_reason_through() {
    let told = Told::new(AskOutcome::BlockedByPolicy, Surface::Mcp, "x").why("x is denied by the permission list.");
    assert_eq!(shipped(&told), "x is denied by the permission list.");
    let floor = Reason::Hard(Category::Hidden);
    let floored = shipped(&Told::new(AskOutcome::BlockedByPolicy, Surface::Routine, "x").reason(&floor));
    assert!(floored.contains("safety floor") && floored.contains("no one to ask"), "{floored}");
}

#[test]
fn with_no_one_to_ask_the_run_is_told_it_is_unavailable() {
    let routine = shipped(&Told::new(AskOutcome::Unavailable, Surface::Routine, "x_tracker_link"));
    assert_eq!(routine, "x_tracker_link asks a person first, and routine cannot ask anyone yet.");
    let chat = shipped(&Told::new(AskOutcome::Unavailable, Surface::Chat, "Bash"));
    assert_eq!(chat, "Bash requires permission; dontAsk denies unapproved actions.");
    let declared = Reason::Declared { why: "sends outside".into(), overrode: false };
    let strict = shipped(&Told::new(AskOutcome::Unavailable, Surface::Chat, "mail_send").reason(&declared));
    assert_eq!(strict, "mail_send sends outside, so a person approves it every time; it does not run unattended.");
    let flow = Reason::Flow("carries internal data".into());
    let flowed = shipped(&Told::new(AskOutcome::Unavailable, Surface::Mcp, "x").reason(&flow));
    assert_eq!(flowed, "x carries internal data, so a person approves it first, and this run has no one to ask.");
}

#[test]
fn an_operation_declares_its_own_conduct_and_falls_back_for_the_rest() {
    let folder = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let own = folder.path().join(FOLDER);
    std::fs::create_dir_all(&own).unwrap_or_else(|error| panic!("{error}"));
    let script =
        "export default (ctx) => ctx.outcome === 'declined' ? `Leave ${ctx.operation} to the release team.` : null;";
    std::fs::write(own.join("code.merge.js"), script).unwrap_or_else(|error| panic!("{error}"));
    let merge = Told::new(AskOutcome::Declined, Surface::Workflow, "x_code_merge").operation("code.merge");
    assert_eq!(told_in(Some(folder.path()), &merge), "Leave code.merge to the release team.");
    let expired = Told::new(AskOutcome::Expired, Surface::Workflow, "x_code_merge").operation("code.merge");
    assert!(told_in(Some(folder.path()), &expired).contains("the item stopped first"));
    let other = Told::new(AskOutcome::Declined, Surface::Workflow, "x").operation("tracker.link");
    assert_eq!(told_in(Some(folder.path()), &other), "x was refused by the person asked.");
}

#[test]
fn a_broken_conduct_falls_back_and_an_odd_operation_name_is_never_a_path() {
    let folder = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let prompts = folder.path().join("prompts");
    std::fs::create_dir_all(&prompts).unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(prompts.join("outcomes.js"), "export default () => { throw new Error('mine') };")
        .unwrap_or_else(|error| panic!("{error}"));
    let told = Told::new(AskOutcome::Declined, Surface::Workflow, "x");
    assert_eq!(told_in(Some(folder.path()), &told), "x was refused by the person asked.");
    for odd in ["../outcomes", ".hidden", "a/b", ""] {
        assert_eq!(file_of(odd), None, "{odd}");
    }
    assert_eq!(file_of("code.merge").as_deref(), Some("prompts/outcomes/code.merge.js"));
}
