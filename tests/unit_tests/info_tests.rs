// Info route tests - extracted from src/routes/info.rs

use the_beaconator::routes::index;

#[test]
fn test_index() {
    let result = index();
    let response = result.into_inner();

    assert!(response.success);
    assert!(response.data.is_some());

    let api_summary = response.data.unwrap();
    assert!(api_summary.total_endpoints > 0);
    assert!(response.message.contains("Beaconator"));
    assert!(response.message.contains("endpoints"));
}

#[test]
fn test_index_detailed_output() {
    let result = index();
    let response = result.into_inner();

    assert!(response.success);
    assert!(response.data.is_some());

    let api_summary = response.data.unwrap();
    assert_eq!(api_summary.total_endpoints, api_summary.endpoints.len());
    assert!(api_summary.working_endpoints > 0);
    assert!(api_summary.not_implemented > 0);
}
