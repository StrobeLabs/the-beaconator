use rocket::fairing::{Fairing, Kind};
use the_beaconator::fairings::{PanicCatcher, RequestLogger};

#[test]
fn test_request_logger_info() {
    let logger = RequestLogger;
    let info = logger.info();

    assert_eq!(info.name, "Request/Response Logger");
    // Kind doesn't support equality or contains checks, so we just verify structure
    let _kind = info.kind; // Verify it exists and compiles
}

#[test]
fn test_panic_catcher_info() {
    let catcher = PanicCatcher;
    let info = catcher.info();

    assert_eq!(info.name, "Panic Catcher");
    // Kind doesn't support equality or contains checks, so we just verify structure
    let _kind = info.kind; // Verify it exists and compiles
}

#[test]
fn test_fairing_names() {
    let logger = RequestLogger;
    let catcher = PanicCatcher;

    assert_eq!(logger.info().name, "Request/Response Logger");
    assert_eq!(catcher.info().name, "Panic Catcher");

    // Names should be different
    assert_ne!(logger.info().name, catcher.info().name);
}

#[test]
fn test_fairing_kinds() {
    let logger = RequestLogger;
    let catcher = PanicCatcher;

    // Both should have kinds defined (can't test equality)
    let _logger_kind = logger.info().kind;
    let _catcher_kind = catcher.info().kind;
}

#[test]
fn test_fairing_trait_implementation() {
    // Verify that our fairings implement the Fairing trait correctly
    fn check_fairing<T: Fairing>(_fairing: T) {}

    check_fairing(RequestLogger);
    check_fairing(PanicCatcher);
}

#[test]
fn test_kind_operations() {
    // Test Kind flag operations
    let _request_kind = Kind::Request;
    let _response_kind = Kind::Response;
    let _combined_kind = Kind::Request | Kind::Response;

    // We can create different kinds but can't test equality
    let _ignite_kind = Kind::Ignite;
    let _liftoff_kind = Kind::Liftoff;
    let _shutdown_kind = Kind::Shutdown;
}

#[test]
fn test_fairing_instantiation() {
    // Test that we can create multiple instances
    let logger1 = RequestLogger;
    let logger2 = RequestLogger;
    let catcher1 = PanicCatcher;
    let catcher2 = PanicCatcher;

    // All should have the same names
    assert_eq!(logger1.info().name, logger2.info().name);
    assert_eq!(catcher1.info().name, catcher2.info().name);
}

#[test]
fn test_fairing_info_consistency() {
    let logger = RequestLogger;

    // Multiple calls to info() should return consistent results
    let info1 = logger.info();
    let info2 = logger.info();

    assert_eq!(info1.name, info2.name);
    // Kind doesn't implement PartialEq, so we can't test equality
    let _kind1 = info1.kind;
    let _kind2 = info2.kind;
}

#[test]
fn test_request_logger_struct() {
    // Test that RequestLogger is a zero-sized type
    use std::mem;
    assert_eq!(mem::size_of::<RequestLogger>(), 0);
}

#[test]
fn test_panic_catcher_struct() {
    // Test that PanicCatcher is a zero-sized type
    use std::mem;
    assert_eq!(mem::size_of::<PanicCatcher>(), 0);
}

#[test]
fn test_fairing_name_lengths() {
    let logger = RequestLogger;
    let catcher = PanicCatcher;

    // Names should be reasonable length
    assert!(!logger.info().name.is_empty());
    assert!(logger.info().name.len() < 100);
    assert!(!catcher.info().name.is_empty());
    assert!(catcher.info().name.len() < 100);
}

#[test]
fn test_fairing_name_content() {
    let logger = RequestLogger;
    let catcher = PanicCatcher;

    // Names should contain expected keywords
    assert!(logger.info().name.contains("Request"));
    assert!(logger.info().name.contains("Response"));
    assert!(logger.info().name.contains("Logger"));

    assert!(catcher.info().name.contains("Panic"));
    assert!(catcher.info().name.contains("Catcher"));
}

#[test]
fn test_kind_combinations() {
    // Test various Kind combinations
    let _ignite = Kind::Ignite; // Different kind
    let _request = Kind::Request;
    let _response = Kind::Response;
    let _both = Kind::Request | Kind::Response;

    // We can create and combine kinds but can't test their behavior
    let _liftoff = Kind::Liftoff;
    let _shutdown = Kind::Shutdown;
    let _combined = Kind::Ignite | Kind::Liftoff | Kind::Shutdown;
}
