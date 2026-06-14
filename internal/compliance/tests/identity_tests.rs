use compliance::IdentityManager;
use conxian_core::IdentityResolutionRequest;

#[cfg(not(feature = "mock-integrations"))]
#[tokio::test]
async fn test_resolve_ens_disabled_by_default() {
    let manager = IdentityManager::new();
    let req = IdentityResolutionRequest {
        identifier: "alice.eth".to_string(),
        provider: "ens".to_string(),
    };

    let err = manager.resolve_identity(&req).await.unwrap_err();
    assert!(
        err.to_string().contains("ENS resolution is disabled"),
        "unexpected error: {err}"
    );
}

#[cfg(feature = "mock-integrations")]
#[tokio::test]
async fn test_resolve_ens_simulated() {
    let manager = IdentityManager::new();
    let req = IdentityResolutionRequest {
        identifier: "alice.eth".to_string(),
        provider: "ens".to_string(),
    };
    let res = manager.resolve_identity(&req).await.unwrap();
    assert_eq!(res.provider, "ens");
    assert!(res.verified);
}

#[cfg(not(feature = "mock-integrations"))]
#[tokio::test]
async fn test_resolve_bns_disabled_by_default() {
    let manager = IdentityManager::new();
    let req = IdentityResolutionRequest {
        identifier: "bob.id".to_string(),
        provider: "bns".to_string(),
    };

    let err = manager.resolve_identity(&req).await.unwrap_err();
    assert!(
        err.to_string().contains("BNS resolution is disabled"),
        "unexpected error: {err}"
    );
}

#[cfg(feature = "mock-integrations")]
#[tokio::test]
async fn test_resolve_bns_simulated() {
    let manager = IdentityManager::new();
    let req = IdentityResolutionRequest {
        identifier: "bob.id".to_string(),
        provider: "bns".to_string(),
    };
    let res = manager.resolve_identity(&req).await.unwrap();
    assert_eq!(res.provider, "bns");
    assert!(res.verified);
}

#[cfg(not(feature = "mock-integrations"))]
#[tokio::test]
async fn test_resolve_worldid_disabled_by_default() {
    let manager = IdentityManager::new();
    let req = IdentityResolutionRequest {
        identifier: "world-id-user".to_string(),
        provider: "worldid".to_string(),
    };

    let err = manager.resolve_identity(&req).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("World ID verification is disabled"),
        "unexpected error: {err}"
    );
}

#[cfg(feature = "mock-integrations")]
#[tokio::test]
async fn test_resolve_worldid_simulated() {
    let manager = IdentityManager::new();
    let req = IdentityResolutionRequest {
        identifier: "world-id-user".to_string(),
        provider: "worldid".to_string(),
    };
    let res = manager.resolve_identity(&req).await.unwrap();
    assert_eq!(res.provider, "worldid");
    assert!(res.verified);
}
