fn main() {
    println!("cargo:rerun-if-env-changed=VOID_RELEASE_BUILD");
    println!("cargo:rerun-if-env-changed=VOID_UPDATE_SIGNING_TEAM_ID");

    let release_build = std::env::var("VOID_RELEASE_BUILD").as_deref() == Ok("1");
    let team_id = std::env::var("VOID_UPDATE_SIGNING_TEAM_ID").unwrap_or_default();
    if release_build {
        assert!(
            is_valid_team_id(&team_id),
            "release builds require VOID_UPDATE_SIGNING_TEAM_ID to be a 10-character Apple Team ID"
        );
    }
    println!("cargo:rustc-env=VOID_UPDATE_SIGNING_TEAM_ID={team_id}");
}

fn is_valid_team_id(team_id: &str) -> bool {
    team_id.len() == 10
        && team_id
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}
