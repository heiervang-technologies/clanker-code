use codex_utils_cargo_bin::cargo_bin;
use pretty_assertions::assert_eq;
use std::process::Command;

#[test]
fn retained_binaries_identify_clanker_code_and_its_upstream_base() {
    for binary in ["clanker", "codex"] {
        let binary_path =
            cargo_bin(binary).unwrap_or_else(|err| panic!("locate {binary} binary: {err}"));
        let output = Command::new(binary_path)
            .arg("--version")
            .output()
            .unwrap_or_else(|err| panic!("run {binary} --version: {err}"));

        assert!(output.status.success());
        assert_eq!(output.stderr, b"");
        let stdout = String::from_utf8(output.stdout).expect("version output should be UTF-8");
        let version = stdout
            .strip_prefix("Clanker Code ")
            .and_then(|version| version.strip_suffix('\n'))
            .expect("version should use Clanker Code branding");
        semver::Version::parse(version).expect("Clanker Code version should be valid SemVer");
        let upstream_version = version
            .strip_prefix("0.1.0+codex.")
            .expect("version should include the Codex base");
        let (release_and_distance, upstream_sha) = upstream_version
            .rsplit_once(".g")
            .expect("upstream version should end with its Git revision");
        let (release, distance) = release_and_distance
            .rsplit_once('.')
            .expect("upstream version should include its distance from the release");

        assert!(release.chars().next().is_some_and(|ch| ch.is_ascii_digit()));
        assert!(release.matches('.').count() >= 2);
        assert!(distance.parse::<u64>().is_ok());
        assert_eq!(upstream_sha.len(), 12);
        assert!(
            upstream_sha
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
    }
}
