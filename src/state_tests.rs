use super::*;

fn file() -> (tempfile::TempDir, StateFile) {
    let dir = tempfile::tempdir().unwrap();
    let file = StateFile::at(dir.path().join("chatgpt").join("state.json"));
    (dir, file)
}

#[test]
fn host_id_is_created_once_and_reused() {
    let (_dir, file) = file();
    let first = file.host_id().unwrap();
    assert!(first.starts_with("urn:uuid:"), "{first}");
    assert_eq!(first.len(), "urn:uuid:".len() + 36);
    assert_eq!(file.host_id().unwrap(), first);
}

#[test]
fn forgetting_the_registration_keeps_the_host() {
    let (_dir, file) = file();
    let host = file.host_id().unwrap();
    file.set_registration(Registration {
        client_id: "oaiapp_test".into(),
        subject: Some("sub".into()),
        email: Some("a@example.com".into()),
    })
    .unwrap();
    assert_eq!(file.registration().unwrap().client_id, "oaiapp_test");
    file.forget_registration().unwrap();
    assert!(file.registration().is_none());
    assert_eq!(file.host_id().unwrap(), host);
}

#[test]
fn corrupt_state_reads_as_fresh_install() {
    let (_dir, file) = file();
    std::fs::create_dir_all(file.path().parent().unwrap()).unwrap();
    std::fs::write(file.path(), b"{not json").unwrap();
    assert_eq!(file.load(), State::default());
}

#[cfg(unix)]
#[test]
fn state_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, file) = file();
    file.host_id().unwrap();
    let mode = std::fs::metadata(file.path()).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}
