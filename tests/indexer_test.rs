use arbor::analysis::indexer::Indexer;
use std::path::PathBuf;

fn fixtures_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn test_index_fixtures() {
    let mut indexer = Indexer::new().unwrap();
    let index = indexer.index_directories(&[fixtures_path()]).unwrap();

    println!("Indexed {} symbols", index.len());

    // Check that key symbols are indexed
    assert!(index.contains("simple_module.simple_function"));
    assert!(index.contains("simple_module.another_function"));
    assert!(index.contains("simple_module.SimpleClass"));
    assert!(index.contains("simple_module.SimpleClass.method_one"));
    assert!(index.contains("simple_module.SimpleClass.method_two"));

    // Check package symbols
    assert!(index.contains("mypackage.api.get_data"));
    assert!(index.contains("mypackage.api.post_data"));
    assert!(index.contains("mypackage.api.APIClient"));
    assert!(index.contains("mypackage.api.APIClient.request"));
    assert!(index.contains("mypackage.api.APIClient.get"));
    assert!(index.contains("mypackage.models.User"));
    assert!(index.contains("mypackage.models.Admin"));
    assert!(index.contains("mypackage.utils.helper_function"));
}

#[test]
fn test_index_locations_are_correct() {
    let mut indexer = Indexer::new().unwrap();
    let index = indexer.index_directories(&[fixtures_path()]).unwrap();

    let simple_fn = index.get("simple_module.simple_function").unwrap();
    assert!(simple_fn.file_path.ends_with("simple_module.py"));
    assert_eq!(simple_fn.line_start, 4);
    assert!(!simple_fn.is_method);

    let method = index.get("simple_module.SimpleClass.method_one").unwrap();
    assert!(method.is_method);
    assert_eq!(method.parent_class, Some("SimpleClass".to_string()));
}

#[test]
fn test_index_file_hashes() {
    let mut indexer = Indexer::new().unwrap();
    let index = indexer.index_directories(&[fixtures_path()]).unwrap();

    // File hashes should be set
    assert!(!index.file_hashes.is_empty());
    assert!(index.indexed_at.is_some());
}
