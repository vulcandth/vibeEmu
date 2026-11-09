//! Boot fixture fallback must not change the bytes used by accuracy tests.
#[path = "common/bootrom_download.rs"]
mod bootrom_download;

const FIXTURE: &[u8] = b"verified boot fixture";
const DIGEST: &str = "7ed93e71a1c2aa7c465e50b3afe5fa874ce5a9241b7d219754e35c6e14db1f7a";

#[test]
fn primary_success_does_not_contact_backup() {
    let mut calls = Vec::new();
    let bytes = bootrom_download::download_verified("primary", "backup", DIGEST, |url| {
        calls.push(url.to_owned());
        Ok(FIXTURE.to_vec())
    })
    .unwrap();
    assert_eq!(bytes, FIXTURE);
    assert_eq!(calls, ["primary"]);
}

#[test]
fn backup_is_verified_after_primary_error_or_corruption() {
    for primary in [Err("offline".to_owned()), Ok(b"corrupt primary".to_vec())] {
        let mut calls = Vec::new();
        let bytes = bootrom_download::download_verified("primary", "backup", DIGEST, |url| {
            calls.push(url.to_owned());
            if url == "primary" {
                primary.clone()
            } else {
                Ok(FIXTURE.to_vec())
            }
        })
        .unwrap();
        assert_eq!(bytes, FIXTURE);
        assert_eq!(calls, ["primary", "backup"]);
    }
}

#[test]
fn failed_or_corrupt_backup_is_never_accepted() {
    for backup in [
        Err("also offline".to_owned()),
        Ok(b"corrupt backup".to_vec()),
    ] {
        let error = bootrom_download::download_verified("primary", "backup", DIGEST, |url| {
            if url == "primary" {
                Err("offline".to_owned())
            } else {
                backup.clone()
            }
        })
        .unwrap_err();
        assert!(error.contains("primary: offline"));
        assert!(error.contains("backup:"));
    }
    assert!(bootrom_download::verify(b"corrupt cache", DIGEST).is_err());
}
