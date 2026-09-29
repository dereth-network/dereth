//! ACE: Source/ACE.Server/ServerBuildInfo_Static.cs::FullVersion
//! Tests of build information.
//! Fixture: isolated world state and the shared area fixtures.

mod version {

    /// `ServerBuildInfo` (ServerBuildInfo_Static.cs): `FullVersion` is
    /// `{Version}.{Build}.{yyyyMMddHHmmss}-{Branch}-{CommitID}` and `GetServerVersion` parses
    /// `Version.Build` (here the build environment sets none of the EMPYREAN_BUILD_* values).
    #[test]
    fn server_build_info_formats_the_version() {
        use empyrean_common::server_build_info as sbi;
        if option_env!("EMPYREAN_BUILD_COMMIT").is_some()
            || option_env!("EMPYREAN_BUILD_UTC").is_some()
        {
            return;
        }
        assert_eq!(sbi::commit_id(), "0000000");
        assert_eq!(
            sbi::full_version(),
            format!(
                "{}.{}.00010101000000-{}-0000000",
                sbi::VERSION,
                sbi::BUILD,
                sbi::BRANCH
            )
        );
        assert_eq!(
            sbi::get_server_version().len(),
            sbi::VERSION.split('.').count() + 1
        );
    }
}

/// A release version with a pre-release or build suffix reads as its numeric core, and the build
/// number still follows it.
#[test]
fn a_pre_release_version_reads_as_its_numeric_core() {
    use empyrean_common::server_build_info as sbi;
    assert_eq!(sbi::version_components("0.1.0-rc.1", "7"), vec![0, 1, 0, 7]);
    assert_eq!(sbi::version_components("0.2.3+abc", "0"), vec![0, 2, 3, 0]);
    assert_eq!(sbi::version_components("0.1.0", "12"), vec![0, 1, 0, 12]);
}
