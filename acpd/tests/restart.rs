use acpd::{api::Host, config::Config, registry::Registry};
use serde_json::json;

#[tokio::test]
async fn restart_retains_effective_access_and_explicit_load_can_choose_new_access() {
    use acpd::connection::WorkspaceAccess;
    let (_directory, config, registry) = fixture(&[]);
    let host = Host::new(config.clone(), registry.clone()).unwrap();
    let original = WorkspaceAccess {
        read_files: true,
        write_files: false,
        terminal: false,
        form_elicitation: false,
    };
    let session = host
        .sessions
        .open_configured("mock", &config.workspace_roots[0], None, vec![], original)
        .await
        .unwrap();
    let metadata = session.metadata();
    drop(session);
    host.sessions.shutdown().await;
    drop(host);
    let host = Host::new(config.clone(), registry).unwrap();
    assert_eq!(
        host.sessions
            .metadata(metadata.id)
            .await
            .unwrap()
            .workspace_access,
        original
    );
    assert_eq!(host.sessions.active_count().await, 0);
    let next = WorkspaceAccess {
        read_files: false,
        write_files: false,
        terminal: false,
        form_elicitation: false,
    };
    let loaded = host
        .sessions
        .open_configured(
            "mock",
            &metadata.workspace,
            Some(&metadata.acp_session_id),
            vec![],
            next,
        )
        .await
        .unwrap();
    assert_eq!(loaded.metadata().workspace_access, next);
    assert_eq!(
        loaded
            .connection
            .request("_mock/client_capabilities", json!({}))
            .await
            .unwrap(),
        next.capabilities()
    );
    assert_eq!(
        host.sessions
            .metadata(metadata.id)
            .await
            .unwrap()
            .workspace_access,
        original
    );
    host.sessions.shutdown().await;
}

fn fixture(args: &[&str]) -> (tempfile::TempDir, Config, Registry) {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let config = Config {
        workspace_roots: vec![workspace],
        state_directory: directory.path().join("state"),
        ..Default::default()
    };
    let registry=Registry::parse(&json!([{"id":"mock","name":"Mock","command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":args}]).to_string()).unwrap();
    (directory, config, registry)
}

#[tokio::test]
async fn persisted_authentication_setup_and_disabled_resume_remain_capability_driven() {
    for args in [vec!["--require-auth"], vec!["--no-resume"]] {
        let (_directory, config, registry) = fixture(&args);
        let host = Host::new(config.clone(), registry.clone()).unwrap();
        let session = host
            .sessions
            .create("mock", &config.workspace_roots[0])
            .await
            .unwrap();
        if args.contains(&"--require-auth") {
            assert!(session.metadata().acp_session_id.is_empty());
            session
                .authenticate(json!({"methodId":"mock-login"}))
                .await
                .unwrap();
        }
        let metadata = session.metadata();
        drop(session);
        host.sessions.shutdown().await;
        drop(host);
        let host = Host::new(config.clone(), registry.clone()).unwrap();
        let recovered = host.sessions.metadata(metadata.id).await.unwrap();
        assert_eq!(recovered.acp_session_id, metadata.acp_session_id);
        assert_eq!(recovered.status, "interrupted");
        assert_eq!(host.sessions.active_count().await, 0);
        if args.contains(&"--no-resume") {
            assert!(
                host.sessions
                    .load("mock", &metadata.workspace, &metadata.acp_session_id)
                    .await
                    .is_err()
            );
            assert_eq!(host.sessions.active_count().await, 0);
        }
        host.sessions.shutdown().await;
    }
}

#[tokio::test]
async fn catalog_has_one_owner_revalidates_workspace_policy_and_fails_closed_on_corruption() {
    let (directory, mut config, registry) = fixture(&[]);
    let host = Host::new(config.clone(), registry.clone()).unwrap();
    assert!(Host::new(config.clone(), registry.clone()).is_err());
    let session = host
        .sessions
        .create("mock", &config.workspace_roots[0])
        .await
        .unwrap();
    let id = session.metadata().id;
    drop(session);
    host.sessions.shutdown().await;
    drop(host);
    let outside = directory.path().join("different-workspace");
    std::fs::create_dir(&outside).unwrap();
    config.workspace_roots = vec![outside];
    let host = Host::new(config.clone(), registry.clone()).unwrap();
    assert!(host.sessions.list().await.is_empty());
    assert!(host.sessions.metadata(id).await.is_err());
    drop(host);
    std::fs::write(
        config.state_directory.join("sessions.json"),
        b"invalid catalog",
    )
    .unwrap();
    assert!(Host::new(config, registry).is_err());
}
