use super::*;

#[test]
fn git_adapter_default_repo_root() {
    let adapter = GitAdapter::default();
    assert_eq!(adapter.repo_root, PathBuf::from("."));
}
