use compliance::IdentityManager;
use conxian_core::IdentityResolutionRequest;

#[tokio::test]
async fn test_resolve_ens_simulated() {
    let manager = IdentityManager::new();
    let req = IdentityResolutionRequest {
        identifier: "alice.eth".to_string(),
        signature: None,
        provider: "ens".to_string(),
    };
    let res = manager.resolve_identity(&req).await.unwrap();
    assert_eq!(res.provider, "ens");
    assert!(res.verified);
}

#[tokio::test]
async fn test_resolve_bns_simulated() {
    let manager = IdentityManager::new();
    let req = IdentityResolutionRequest {
        identifier: "bob.id".to_string(),
        signature: None,
        provider: "bns".to_string(),
    };
    let res = manager.resolve_identity(&req).await.unwrap();
    assert_eq!(res.provider, "bns");
    assert!(res.verified);
}

#[tokio::test]
async fn test_resolve_worldid_simulated() {
    let manager = IdentityManager::new();
    let req = IdentityResolutionRequest {
        identifier: "world-id-user".to_string(),
        signature: None,
        provider: "worldid".to_string(),
    };
    let res = manager.resolve_identity(&req).await.unwrap();
    assert_eq!(res.provider, "worldid");
    assert!(res.verified);
}
