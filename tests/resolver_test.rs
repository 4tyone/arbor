use arbor::plugins::python::resolver::PythonResolver;
use std::path::PathBuf;

fn fixtures_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn test_resolve_simple_function() {
    let fixtures = fixtures_path();
    let mut resolver = PythonResolver::new(vec![fixtures.clone()], vec![]);

    let result = resolver.resolve("simple_module.simple_function");
    assert!(result.is_ok(), "Failed to resolve: {:?}", result);

    let resolved = result.unwrap();
    assert_eq!(resolved.function_name, "simple_function");
    assert!(resolved.file_path.ends_with("simple_module.py"));
    assert_eq!(resolved.line_start, 4);
    assert!(!resolved.is_method);
}

#[test]
fn test_resolve_class_method() {
    let fixtures = fixtures_path();
    let mut resolver = PythonResolver::new(vec![fixtures.clone()], vec![]);

    let result = resolver.resolve("simple_module.SimpleClass.method_one");
    assert!(result.is_ok(), "Failed to resolve: {:?}", result);

    let resolved = result.unwrap();
    assert_eq!(resolved.function_name, "SimpleClass.method_one");
    assert!(resolved.is_method);
    assert_eq!(resolved.parent_class, Some("SimpleClass".to_string()));
}

#[test]
fn test_resolve_package_function() {
    let fixtures = fixtures_path();
    let mut resolver = PythonResolver::new(vec![fixtures.clone()], vec![]);

    let result = resolver.resolve("mypackage.api.get_data");
    assert!(result.is_ok(), "Failed to resolve: {:?}", result);

    let resolved = result.unwrap();
    assert_eq!(resolved.function_name, "get_data");
    assert!(resolved.file_path.ends_with("api.py"));
}

#[test]
fn test_resolve_reexported_function() {
    let fixtures = fixtures_path();
    let mut resolver = PythonResolver::new(vec![fixtures.clone()], vec![]);

    let result = resolver.resolve("mypackage.get_data");
    assert!(result.is_ok(), "Failed to resolve reexported function: {:?}", result);

    let resolved = result.unwrap();
    assert_eq!(resolved.function_name, "get_data");
    assert!(resolved.file_path.ends_with("api.py"));
}

#[test]
fn test_resolve_class_in_package() {
    let fixtures = fixtures_path();
    let mut resolver = PythonResolver::new(vec![fixtures.clone()], vec![]);

    let result = resolver.resolve("mypackage.api.APIClient.request");
    assert!(result.is_ok(), "Failed to resolve: {:?}", result);

    let resolved = result.unwrap();
    assert_eq!(resolved.function_name, "APIClient.request");
    assert!(resolved.is_method);
}

#[test]
fn test_nonexistent_function() {
    let fixtures = fixtures_path();
    let mut resolver = PythonResolver::new(vec![fixtures.clone()], vec![]);

    let result = resolver.resolve("nonexistent.function");
    assert!(result.is_err());
}

#[test]
fn test_empty_qualified_name() {
    let mut resolver = PythonResolver::new(vec![], vec![]);
    let result = resolver.resolve("");
    assert!(result.is_err());
}

#[test]
fn test_resolve_reexported_class() {
    let fixtures = fixtures_path();
    let mut resolver = PythonResolver::new(vec![fixtures.clone()], vec![]);

    // User is re-exported from __init__.py
    let result = resolver.resolve("mypackage.User");
    assert!(result.is_ok(), "Failed to resolve reexported class: {:?}", result);

    let resolved = result.unwrap();
    assert_eq!(resolved.function_name, "User");
    assert!(resolved.file_path.ends_with("models.py"));
}
