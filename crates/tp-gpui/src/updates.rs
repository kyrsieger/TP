use anyhow::{ensure, Context, Result};
use semver::Version;
use std::time::Duration;
use zed_reqwest::{header::LOCATION, StatusCode};

const LATEST_RELEASE_URL: &str = "https://github.com/kyrsieger/TP/releases/latest";
const GITHUB_ORIGIN: &str = "https://github.com";
const RELEASE_TAG_PATH: &str = "/kyrsieger/TP/releases/tag/";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
pub(crate) struct Release {
    pub(crate) version: Version,
}

impl Release {
    pub(crate) fn page_url(&self) -> String {
        format!("{GITHUB_ORIGIN}{RELEASE_TAG_PATH}v{}", self.version)
    }
}

pub(crate) fn check() -> Result<Option<Release>> {
    latest_release_at(LATEST_RELEASE_URL, CURRENT_VERSION)
}

fn latest_release_at(endpoint: &str, current_version: &str) -> Result<Option<Release>> {
    let response = zed_reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .redirect(zed_reqwest::redirect::Policy::none())
        .build()?
        .get(endpoint)
        .timeout(REQUEST_TIMEOUT)
        .header("User-Agent", concat!("TP/", env!("CARGO_PKG_VERSION")))
        .send()?;
    let status = response.status();
    ensure!(
        matches!(
            status,
            StatusCode::MOVED_PERMANENTLY
                | StatusCode::FOUND
                | StatusCode::SEE_OTHER
                | StatusCode::TEMPORARY_REDIRECT
                | StatusCode::PERMANENT_REDIRECT
        ),
        "Expected a GitHub release redirect, got {status}"
    );

    let mut locations = response.headers().get_all(LOCATION).into_iter();
    let location = locations
        .next()
        .context("Missing GitHub release location")?;
    ensure!(
        locations.next().is_none(),
        "Multiple GitHub release locations"
    );
    release_from_location(location.to_str()?, current_version)
}

fn release_from_location(location: &str, current_version: &str) -> Result<Option<Release>> {
    // Accept the exact GitHub release URL or its root-relative path.
    let path = location.strip_prefix(GITHUB_ORIGIN).unwrap_or(location);
    let tag = path
        .strip_prefix(RELEASE_TAG_PATH)
        .context("Unexpected GitHub release location")?;
    // TP's publishing script accepts only vMAJOR.MINOR.PATCH tags.
    let version = Version::parse(
        tag.strip_prefix('v')
            .context("Invalid GitHub release tag")?,
    )?;
    ensure!(
        version.pre.is_empty() && version.build.is_empty(),
        "Invalid TP release tag"
    );
    let current = Version::parse(current_version)?;
    Ok(version
        .cmp_precedence(&current)
        .is_gt()
        .then_some(Release { version }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn compares_release_versions_numerically() {
        let location = "https://github.com/kyrsieger/TP/releases/tag/v1.1.10";
        let release = release_from_location(location, "1.1.9").unwrap().unwrap();
        assert_eq!(
            release.page_url(),
            "https://github.com/kyrsieger/TP/releases/tag/v1.1.10"
        );
        for version in ["1.1.10", "1.1.10+local", "1.2.0"] {
            assert!(release_from_location(location, version).unwrap().is_none());
        }
        assert!(release_from_location(location, "1.1.10-rc.1")
            .unwrap()
            .is_some());
    }

    #[test]
    fn rejects_untrusted_locations_and_invalid_or_nonstable_tags() {
        for location in [
            "http://github.com/kyrsieger/TP/releases/tag/v1.2.0",
            "https://example.com/kyrsieger/TP/releases/tag/v1.2.0",
            "https://github.com.example.com/kyrsieger/TP/releases/tag/v1.2.0",
            "https://github.com@other.example/kyrsieger/TP/releases/tag/v1.2.0",
            "//github.com/kyrsieger/TP/releases/tag/v1.2.0",
            "/other/TP/releases/tag/v1.2.0",
            "/kyrsieger/TP/releases/latest",
            "/kyrsieger/TP/releases/tag/v1.2.0-rc.1",
            "/kyrsieger/TP/releases/tag/v1.2.0+build",
            "/kyrsieger/TP/releases/tag/v1.2.0/other",
            "/kyrsieger/TP/releases/tag/v1.2.0?other",
            "/kyrsieger/TP/releases/tag/v1.2.0#other",
            "/kyrsieger/TP/releases/tag/v1.2",
        ] {
            assert!(
                release_from_location(location, "1.1.0").is_err(),
                "{location}"
            );
        }
    }

    #[test]
    fn reads_redirect_without_following_it_and_rejects_invalid_responses() {
        for (status, locations, expected_error) in [
            (
                "302 Found",
                "Location: /kyrsieger/TP/releases/tag/v1.2.0\r\n",
                None,
            ),
            ("302 Found", "", Some("Missing GitHub release location")),
            (
                "302 Found",
                concat!(
                    "Location: /kyrsieger/TP/releases/tag/v1.2.0\r\n",
                    "Location: /kyrsieger/TP/releases/tag/v1.3.0\r\n",
                ),
                Some("Multiple GitHub release locations"),
            ),
            ("200 OK", "", Some("200 OK")),
            ("304 Not Modified", "", Some("304 Not Modified")),
            ("403 Forbidden", "", Some("403 Forbidden")),
            ("404 Not Found", "", Some("404 Not Found")),
            (
                "500 Internal Server Error",
                "",
                Some("500 Internal Server Error"),
            ),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = format!("http://{}/releases/latest", listener.local_addr().unwrap());
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = String::new();
                let mut reader = BufReader::new(&stream);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                assert!(request.starts_with("GET /releases/latest HTTP/1.1\r\n"));
                let request = request.to_ascii_lowercase();
                assert!(request.contains("\r\nuser-agent: tp/"));
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\n{locations}Content-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
            });
            let result = latest_release_at(&endpoint, "1.1.0");
            if let Some(expected_error) = expected_error {
                let error = result.err().expect("HTTP failure should fail the check");
                assert!(error.to_string().contains(expected_error), "{error:#}");
            } else {
                assert_eq!(result.unwrap().unwrap().version, Version::new(1, 2, 0));
            }
            server.join().unwrap();
        }
    }
}
