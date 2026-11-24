//! Storage backends for spaces.
//!
//! A [`Space`](crate::space::Space) keeps all of its metadata (objects, refs,
//! index, config, reflogs) in a [`Store`]: an opendal operator injected at
//! construction. Any opendal service works — local disk, S3, GCS, WebDAV,
//! in-memory, … — because the space only ever talks to the operator.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::OnceLock;

use opendal::layers::BlockingLayer;
use opendal::{Operator, Scheme};

use crate::error::DuhError;
use crate::space::SpaceResult;

/// The storage handle a space is built on.
pub type Store = opendal::BlockingOperator;

/// Runtime driving backends that only have async APIs (most network
/// services). Created on first use and kept for the life of the process.
fn runtime() -> SpaceResult<&'static tokio::runtime::Runtime> {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    if let Some(rt) = RUNTIME.get() {
        return Ok(rt);
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    Ok(RUNTIME.get_or_init(|| rt))
}

/// Turn any operator into a [`Store`], adding opendal's blocking layer
/// (backed by duh's runtime) when the service has no native blocking API.
pub fn from_operator(op: Operator) -> SpaceResult<Store> {
    if op.info().native_capability().blocking {
        return Ok(op.blocking());
    }
    let _guard = runtime()?.enter();
    Ok(op.layer(BlockingLayer::create()?).blocking())
}

/// A store on the local filesystem rooted at `root` (a space's `.duh/`).
/// Writes go through a temporary file so objects are never half-written.
pub fn local(root: &Path) -> SpaceResult<Store> {
    let root = root.to_str().ok_or("non-UTF-8 path")?;
    let builder = opendal::services::Fs::default()
        .root(root)
        .atomic_write_dir(&format!("{}/.tmp", root.trim_end_matches('/')));
    from_operator(Operator::new(builder)?.finish())
}

/// Opendal service and options for `url`, merged with `options` (which win).
///
/// - Plain paths and `file://` URLs point at a local space: its worktree
///   (containing `.duh/`) or the metadata directory itself.
/// - Any other URL's scheme names an opendal service (`s3://bucket/path`,
///   `gcs://…`, `webdav://host/path`, `memory://`, …). The host, port,
///   user info and path become the usual options (`bucket` for s3/gcs/…,
///   `endpoint`, `root`, …) and query parameters are passed through, so
///   `s3://bucket/repo?region=eu-west-1` works.
/// - `options` carries everything else a service needs (credentials,
///   region, …) under opendal's option names for that service.
pub fn config_for_url(
    url: &str,
    options: &[(String, String)],
) -> SpaceResult<(Scheme, Vec<(String, String)>)> {
    let (scheme, mut pairs) = parse_url(url)?;
    for (k, v) in options {
        upsert(&mut pairs, k, v.clone());
    }
    Ok((scheme, pairs))
}

/// Check that `url` names a known backend, without contacting it or
/// requiring a local path to exist yet.
pub fn validate_url(url: &str) -> SpaceResult<()> {
    match url::Url::parse(url) {
        Ok(u) if u.scheme() != "file" => parse_url(url).map(|_| ()),
        _ => Ok(()),
    }
}

/// The form of `url` to save in a config: local paths become absolute (so
/// the remote works from any directory); everything else is unchanged.
pub fn normalize_url(url: &str) -> SpaceResult<String> {
    match url::Url::parse(url) {
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            let path = std::env::current_dir()?.join(url);
            let path = path.canonicalize().unwrap_or(path);
            Ok(path.to_string_lossy().to_string())
        }
        _ => Ok(url.to_string()),
    }
}

/// Parse `key=value` (as given to `--option`).
pub fn parse_option(s: &str) -> SpaceResult<(String, String)> {
    match s.split_once('=') {
        Some((k, v)) if !k.trim().is_empty() => Ok((k.trim().to_string(), v.to_string())),
        _ => Err(format!("expected KEY=VALUE, got '{}'", s).into()),
    }
}

/// A store for `url` with extra service `options` (see [`config_for_url`]).
pub fn from_url(url: &str, options: &[(String, String)]) -> SpaceResult<Store> {
    let (scheme, pairs) = config_for_url(url, options)?;
    from_operator(Operator::via_iter(scheme, pairs)?)
}

fn upsert(pairs: &mut Vec<(String, String)>, key: &str, value: String) {
    if let Some((_, v)) = pairs.iter_mut().find(|(k, _)| k == key) {
        *v = value;
    } else {
        pairs.push((key.to_string(), value));
    }
}

/// Resolve a local repo path (worktree or metadata dir) to fs options.
fn local_pairs(url: &str, path: PathBuf) -> SpaceResult<(Scheme, Vec<(String, String)>)> {
    let meta = path.join(crate::utils::SPACE_METADATA_DIR_NAME);
    let root = if meta.is_dir() { meta } else { path };
    if !root.join("config").is_file() {
        return Err(Box::new(DuhError::Generic(format!(
            "remote path '{}' is not a valid duh repo (missing config)",
            url
        ))));
    }
    let root = root.to_string_lossy().to_string();
    let atomic = format!("{}/.tmp", root.trim_end_matches('/'));
    Ok((
        Scheme::Fs,
        vec![("root".into(), root), ("atomic_write_dir".into(), atomic)],
    ))
}

/// Services whose URL host names a bucket/container rather than a server.
fn host_is_bucket(scheme: &Scheme) -> bool {
    matches!(
        scheme,
        Scheme::S3 | Scheme::Gcs | Scheme::Oss | Scheme::Obs | Scheme::Cos | Scheme::B2
    )
}

fn parse_url(url: &str) -> SpaceResult<(Scheme, Vec<(String, String)>)> {
    let parsed = match url::Url::parse(url) {
        Ok(u) if u.scheme() == "file" => {
            let path = u
                .to_file_path()
                .map_err(|_| format!("invalid file URL '{}'", url))?;
            return local_pairs(url, path);
        }
        Ok(u) => u,
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            let path = PathBuf::from(url);
            let abs = if path.is_absolute() {
                path
            } else {
                std::env::current_dir()?.join(path)
            };
            return local_pairs(url, abs);
        }
        Err(e) => return Err(Box::new(e)),
    };

    // Scheme::from_str maps unknown names to Scheme::Custom rather than failing.
    let scheme = match Scheme::from_str(parsed.scheme()) {
        Ok(Scheme::Custom(_)) | Err(_) => {
            return Err(format!("unknown storage scheme '{}' in '{}'", parsed.scheme(), url).into())
        }
        Ok(s) => s,
    };
    let mut pairs: Vec<(String, String)> = Vec::new();

    if let Some(host) = parsed.host_str() {
        if host_is_bucket(&scheme) {
            upsert(&mut pairs, "bucket", host.to_string());
        } else {
            // Network services: http(s) endpoint for web-ish protocols,
            // otherwise keep the URL's own scheme.
            let proto = match scheme {
                Scheme::Webdav | Scheme::Http => "https",
                _ => parsed.scheme(),
            };
            let mut endpoint = format!("{}://{}", proto, host);
            if let Some(port) = parsed.port() {
                endpoint.push_str(&format!(":{}", port));
            }
            upsert(&mut pairs, "endpoint", endpoint);
        }
    }
    if !parsed.username().is_empty() {
        upsert(&mut pairs, "username", parsed.username().to_string());
        upsert(&mut pairs, "user", parsed.username().to_string());
    }
    if let Some(password) = parsed.password() {
        upsert(&mut pairs, "password", password.to_string());
    }
    let root = parsed.path();
    if !root.is_empty() && root != "/" {
        upsert(&mut pairs, "root", root.to_string());
    }
    for (k, v) in parsed.query_pairs() {
        upsert(&mut pairs, &k, v.into_owned());
    }
    Ok((scheme, pairs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s3_url_maps_bucket_root_and_query() {
        let (scheme, pairs) =
            config_for_url("s3://my-bucket/repos/game?region=eu-west-1", &[]).unwrap();
        assert_eq!(scheme, Scheme::S3);
        assert!(pairs.contains(&("bucket".into(), "my-bucket".into())));
        assert!(pairs.contains(&("root".into(), "/repos/game".into())));
        assert!(pairs.contains(&("region".into(), "eu-west-1".into())));
    }

    #[test]
    fn options_override_url_values() {
        let (_, pairs) = config_for_url(
            "s3://my-bucket/x?region=a",
            &[("region".into(), "b".into()), ("access_key_id".into(), "AKIA".into())],
        )
        .unwrap();
        assert!(pairs.contains(&("region".into(), "b".into())));
        assert!(pairs.contains(&("access_key_id".into(), "AKIA".into())));
    }

    #[test]
    fn webdav_url_becomes_an_https_endpoint() {
        let (scheme, pairs) =
            config_for_url("webdav://user:pw@dav.example.com:8443/remote.php/files", &[]).unwrap();
        assert_eq!(scheme, Scheme::Webdav);
        assert!(pairs.contains(&("endpoint".into(), "https://dav.example.com:8443".into())));
        assert!(pairs.contains(&("username".into(), "user".into())));
        assert!(pairs.contains(&("password".into(), "pw".into())));
    }

    #[test]
    fn unknown_scheme_is_an_error() {
        assert!(config_for_url("nosuchservice://x", &[]).is_err());
    }
}
