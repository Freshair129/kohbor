//! Pure request/response logic for serving prewarmed visual media.

use std::fs::{self, File, Metadata};
use std::io::{Read, Seek, SeekFrom};

use crate::media::prewarm::{VisualEntry, VisualIndex};

const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_RANGE_BYTES: u64 = 4 * 1024 * 1024;
const NOT_FOUND_BODY: &[u8] = b"Not Found";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolRequest {
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub range: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Serve one request from the immutable allowlist and its captured file metadata.
pub fn serve(request: &ProtocolRequest, index: &VisualIndex) -> ProtocolResponse {
    let is_head = request.method == "HEAD";
    if (!is_head && request.method != "GET") || request.query.is_some() {
        return not_found();
    }

    let Some((install_id, token)) = parse_media_path(&request.path) else {
        return not_found();
    };
    if index.install_id.as_deref() != Some(install_id) {
        return not_found();
    }
    let Some(entry) = index.entries.get(token) else {
        return not_found();
    };
    if entry.size_bytes > MAX_FILE_BYTES {
        return not_found();
    }

    let Some(metadata) = safe_path_metadata(&entry.path) else {
        return not_found();
    };
    if !metadata_matches_entry(&metadata, entry) {
        return not_found();
    }

    let range = match request.range.as_deref() {
        Some(value) => match parse_range(value, entry.size_bytes) {
            Some(range) => Some(range),
            None => return range_not_satisfiable(entry.size_bytes),
        },
        None => None,
    };

    let (status, start, length, content_range) = match range {
        Some((start, end)) => (
            206,
            start,
            end.saturating_sub(start).saturating_add(1),
            Some(format!("bytes {start}-{end}/{}", entry.size_bytes)),
        ),
        None => (200, 0, entry.size_bytes, None),
    };

    let body = if is_head {
        if open_verified_file(&entry.path, entry).is_none() {
            return not_found();
        }
        Vec::new()
    } else {
        match read_bounded_file(&entry.path, entry, start, length) {
            Some(body) => body,
            None => return not_found(),
        }
    };

    let mut headers = media_headers();
    if let Some(content_range) = content_range {
        headers.push(("Content-Range".to_owned(), content_range));
    }
    ProtocolResponse {
        status,
        headers,
        body,
    }
}

/// Return whether a path has the exact media route shape expected by the handler.
pub fn is_media_url_path(path: &str) -> bool {
    parse_media_path(path).is_some()
}

fn parse_media_path(path: &str) -> Option<(&str, &str)> {
    if path.contains('%') || path.contains('\\') || path.contains("..") {
        return None;
    }
    let suffix = path.strip_prefix("/v1/")?;
    let (install_id, token) = suffix.split_once('/')?;
    if token.contains('/') || !valid_install_id(install_id) || !valid_token(token) {
        return None;
    }
    Some((install_id, token))
}

fn valid_install_id(value: &str) -> bool {
    let Some(digits) = value.strip_prefix("ins_") else {
        return false;
    };
    digits.len() == 10 && digits.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_token(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn safe_path_metadata(path: &std::path::Path) -> Option<Metadata> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_symlink() {
        return None;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return None;
        }
    }
    if !metadata.is_file() {
        return None;
    }
    Some(metadata)
}

fn metadata_matches_entry(metadata: &Metadata, entry: &VisualEntry) -> bool {
    metadata.is_file()
        && metadata.len() == entry.size_bytes
        && metadata.modified().ok() == Some(entry.modified)
}

fn parse_range(value: &str, total: u64) -> Option<(u64, u64)> {
    let value = value.strip_prefix("bytes=")?;
    if value.contains(',') {
        return None;
    }
    let (start_text, end_text) = value.split_once('-')?;
    if end_text.contains('-') || total == 0 {
        return None;
    }

    let (start, requested_end) = if start_text.is_empty() {
        let suffix = parse_decimal(end_text)?;
        if suffix == 0 {
            return None;
        }
        let length = suffix.min(total).min(MAX_RANGE_BYTES);
        (total.saturating_sub(length), Some(total.saturating_sub(1)))
    } else {
        let start = parse_decimal(start_text)?;
        if start >= total {
            return None;
        }
        let end = if end_text.is_empty() {
            total.saturating_sub(1)
        } else {
            parse_decimal(end_text)?
        };
        if end < start {
            return None;
        }
        (start, Some(end))
    };

    if start >= total {
        return None;
    }
    let last_file_byte = total.saturating_sub(1);
    let last_range_byte = start.saturating_add(MAX_RANGE_BYTES.saturating_sub(1));
    let end = requested_end?.min(last_file_byte).min(last_range_byte);
    (end >= start).then_some((start, end))
}

fn parse_decimal(value: &str) -> Option<u64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

fn read_bounded_file(
    path: &std::path::Path,
    entry: &VisualEntry,
    start: u64,
    length: u64,
) -> Option<Vec<u8>> {
    let mut file = open_verified_file(path, entry)?;
    file.seek(SeekFrom::Start(start)).ok()?;
    let capacity = usize::try_from(length).ok()?;
    let mut body = Vec::with_capacity(capacity);
    let mut limited = file.take(length);
    limited.read_to_end(&mut body).ok()?;
    if body.len() != capacity {
        return None;
    }
    let file = limited.into_inner();
    let after_read = file.metadata().ok()?;
    if !metadata_matches_entry(&after_read, entry) {
        return None;
    }
    let path_after_read = safe_path_metadata(path)?;
    if !metadata_matches_entry(&path_after_read, entry) {
        return None;
    }
    Some(body)
}

fn open_verified_file(path: &std::path::Path, entry: &VisualEntry) -> Option<File> {
    let before_open = safe_path_metadata(path)?;
    if !metadata_matches_entry(&before_open, entry) {
        return None;
    }

    let file = File::open(path).ok()?;
    let opened_metadata = file.metadata().ok()?;
    if !metadata_matches_entry(&opened_metadata, entry) {
        return None;
    }
    let after_open = safe_path_metadata(path)?;
    if !metadata_matches_entry(&after_open, entry) {
        return None;
    }
    Some(file)
}

fn media_headers() -> Vec<(String, String)> {
    vec![
        ("Content-Type".to_owned(), "video/webm".to_owned()),
        ("Accept-Ranges".to_owned(), "bytes".to_owned()),
        ("Cache-Control".to_owned(), "no-store".to_owned()),
        ("X-Content-Type-Options".to_owned(), "nosniff".to_owned()),
    ]
}

fn not_found() -> ProtocolResponse {
    ProtocolResponse {
        status: 404,
        headers: media_headers(),
        body: NOT_FOUND_BODY.to_vec(),
    }
}

fn range_not_satisfiable(total: u64) -> ProtocolResponse {
    let mut headers = media_headers();
    headers.push(("Content-Range".to_owned(), format!("bytes */{total}")));
    ProtocolResponse {
        status: 416,
        headers,
        body: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs::{self, File};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

    use super::{is_media_url_path, serve, ProtocolRequest, ProtocolResponse};
    use crate::media::prewarm::{VisualEntry, VisualIndex};

    const INSTALL_ID: &str = "ins_0000000001";
    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let base = std::env::current_exe()
                .ok()
                .and_then(|exe| exe.ancestors().nth(3).map(|target| target.join("tmp")))
                .unwrap_or_else(std::env::temp_dir);
            fs::create_dir_all(&base).expect("create Cargo target temporary root");
            let index = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0);
            let path = base.join(format!(
                "kohbor-s3-5j-protocol-{label}-{}-{nanos}-{index}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create unique protocol test directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct Fixture {
        _temp: TempDir,
        path: PathBuf,
        index: VisualIndex,
    }

    fn fixture(bytes: &[u8]) -> Fixture {
        let temp = TempDir::new("fixture");
        let path = temp.path().join("visual.webm");
        fs::write(&path, bytes).expect("write visual fixture");
        let metadata = fs::symlink_metadata(&path).expect("inspect visual fixture");
        let modified = metadata.modified().expect("read visual fixture time");
        let entry = VisualEntry {
            path: path.clone(),
            size_bytes: metadata.len(),
            modified,
        };
        let index = VisualIndex {
            install_id: Some(Arc::from(INSTALL_ID)),
            entries: HashMap::from([(TOKEN.to_owned(), entry)]),
        };
        Fixture {
            _temp: temp,
            path,
            index,
        }
    }

    fn path_for(install_id: &str, token: &str) -> String {
        format!("/v1/{install_id}/{token}")
    }

    fn request(path: impl Into<String>) -> ProtocolRequest {
        ProtocolRequest {
            method: "GET".to_owned(),
            path: path.into(),
            query: None,
            range: None,
        }
    }

    fn request_with_range(path: impl Into<String>, range: &str) -> ProtocolRequest {
        ProtocolRequest {
            range: Some(range.to_owned()),
            ..request(path)
        }
    }

    fn header<'a>(response: &'a ProtocolResponse, name: &str) -> Option<&'a str> {
        response
            .headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn media_request() -> ProtocolRequest {
        request(path_for(INSTALL_ID, TOKEN))
    }

    fn assert_416(response: &ProtocolResponse, total: u64) {
        assert_eq!(response.status, 416);
        let content_range = format!("bytes */{total}");
        assert_eq!(
            header(response, "Content-Range"),
            Some(content_range.as_str())
        );
    }

    #[test]
    fn valid_get_serves_the_exact_file_and_required_headers() {
        let fixture = fixture(b"synthetic-webm-payload");
        let response = serve(&media_request(), &fixture.index);

        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"synthetic-webm-payload");
        assert_eq!(header(&response, "Content-Type"), Some("video/webm"));
        assert_eq!(header(&response, "Accept-Ranges"), Some("bytes"));
        assert_eq!(header(&response, "Cache-Control"), Some("no-store"));
        assert_eq!(header(&response, "X-Content-Type-Options"), Some("nosniff"));
    }

    #[test]
    fn head_keeps_get_status_and_headers_without_a_body() {
        let fixture = fixture(b"synthetic-webm-payload");
        let get = serve(&media_request(), &fixture.index);
        let mut request = media_request();
        request.method = "HEAD".to_owned();

        let response = serve(&request, &fixture.index);

        assert_eq!(response.status, 200);
        assert_eq!(response.headers, get.headers);
        assert!(response.body.is_empty());
        assert_eq!(header(&response, "Content-Type"), Some("video/webm"));
        assert_eq!(header(&response, "Accept-Ranges"), Some("bytes"));
        assert_eq!(header(&response, "Cache-Control"), Some("no-store"));
        assert_eq!(header(&response, "X-Content-Type-Options"), Some("nosniff"));
    }

    #[test]
    fn media_path_helper_accepts_only_the_route_shape() {
        assert!(is_media_url_path(&path_for(INSTALL_ID, TOKEN)));
        for path in [
            "",
            "/",
            "/v1/",
            "/v1/ins_0000000001/short",
            "/v1/INS_0000000001/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "/v1/ins_0000000001/0123456789abcdef0123456789abcdef0123456789abcdef0123456789ABCDEf",
            "/v1/ins_0000000001/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef/extra",
        ] {
            assert!(!is_media_url_path(path), "unexpected media path acceptance: {path}");
        }
    }

    #[test]
    fn closed_and_open_ranges_return_partial_content() {
        let fixture = fixture(b"0123456789");
        for (range, want, content_range) in [
            ("bytes=2-5", b"2345".as_slice(), "bytes 2-5/10"),
            ("bytes=7-", b"789".as_slice(), "bytes 7-9/10"),
        ] {
            let response = serve(
                &request_with_range(path_for(INSTALL_ID, TOKEN), range),
                &fixture.index,
            );
            assert_eq!(response.status, 206);
            assert_eq!(response.body, want);
            assert_eq!(header(&response, "Content-Range"), Some(content_range));
        }
    }

    #[test]
    fn suffix_range_returns_the_last_requested_bytes() {
        let fixture = fixture(b"0123456789");
        let response = serve(
            &request_with_range(path_for(INSTALL_ID, TOKEN), "bytes=-4"),
            &fixture.index,
        );
        assert_eq!(response.status, 206);
        assert_eq!(response.body, b"6789");
        assert_eq!(header(&response, "Content-Range"), Some("bytes 6-9/10"));
    }

    #[test]
    fn one_byte_range_is_served() {
        let fixture = fixture(b"0123456789");
        let response = serve(
            &request_with_range(path_for(INSTALL_ID, TOKEN), "bytes=4-4"),
            &fixture.index,
        );
        assert_eq!(response.status, 206);
        assert_eq!(response.body, b"4");
        assert_eq!(header(&response, "Content-Range"), Some("bytes 4-4/10"));
    }

    #[test]
    fn explicit_end_is_clamped_to_the_file_end() {
        let fixture = fixture(b"0123456789");
        let response = serve(
            &request_with_range(path_for(INSTALL_ID, TOKEN), "bytes=7-999"),
            &fixture.index,
        );
        assert_eq!(response.status, 206);
        assert_eq!(response.body, b"789");
        assert_eq!(header(&response, "Content-Range"), Some("bytes 7-9/10"));
    }

    #[test]
    fn range_body_is_clamped_to_four_mib() {
        let payload = vec![b'x'; 5 * 1024 * 1024];
        let fixture = fixture(&payload);
        let response = serve(
            &request_with_range(path_for(INSTALL_ID, TOKEN), "bytes=0-5242879"),
            &fixture.index,
        );
        assert_eq!(response.status, 206);
        assert_eq!(response.body.len(), 4 * 1024 * 1024);
        assert_eq!(
            header(&response, "Content-Range"),
            Some("bytes 0-4194303/5242880")
        );
    }

    #[test]
    fn beyond_end_multiple_and_malformed_ranges_are_unsatisfiable() {
        let fixture = fixture(b"0123456789");
        for range in [
            "bytes=10-10",
            "bytes=0-1,4-5",
            "items=0-1",
            "bytes=abc-def",
            "bytes=2-1",
            "bytes=-0",
            "bytes=1-2-3",
        ] {
            let response = serve(
                &request_with_range(path_for(INSTALL_ID, TOKEN), range),
                &fixture.index,
            );
            assert_416(&response, 10);
        }
    }

    #[test]
    fn head_range_has_the_same_partial_headers_and_no_body() {
        let fixture = fixture(b"0123456789");
        let mut request = request_with_range(path_for(INSTALL_ID, TOKEN), "bytes=2-4");
        request.method = "HEAD".to_owned();

        let response = serve(&request, &fixture.index);
        assert_eq!(response.status, 206);
        assert_eq!(header(&response, "Content-Range"), Some("bytes 2-4/10"));
        assert!(response.body.is_empty());
    }

    #[test]
    fn zero_length_file_has_empty_full_response_and_unsatisfiable_ranges() {
        let fixture = fixture(b"");
        let full = serve(&media_request(), &fixture.index);
        assert_eq!(full.status, 200);
        assert!(full.body.is_empty());

        let ranged = serve(
            &request_with_range(path_for(INSTALL_ID, TOKEN), "bytes=0-"),
            &fixture.index,
        );
        assert_416(&ranged, 0);
    }

    #[test]
    fn methods_other_than_get_and_head_share_the_not_found_response() {
        let fixture = fixture(b"payload");
        let mut expected: Option<ProtocolResponse> = None;
        for method in ["POST", "PUT", "DELETE", "OPTIONS"] {
            let mut request = media_request();
            request.method = method.to_owned();
            let response = serve(&request, &fixture.index);
            assert_eq!(response.status, 404);
            if let Some(expected) = &expected {
                assert_eq!(&response, expected);
            } else {
                expected = Some(response);
            }
        }
    }

    #[test]
    fn traversal_encoded_backslash_uppercase_and_wrong_length_paths_are_hidden() {
        let fixture = fixture(b"payload");
        let invalid_paths = [
            "/v1/../ins_0000000001/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "/v1/ins_0000000001/../../secret",
            "/v1/%2e%2e/ins_0000000001/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "/v1/ins_0000000001\\0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "/v1/ins_0000000001/0123456789abcdef0123456789abcdef0123456789abcdef0123456789ABCDEf",
            "/v1/ins_0000000001/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde",
            "/v1/ins_0000000001/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0",
            "/v1/ins_000000000x/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "/v1/ins_00000000001/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "",
        ];
        for path in invalid_paths {
            assert_eq!(serve(&request(path), &fixture.index).status, 404, "{path}");
        }
    }

    #[test]
    fn wrong_install_id_missing_token_query_trailing_slash_and_extra_segment_are_hidden() {
        let fixture = fixture(b"payload");
        for path in [
            "/v1/ins_0000000002/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "/v1/ins_0000000001/ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            "/v1/ins_0000000001/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef/",
            "/v1/ins_0000000001/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef/extra",
        ] {
            assert_eq!(serve(&request(path), &fixture.index).status, 404, "{path}");
        }

        let mut queried = media_request();
        queried.query = Some("x=1".to_owned());
        assert_eq!(serve(&queried, &fixture.index).status, 404);
    }

    #[test]
    fn missing_active_install_and_unknown_token_are_not_found() {
        let fixture = fixture(b"payload");
        let mut no_install = fixture.index.clone();
        no_install.install_id = None;
        assert_eq!(serve(&media_request(), &no_install).status, 404);

        let mut unknown = media_request();
        unknown.path = path_for(INSTALL_ID, &"f".repeat(64));
        assert_eq!(serve(&unknown, &fixture.index).status, 404);
    }

    #[test]
    fn replacing_file_with_a_different_length_is_not_found() {
        let fixture = fixture(b"original");
        fs::write(&fixture.path, b"different length").expect("replace visual contents");
        assert_eq!(serve(&media_request(), &fixture.index).status, 404);
    }

    #[test]
    fn replacing_file_with_same_length_but_different_time_is_not_found() {
        let fixture = fixture(b"original");
        let old_time = fs::symlink_metadata(&fixture.path)
            .expect("inspect original visual")
            .modified()
            .expect("read original visual time");
        fs::write(&fixture.path, b"replaced").expect("replace visual contents");
        let new_time = old_time
            .checked_add(Duration::from_secs(10))
            .expect("form a distinct timestamp");
        File::options()
            .write(true)
            .open(&fixture.path)
            .expect("open replaced visual")
            .set_times(std::fs::FileTimes::new().set_modified(new_time))
            .expect("set a different replacement timestamp");
        assert_eq!(serve(&media_request(), &fixture.index).status, 404);
    }

    #[cfg(unix)]
    #[test]
    fn replacing_file_with_a_symlink_is_not_found() {
        use std::os::unix::fs::symlink;

        let fixture = fixture(b"original");
        let target = fixture._temp.path().join("elsewhere.webm");
        fs::write(&target, b"original").expect("write symlink target");
        fs::remove_file(&fixture.path).expect("remove indexed file");
        symlink(&target, &fixture.path).expect("create symlink in indexed file position");
        assert_eq!(serve(&media_request(), &fixture.index).status, 404);
    }

    #[cfg(windows)]
    #[test]
    fn replacing_file_with_a_junction_is_not_found() {
        use std::process::Command;

        let fixture = fixture(b"original");
        let target = fixture._temp.path().join("junction-target");
        fs::create_dir(&target).expect("create junction target directory");
        fs::write(target.join("visual.webm"), b"original").expect("write junction target file");
        fs::remove_file(&fixture.path).expect("remove indexed file");
        let result = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&fixture.path)
            .arg(&target)
            .output();
        match result {
            Ok(output) if output.status.success() => {
                assert_eq!(serve(&media_request(), &fixture.index).status, 404);
                fs::remove_dir(&fixture.path).expect("remove test junction");
            }
            Ok(output) => println!(
                "junction test skipped: mklink /J failed ({}): {}{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
            Err(error) => println!("junction test skipped: could not run mklink /J: {error}"),
        }
    }

    #[test]
    fn replacing_file_with_a_directory_is_not_found() {
        let fixture = fixture(b"original");
        fs::remove_file(&fixture.path).expect("remove indexed file");
        fs::create_dir(&fixture.path).expect("replace file with directory");
        assert_eq!(serve(&media_request(), &fixture.index).status, 404);
    }

    #[test]
    fn recorded_file_size_over_thirty_two_mib_is_not_found() {
        let mut fixture = fixture(b"small file");
        let entry = fixture
            .index
            .entries
            .get_mut(TOKEN)
            .expect("fixture index entry");
        entry.size_bytes = 32 * 1024 * 1024 + 1;
        assert_eq!(serve(&media_request(), &fixture.index).status, 404);
    }

    #[test]
    fn every_404_refusal_has_the_same_body_and_headers() {
        let fixture = fixture(b"payload");
        let mut missing_file = fixture.index.clone();
        missing_file
            .entries
            .get_mut(TOKEN)
            .expect("fixture index entry")
            .path = fixture.path.with_file_name("absent.webm");
        let mut wrong_id = media_request();
        wrong_id.path = path_for("ins_0000000002", TOKEN);
        let mut wrong_query = media_request();
        wrong_query.query = Some(String::new());
        let mut wrong_method = media_request();
        wrong_method.method = "POST".to_owned();
        let responses = [
            serve(&request("/invalid"), &fixture.index),
            serve(&wrong_id, &fixture.index),
            serve(&wrong_query, &fixture.index),
            serve(&wrong_method, &fixture.index),
            serve(&media_request(), &missing_file),
        ];
        assert!(responses.iter().all(|response| response.status == 404));
        assert!(responses.iter().all(|response| response == &responses[0]));
    }

    #[test]
    fn responses_do_not_expose_the_filesystem_path_or_install_id() {
        let fixture = fixture(b"synthetic-webm-payload");
        let ranged = request_with_range(path_for(INSTALL_ID, TOKEN), "bytes=0-1");
        let bad_range = request_with_range(path_for(INSTALL_ID, TOKEN), "bytes=99-100");
        let responses = [
            serve(&media_request(), &fixture.index),
            serve(&request("/v1/../private"), &fixture.index),
            serve(&ranged, &fixture.index),
            serve(&bad_range, &fixture.index),
        ];
        for response in responses {
            for (name, value) in response.headers {
                assert!(!name.contains(INSTALL_ID));
                assert!(!value.contains(INSTALL_ID));
                assert!(!name.contains(&fixture.path.to_string_lossy().to_string()));
                assert!(!value.contains(&fixture.path.to_string_lossy().to_string()));
            }
            assert!(!String::from_utf8_lossy(&response.body).contains(INSTALL_ID));
            assert!(!String::from_utf8_lossy(&response.body)
                .contains(&fixture.path.to_string_lossy().to_string()));
        }
    }
}
