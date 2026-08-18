/// Unit tests for `catalog add`'s record-building logic — alias parsing, attrs shape,
/// and kind rejection. No network calls.
use braincrawl_cli::provider::build_manual_work_record;

#[test]
fn alias_splits_on_first_colon() {
    let record = build_manual_work_record(
        &["isbn:9780691215105".to_string()],
        "A Test Book".to_string(),
        vec![],
        None,
        "work",
    )
    .unwrap();
    assert_eq!(record.aliases.len(), 1);
    assert_eq!(record.aliases[0].scheme, "isbn");
    assert_eq!(record.aliases[0].value, "9780691215105");
}

#[test]
fn alias_without_colon_errors() {
    let err = build_manual_work_record(
        &["isbn9780691215105".to_string()],
        "A Test Book".to_string(),
        vec![],
        None,
        "work",
    )
    .unwrap_err();
    assert!(err.contains("ns:value"), "error should name the expected form: {err}");
}

#[test]
fn attrs_carry_title_authors_and_year() {
    let record = build_manual_work_record(
        &["isbn:9780691215105".to_string()],
        "A Test Book".to_string(),
        vec!["Surname, A.".to_string()],
        Some(2020),
        "work",
    )
    .unwrap();
    assert_eq!(record.attrs["title"], "A Test Book");
    assert_eq!(record.attrs["authors"], serde_json::json!(["Surname, A."]));
    assert_eq!(record.attrs["publication_year"], 2020);
}

#[test]
fn attrs_omit_absent_authors_and_year() {
    let record = build_manual_work_record(
        &["isbn:9780691215105".to_string()],
        "A Test Book".to_string(),
        vec![],
        None,
        "work",
    )
    .unwrap();
    let obj = record.attrs.as_object().unwrap();
    assert!(!obj.contains_key("authors"));
    assert!(!obj.contains_key("publication_year"));
    assert_eq!(obj.len(), 1);
}

#[test]
fn source_is_manual() {
    let record = build_manual_work_record(
        &["isbn:9780691215105".to_string()],
        "A Test Book".to_string(),
        vec![],
        None,
        "work",
    )
    .unwrap();
    assert_eq!(record.source, "manual");
}

#[test]
fn kind_is_lowercased() {
    let record = build_manual_work_record(
        &["isbn:9780691215105".to_string()],
        "A Test Book".to_string(),
        vec![],
        None,
        "Work",
    )
    .unwrap();
    assert_eq!(record.kind, "work");
}

#[test]
fn unknown_kind_is_rejected() {
    let err = build_manual_work_record(
        &["isbn:9780691215105".to_string()],
        "A Test Book".to_string(),
        vec![],
        None,
        "bogus",
    )
    .unwrap_err();
    assert!(err.contains("bogus"), "error should name the bad kind: {err}");
    assert!(err.contains("work"), "error should list accepted kinds: {err}");
}
