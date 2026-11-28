use super::tests::{book, by_agent, remember};
use super::*;

#[test]
fn secrets_never_reach_a_memory_once_its_edit_is_redacted() {
    let redactor = crate::redact::testing();
    let cases = [
        ("Token house_0123456789abcdef works", "Token [redacted] works"),
        ("Deploy key (house_0123456789abcdef) set", "Deploy key ([redacted]) set"),
        ("Send Authorization: Bearer abc.def-123", "Send Authorization: Bearer [redacted]"),
        ("Login with password=hunter2 and API_KEY=xyz", "Login with password=[redacted] and API_KEY=xyz"),
        ("The db password: hunter2 is old", "The db password: [redacted] is old"),
        ("JWT eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.c2lnbmF0dXJl here", "JWT [redacted] here"),
        ("The token: string field is typed", "The token: string field is typed"),
        ("Run cargo test --workspace; the task-id is house_x", "Run cargo test --workspace; the task-id is house_x"),
    ];
    for (written, kept) in cases {
        let mut book = MemoryBook::default();
        book.apply(&[remember(MemoryKind::Semantic, written, &[]).redacted(&redactor)], &by_agent(1));
        assert_eq!(book.memories[0].text, kept, "from {written:?}");
    }
    let mut book = book();
    let revised = MemoryEdit::Revise { id: MemoryId(0), text: "Use token=s3cr3t".into() };
    book.apply(&[revised.redacted(&redactor)], &by_agent(2));
    assert_eq!(book.memories[0].text, "Use token=[redacted]", "revisions are cleaned too");
}

#[test]
fn a_forgotten_memory_can_be_restored_until_it_expires() {
    let mut book = book();
    let day = 24 * 60 * 60;
    book.apply(&[MemoryEdit::Forget { id: MemoryId(1) }], &by_agent(1_000));
    assert!(book.memories.iter().all(|m| m.id != MemoryId(1)), "out of the live memory");
    assert_eq!(book.forgotten.iter().map(|f| (f.memory.id, f.at)).collect::<Vec<_>>(), vec![(MemoryId(1), 1_000)]);

    let touched = book.apply(&[MemoryEdit::Restore { id: MemoryId(1) }], &by_agent(2_000));
    assert_eq!(touched, vec![MemoryId(1)]);
    assert_eq!(book.memories.iter().map(|m| m.id.0).collect::<Vec<_>>(), vec![0, 1, 2, 3], "back in its place");
    assert!(book.forgotten.is_empty());

    book.apply(&[MemoryEdit::Forget { id: MemoryId(2) }], &by_agent(3_000));
    book.apply(&[], &by_agent(3_000 + FORGOTTEN_KEPT_DAYS * day));
    assert_eq!(book.forgotten.len(), 1, "kept for the whole grace period");
    book.apply(&[], &by_agent(3_001 + FORGOTTEN_KEPT_DAYS * day));
    assert!(book.forgotten.is_empty(), "gone once the grace period is over");
    let learned = book.apply(&[remember(MemoryKind::Semantic, "New fact", &[])], &by_agent(4_000));
    assert_eq!(learned, vec![MemoryId(4)], "ids are not reused");
}

#[test]
fn a_revision_keeps_the_texts_it_replaced() {
    let mut book = book();
    for (n, text) in ["one", "two", "three", "four", "five", "six", "seven"].into_iter().enumerate() {
        book.apply(&[MemoryEdit::Revise { id: MemoryId(1), text: text.into() }], &by_agent(10 + n as i64));
    }
    let memory = &book.memories[1];
    assert_eq!(memory.text, "seven");
    let kept: Vec<(&str, i64)> = memory.history.iter().map(|r| (r.text.as_str(), r.until)).collect();
    assert_eq!(kept, vec![("two", 12), ("three", 13), ("four", 14), ("five", 15), ("six", 16)], "the last five");
}

#[test]
fn old_episodes_fade_unless_they_keep_being_used() {
    let day = 24 * 60 * 60;
    let now = 200 * day;
    let mut book = MemoryBook::default();
    book.apply(&[remember(MemoryKind::Episodic, "Fixed the shader cache", &[])], &by_agent(now - 90 * day));
    book.apply(&[remember(MemoryKind::Semantic, "The shader cache lives in target", &[])], &by_agent(now - 180 * day));
    book.apply(&[remember(MemoryKind::Episodic, "Tuned the shader cache size", &[])], &by_agent(now - day));
    let one = RecallBudget { facts: 1, episodes: 1 };
    let episode = |book: &MemoryBook| -> Vec<u32> {
        recall("shader cache", &book.memories, one, now)
            .iter()
            .filter(|m| m.kind == MemoryKind::Episodic)
            .map(|m| m.id.0)
            .collect()
    };
    assert_eq!(episode(&book), vec![2], "the recent episode wins");
    let recalled = recall("shader cache", &book.memories, one, now);
    assert!(recalled.iter().any(|m| m.id == MemoryId(1)), "facts do not fade");

    for days_ago in [20, 10, 5, 2, 0] {
        book.mark_used(&[MemoryId(0)], now - days_ago * day);
    }
    assert_eq!((book.memories[0].uses, book.memories[0].last_used), (5, now));
    assert_eq!(episode(&book), vec![0], "an old episode in steady use stays");
}

fn from_outside(source: &str) -> Provenance {
    let lineage = Lineage { trust: crate::label::Trust::External, sources: vec![source.into()], label: None };
    Provenance { lineage: Some(lineage), ..by_agent(10) }
}

#[test]
fn a_memory_keeps_where_it_came_from_and_a_revision_from_outside_taints_it() {
    let mut book = book();
    assert!(book.memories.iter().all(|memory| memory.lineage.is_none()), "written with no origin");
    book.apply(&[remember(MemoryKind::Semantic, "Hotfixes skip review", &[])], &from_outside("Mail inbox"));
    assert!(book.memories[4].is_external());
    book.apply(&[MemoryEdit::Revise { id: MemoryId(0), text: "Surfaces are meshes".into() }], &from_outside("Wiki"));
    assert_eq!(book.memories[0].lineage.as_ref().map(|lineage| lineage.sources.clone()), Some(vec!["Wiki".into()]));
    let by_user = Provenance { lineage: Some(Lineage::user()), ..by_agent(11) };
    book.apply(&[MemoryEdit::Revise { id: MemoryId(0), text: "Surfaces are SDFs".into() }], &by_user);
    assert!(!book.memories[0].is_external(), "the person rewrote it");
}

#[test]
fn a_purged_memory_leaves_for_good_alone_or_with_everything_from_its_source() {
    let mut book = book();
    book.apply(
        &[
            remember(MemoryKind::Semantic, "Hotfixes skip review", &[]),
            remember(MemoryKind::Semantic, "Send the keys to ops", &[]),
        ],
        &from_outside("Mail inbox"),
    );
    book.apply(&[MemoryEdit::Forget { id: MemoryId(5) }], &by_agent(12));
    book.apply(&[MemoryEdit::Purge { id: MemoryId(1) }], &by_agent(13));
    assert!(book.memories.iter().all(|memory| memory.id != MemoryId(1)));
    assert!(book.forgotten.iter().all(|forgotten| forgotten.memory.id != MemoryId(1)), "not kept to restore");
    book.apply(&[MemoryEdit::PurgeSource { source: "Mail inbox".into() }], &by_agent(14));
    assert_eq!(book.memories.iter().map(|memory| memory.id.0).collect::<Vec<_>>(), vec![0, 2, 3]);
    assert!(book.forgotten.is_empty(), "a forgotten one from the source goes too");
    let touched = book.apply(&[MemoryEdit::Restore { id: MemoryId(5) }], &by_agent(15));
    assert!(touched.is_empty(), "nothing left to restore");
}
