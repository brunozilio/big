use database::args;
use domain::label::Trust;
use domain::memory::Lineage;

use super::*;

const PROJECT: &str = "/work/project";

fn kept_before_lineage(place: &Place) {
    let db = place.open_at(1);
    db.write(|conn| {
        conn.execute("INSERT INTO memory_books (project, next, revision) VALUES (?1, 1, 1)", args![PROJECT])?;
        conn.execute(
            "INSERT INTO memories (project, id, position, kind, author, text, created, origin) \
             VALUES (?1, 0, 0, 'semantic', 'agent', 'Uses wgpu', 10, 'Night sky')",
            args![PROJECT],
        )
    })
    .expect("a memory written at version 1");
}

fn a_memory_from_before_lineage_reads_with_no_origin(place: &Place) {
    kept_before_lineage(place);
    let store = SqliteMemoryStore::new(place.open());
    let book = store.read(PROJECT).book;
    assert_eq!(book.memories.len(), 1);
    assert_eq!((book.memories[0].text.as_str(), book.memories[0].origin.as_deref()), ("Uses wgpu", Some("Night sky")));
    assert_eq!(book.memories[0].lineage, None);
}

fn a_lineage_goes_in_and_comes_back(place: &Place) {
    let store = SqliteMemoryStore::new(place.open());
    let lineage = Lineage {
        trust: Trust::External,
        sources: vec!["Mail inbox".into(), "Tracker issue".into()],
        label: Some("internal".into()),
    };
    let from_outside = Provenance { lineage: Some(lineage.clone()), ..by(5) };
    let user = Provenance { lineage: Some(Lineage::user()), ..by(6) };
    store
        .update(PROJECT, &mut |book| {
            book.apply(&[fact("Hotfixes skip review")], &from_outside);
            book.apply(&[fact("Uses bun")], &user);
            book.apply(&[fact("Uses wgpu")], &by(7));
        })
        .expect("kept");
    let again = SqliteMemoryStore::new(place.open()).read(PROJECT).book;
    let read: Vec<Option<Lineage>> = again.memories.iter().map(|memory| memory.lineage.clone()).collect();
    assert_eq!(read, vec![Some(lineage), Some(Lineage::user()), None]);
}

#[test]
fn a_memory_kept_before_the_migration_has_no_origin() {
    a_memory_from_before_lineage_reads_with_no_origin(&Place::file());
}

#[test]
fn a_lineage_is_kept_whole_in_the_database() {
    a_lineage_goes_in_and_comes_back(&Place::file());
}

on_postgres! {
    postgres_a_memory_kept_before_the_migration_has_no_origin => a_memory_from_before_lineage_reads_with_no_origin,
    postgres_a_lineage_is_kept_whole_in_the_database => a_lineage_goes_in_and_comes_back,
}
