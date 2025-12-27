use arbor::analysis::traversal::Traverser;
use arbor::analysis::indexer::Indexer;
use arbor::plugins::python::resolver::PythonResolver;
use std::path::PathBuf;

fn fixtures_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn test_analyze_simple_function() {
    let mut indexer = Indexer::new().unwrap();
    let index = indexer.index_directories(&[fixtures_path()]).unwrap();

    let resolver = PythonResolver::new(vec![fixtures_path()], vec![]);
    let mut traverser = Traverser::new(resolver, 10)
        .unwrap()
        .with_symbol_index(index);

    let analysis = traverser.analyze_function("simple_module.simple_function").unwrap();

    assert_eq!(analysis.function_id, "simple_module.simple_function");
    assert!(analysis.functions_traced >= 1);
}

#[test]
fn test_analyze_function_with_raises() {
    let mut indexer = Indexer::new().unwrap();
    let index = indexer.index_directories(&[fixtures_path()]).unwrap();

    let resolver = PythonResolver::new(vec![fixtures_path()], vec![]);
    let mut traverser = Traverser::new(resolver, 10)
        .unwrap()
        .with_symbol_index(index);

    let analysis = traverser.analyze_function("exceptions_and_none.simple_raise").unwrap();

    assert_eq!(analysis.raises.len(), 1);
    assert_eq!(analysis.raises[0].exception_type, "ValueError");
}

#[test]
fn test_analyze_function_with_none_return() {
    let mut indexer = Indexer::new().unwrap();
    let index = indexer.index_directories(&[fixtures_path()]).unwrap();

    let resolver = PythonResolver::new(vec![fixtures_path()], vec![]);
    let mut traverser = Traverser::new(resolver, 10)
        .unwrap()
        .with_symbol_index(index);

    let analysis = traverser.analyze_function("exceptions_and_none.explicit_none_return").unwrap();

    assert_eq!(analysis.none_sources.len(), 1);
}

#[test]
fn test_analyze_nonexistent_function() {
    let resolver = PythonResolver::new(vec![fixtures_path()], vec![]);
    let mut traverser = Traverser::new(resolver, 10).unwrap();

    let analysis = traverser.analyze_function("nonexistent.function").unwrap();

    // Function not found - no raises or none sources
    assert!(analysis.raises.is_empty());
    assert!(analysis.none_sources.is_empty());
}

#[test]
fn test_exception_definition_lookup() {
    let mut indexer = Indexer::new().unwrap();
    let index = indexer.index_directories(&[fixtures_path()]).unwrap();

    let resolver = PythonResolver::new(vec![fixtures_path()], vec![]);
    let mut traverser = Traverser::new(resolver, 10)
        .unwrap()
        .with_symbol_index(index);

    let analysis = traverser.analyze_function("custom_exceptions.raise_custom").unwrap();

    assert_eq!(analysis.raises.len(), 1);
    assert_eq!(analysis.raises[0].exception_type, "CustomError");

    // The definition location should be resolved to the CustomError class
    assert!(analysis.raises[0].definition_location.is_some());
    let def_loc = analysis.raises[0].definition_location.as_ref().unwrap();
    assert!(def_loc.file.to_string_lossy().contains("custom_exceptions.py"));
    assert_eq!(def_loc.line, 4); // CustomError is defined on line 4
}
