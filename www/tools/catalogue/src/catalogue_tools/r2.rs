//! R2 S3 transport and complete-set reconciliation. No unrestricted prefix sync is exposed.
use crate::catalogue_tools::{
    assets::{self, DesiredObject, ObjectStore},
    catalogue::{self, Catalogue},
};
use anyhow::{ensure, Context, Result};
use hmac::{Hmac, KeyInit, Mac};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use reqwest::{
    blocking::{Body, Client, Response},
    redirect::Policy,
    Method,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::Path,
    time::Duration,
};

const URI_ENCODE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');
fn encode(s: &str) -> String {
    utf8_percent_encode(s, URI_ENCODE).to_string()
}
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn mac(key: &[u8], value: &str) -> Vec<u8> {
    let mut hmac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC supports any key length");
    hmac.update(value.as_bytes());
    hmac.finalize().into_bytes().to_vec()
}

fn authorization(
    method: &str,
    path: &str,
    query: &str,
    headers: &BTreeMap<String, String>,
    payload_sha256: &str,
    access_key: &str,
    secret: &str,
) -> Result<String> {
    let timestamp = headers
        .get("x-amz-date")
        .context("missing signing timestamp")?;
    let date = timestamp.get(..8).context("invalid signing date")?;
    let scope = format!("{date}/auto/s3/aws4_request");
    let canonical_headers: String = headers
        .iter()
        .map(|(k, v)| {
            format!(
                "{k}:{}\n",
                v.split_whitespace().collect::<Vec<_>>().join(" ")
            )
        })
        .collect();
    let signed_headers = headers.keys().cloned().collect::<Vec<_>>().join(";");
    let request = format!(
        "{method}\n{path}\n{query}\n{canonical_headers}\n{signed_headers}\n{payload_sha256}"
    );
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{timestamp}\n{scope}\n{}",
        digest(request.as_bytes())
    );
    let date_key = mac(format!("AWS4{secret}").as_bytes(), date);
    let region_key = mac(&date_key, "auto");
    let service_key = mac(&region_key, "s3");
    let signing_key = mac(&service_key, "aws4_request");
    let signature = hex::encode(mac(&signing_key, &string_to_sign));
    Ok(format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed_headers}, Signature={signature}"
    ))
}

pub struct R2Store {
    host: String,
    bucket: String,
    access_key: String,
    secret: String,
    client: Client,
}
impl R2Store {
    pub fn from_env() -> Result<Self> {
        let env = |name| {
            std::env::var(name)
                .with_context(|| format!("{name} is required for trusted R2 operations"))
        };
        let account = env("R2_ACCOUNT_ID")?;
        ensure!(
            account.len() == 32 && account.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid R2 account ID"
        );
        let bucket = env("R2_BUCKET")?;
        ensure!(
            (3..=63).contains(&bucket.len())
                && bucket
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                && !bucket.starts_with('-')
                && !bucket.ends_with('-'),
            "invalid R2 bucket name"
        );
        Ok(Self {
            host: format!("{account}.r2.cloudflarestorage.com"),
            bucket,
            access_key: env("R2_ACCESS_KEY_ID")?,
            secret: env("R2_SECRET_ACCESS_KEY")?,
            client: Client::builder()
                .no_proxy()
                .https_only(true)
                .redirect(Policy::none())
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(900))
                .build()?,
        })
    }
    fn request(
        &self,
        method: Method,
        key: Option<&str>,
        params: BTreeMap<String, String>,
        mut headers: BTreeMap<String, String>,
        body: Option<(&Path, &DesiredObject)>,
    ) -> Result<Response> {
        let path = match key {
            Some(k) => format!(
                "/{}/{}",
                self.bucket,
                k.split('/').map(encode).collect::<Vec<_>>().join("/")
            ),
            None => format!("/{}", self.bucket),
        };
        let query = params
            .iter()
            .map(|(k, v)| (encode(k), encode(v)))
            .collect::<BTreeMap<_, _>>()
            .into_iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        let payload = body
            .map(|(_, a)| a.sha256.clone())
            .unwrap_or_else(|| digest(b""));
        headers.insert("host".into(), self.host.clone());
        headers.insert(
            "x-amz-date".into(),
            jiff::Timestamp::now()
                .strftime("%Y%m%dT%H%M%SZ")
                .to_string(),
        );
        headers.insert("x-amz-content-sha256".into(), payload.clone());
        let auth = authorization(
            method.as_str(),
            &path,
            &query,
            &headers,
            &payload,
            &self.access_key,
            &self.secret,
        )?;
        let url = format!(
            "https://{}{path}{}",
            self.host,
            if query.is_empty() {
                String::new()
            } else {
                format!("?{query}")
            }
        );
        let mut request = self
            .client
            .request(method, url)
            .header("authorization", auth);
        for (k, v) in headers {
            request = request.header(k, v);
        }
        if let Some((file, object)) = body {
            request = request.body(Body::sized(File::open(file)?, object.size_bytes));
        }
        request.send().context("R2 request failed")
    }
    fn verify_object(&self, object: &DesiredObject) -> Result<()> {
        let response = self.request(
            Method::HEAD,
            Some(&object.key),
            BTreeMap::new(),
            BTreeMap::new(),
            None,
        )?;
        ensure!(
            response.status().is_success(),
            "R2 object verification returned {}",
            response.status()
        );
        let headers = response.headers();
        ensure!(
            headers
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                == Some(object.size_bytes)
                && headers
                    .get("x-amz-meta-sha256")
                    .and_then(|v| v.to_str().ok())
                    == Some(&object.sha256),
            "R2 immutable object has conflicting size or SHA-256 metadata: {}",
            object.key
        );
        Ok(())
    }
    pub fn inventory(&self) -> Result<Inventory> {
        let mut objects = Vec::new();
        for prefix in assets::MANAGED_PREFIXES {
            let mut token: Option<String> = None;
            let mut seen_tokens = BTreeSet::new();
            loop {
                let mut params = BTreeMap::from([
                    ("list-type".into(), "2".into()),
                    ("prefix".into(), prefix.into()),
                    ("max-keys".into(), "1000".into()),
                ]);
                if let Some(token) = &token {
                    params.insert("continuation-token".into(), token.clone());
                }
                let response = self.request(Method::GET, None, params, BTreeMap::new(), None)?;
                ensure!(
                    response.status().is_success(),
                    "R2 inventory returned {}",
                    response.status()
                );
                let mut xml = String::new();
                response
                    .take(8 * 1024 * 1024 + 1)
                    .read_to_string(&mut xml)?;
                ensure!(xml.len() <= 8 * 1024 * 1024, "R2 inventory page too large");
                let page: ListPage =
                    quick_xml::de::from_str(&xml).context("invalid R2 inventory XML")?;
                ensure!(
                    page.prefix == prefix && page.key_count == page.contents.len(),
                    "R2 inventory prefix or KeyCount mismatch"
                );
                for o in page.contents {
                    ensure!(
                        o.key.starts_with(prefix),
                        "R2 listed an object outside requested prefix"
                    );
                    objects.push(RemoteObject {
                        key: o.key,
                        size_bytes: o.size,
                        last_modified: o.last_modified,
                    });
                }
                if !page.is_truncated {
                    break;
                }
                let next = page
                    .next_continuation_token
                    .context("truncated inventory is missing continuation token")?;
                ensure!(
                    !next.is_empty() && seen_tokens.insert(next.clone()),
                    "repeated or empty R2 continuation token"
                );
                token = Some(next);
            }
        }
        objects.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(Inventory {
            schema_version: 1,
            complete: true,
            prefixes: assets::MANAGED_PREFIXES.map(String::from).to_vec(),
            objects,
        })
    }
    fn delete(&self, key: &str) -> Result<()> {
        ensure!(
            assets::is_managed_key(key),
            "refusing to delete unmanaged object {key}"
        );
        let response = self.request(
            Method::DELETE,
            Some(key),
            BTreeMap::new(),
            BTreeMap::new(),
            None,
        )?;
        ensure!(
            response.status().is_success(),
            "R2 deletion returned {} for {key}",
            response.status()
        );
        Ok(())
    }
}
impl ObjectStore for R2Store {
    fn put_if_absent(&mut self, object: &DesiredObject, file: &Path) -> Result<()> {
        assets::validate_object(object)?;
        let actual = assets::hash_file(file)?;
        ensure!(
            actual.sha256 == object.sha256 && actual.size_bytes == object.size_bytes,
            "staged file changed before upload"
        );
        let headers = BTreeMap::from([
            ("if-none-match".into(), "*".into()),
            ("content-type".into(), object.content_type.clone()),
            (
                "cache-control".into(),
                "public,max-age=31536000,immutable".into(),
            ),
            ("x-amz-meta-sha256".into(), object.sha256.clone()),
        ]);
        let response = self.request(
            Method::PUT,
            Some(&object.key),
            BTreeMap::new(),
            headers,
            Some((file, object)),
        )?;
        ensure!(
            response.status().is_success()
                || response.status() == reqwest::StatusCode::PRECONDITION_FAILED,
            "R2 conditional upload returned {}",
            response.status()
        );
        self.verify_object(object)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ListPage {
    prefix: String,
    key_count: usize,
    is_truncated: bool,
    next_continuation_token: Option<String>,
    #[serde(default, rename = "Contents")]
    contents: Vec<ListObject>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ListObject {
    key: String,
    size: u64,
    last_modified: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub schema_version: u32,
    pub complete: bool,
    pub prefixes: Vec<String>,
    pub objects: Vec<RemoteObject>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteObject {
    pub key: String,
    pub size_bytes: u64,
    pub last_modified: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeletePolicy {
    pub max_delete: usize,
    pub max_delete_percent: u8,
    pub min_age_hours: u32,
    pub allow_empty: bool,
}
impl Default for DeletePolicy {
    fn default() -> Self {
        Self {
            max_delete: 10,
            max_delete_percent: 10,
            min_age_hours: 168,
            allow_empty: false,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema_version: u32,
    pub source_sha256: String,
    pub inventory_sha256: String,
    pub policy: DeletePolicy,
    pub desired: Vec<DesiredObject>,
    pub missing: Vec<String>,
    pub delete: Vec<String>,
    pub protected: Vec<String>,
    pub blockers: Vec<String>,
}

pub fn plan(
    c: &Catalogue,
    inventory: &Inventory,
    policy: DeletePolicy,
    now: jiff::Timestamp,
) -> Result<Plan> {
    catalogue::validate_catalogue(c)?;
    ensure!(
        inventory.schema_version == 1 && inventory.complete,
        "reconciliation requires a complete v1 inventory"
    );
    let mut prefixes = inventory.prefixes.clone();
    prefixes.sort();
    ensure!(
        prefixes == assets::MANAGED_PREFIXES.map(String::from).to_vec(),
        "inventory must cover exactly both managed prefixes"
    );
    ensure!(
        policy.max_delete_percent <= 100 && policy.min_age_hours >= 24,
        "deletion percentage must be ≤100 and grace period ≥24 hours"
    );
    let compiled = catalogue::build(c)?;
    let desired = assets::desired(c)?;
    let want: BTreeMap<_, _> = desired.iter().map(|o| (&o.key, o)).collect();
    let mut inventory = inventory.clone();
    inventory.objects.sort_by(|a, b| a.key.cmp(&b.key));
    let mut seen = BTreeSet::new();
    let mut candidates = Vec::new();
    let mut protected = Vec::new();
    let mut blockers = Vec::new();
    let mut managed_count = 0_usize;
    for o in &inventory.objects {
        ensure!(
            seen.insert(o.key.clone()),
            "duplicate remote inventory key {}",
            o.key
        );
        if !assets::is_managed_key(&o.key) {
            protected.push(o.key.clone());
            continue;
        }
        managed_count += 1;
        if let Some(w) = want.get(&o.key) {
            if w.size_bytes != o.size_bytes {
                blockers.push(format!("size conflict: {}", o.key));
            }
        } else {
            let modified: jiff::Timestamp = o
                .last_modified
                .parse()
                .context("invalid inventory last_modified")?;
            if now.as_second().saturating_sub(modified.as_second())
                < i64::from(policy.min_age_hours) * 3600
            {
                protected.push(o.key.clone());
            } else {
                candidates.push(o.key.clone());
            }
        }
    }
    let missing: Vec<String> = want
        .keys()
        .filter(|key| !seen.contains(key.as_str()))
        .map(|key| (*key).clone())
        .collect();
    for d in &c.documents {
        for a in &d.entry.artifacts {
            if matches!(
                a.source,
                crate::catalogue_tools::model::AssetSource::Incoming { .. }
                    | crate::catalogue_tools::model::AssetSource::Url { .. }
            ) {
                blockers.push(format!("pending promotion: {}:{}", d.entry.id, a.id));
            }
        }
    }
    if desired.is_empty() && !policy.allow_empty {
        blockers.push("empty desired set requires explicit allow_empty".into());
    }
    if !missing.is_empty() {
        blockers.push("desired objects are missing from R2; restore them before deleting".into());
    }
    if candidates.len() > policy.max_delete {
        blockers.push("deletion count exceeds threshold".into());
    }
    if candidates.len() as u128 * 100
        > managed_count as u128 * u128::from(policy.max_delete_percent)
    {
        blockers.push("deletion percentage exceeds threshold".into());
    }
    Ok(Plan {
        schema_version: 1,
        source_sha256: compiled.source_sha256,
        inventory_sha256: digest(&catalogue::json_bytes(&inventory)?),
        policy,
        desired,
        missing,
        delete: candidates,
        protected,
        blockers,
    })
}

/// Transport boundary for guarded reconciliation, injectable for credential-free tests.
pub trait ReconciliationStore {
    fn inventory(&self) -> Result<Inventory>;
    fn delete(&self, key: &str) -> Result<()>;
}
impl ReconciliationStore for R2Store {
    fn inventory(&self) -> Result<Inventory> {
        R2Store::inventory(self)
    }
    fn delete(&self, key: &str) -> Result<()> {
        R2Store::delete(self, key)
    }
}

/// Recompute against a fresh complete inventory under the local mutation lock before deletion.
pub fn reconcile(root: &Path, reviewed: &Plan, store: &dyn ReconciliationStore) -> Result<usize> {
    let _lock = assets::RepoLock::acquire(root)?;
    ensure!(
        reviewed.schema_version == 1 && reviewed.blockers.is_empty(),
        "reviewed plan is blocked or unsupported"
    );
    let c = catalogue::load(root, catalogue::Mode::Production)?;
    let fresh = plan(
        &c,
        &store.inventory()?,
        reviewed.policy.clone(),
        jiff::Timestamp::now(),
    )?;
    ensure!(
        &fresh == reviewed,
        "catalogue, inventory, or deletion candidates changed; review a fresh plan"
    );
    ensure!(fresh.blockers.is_empty(), "reconciliation is blocked");
    for key in &fresh.delete {
        store.delete(key)?;
    }
    Ok(fresh.delete.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signing_is_stable_and_binds_conditional_headers() {
        let mut headers = BTreeMap::from([
            ("host".into(), "example.r2.cloudflarestorage.com".into()),
            ("x-amz-date".into(), "20260912T010203Z".into()),
            ("x-amz-content-sha256".into(), digest(b"")),
            ("if-none-match".into(), "*".into()),
        ]);
        let a = authorization(
            "PUT",
            "/bucket/key",
            "",
            &headers,
            &digest(b""),
            "access",
            "secret",
        )
        .unwrap();
        assert!(a.contains("Credential=access/20260912/auto/s3/aws4_request"));
        assert!(a.contains("SignedHeaders=host;if-none-match;x-amz-content-sha256;x-amz-date"));
        assert!(a.ends_with(
            "Signature=dc86203a15db53a078775e46d1881ecbfd51aa88d11ddad660d3c9a3cec44663"
        ));
        headers.remove("if-none-match");
        assert_ne!(
            a,
            authorization(
                "PUT",
                "/bucket/key",
                "",
                &headers,
                &digest(b""),
                "access",
                "secret"
            )
            .unwrap()
        );
    }
    #[test]
    fn pagination_xml_requires_completeness_fields() {
        let p: ListPage = quick_xml::de::from_str("<ListBucketResult><Prefix>catalogue/media/sha256/</Prefix><KeyCount>0</KeyCount><IsTruncated>true</IsTruncated><NextContinuationToken>a&amp;b</NextContinuationToken></ListBucketResult>").unwrap();
        assert!(p.is_truncated);
        assert_eq!(p.next_continuation_token.as_deref(), Some("a&b"));
        assert!(quick_xml::de::from_str::<ListPage>("<ListBucketResult/>").is_err());
    }
}
