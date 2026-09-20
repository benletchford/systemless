//! Credential-free public HTTPS ingestion with bounded reads and DNS pinning.
use crate::catalogue_tools::validate;
use anyhow::{ensure, Context, Result};
use reqwest::{blocking::Client, redirect::Policy};
use std::{
    io::{Read, Write},
    net::{IpAddr, ToSocketAddrs},
    time::Duration,
};
use url::Url;

pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || a == 0
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 192 && b == 0 && c == 0)
                || (a == 192 && b == 88 && c == 99)
                || (a == 198 && (b == 18 || b == 19)))
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            // Only global unicast, excluding special-purpose/tunnelling/documentation allocations.
            (s[0] & 0xe000) == 0x2000
                && s[0] != 0x2002
                && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8))
                && !(s[0] == 0x3fff && s[1] < 0x1000)
        }
    }
}

pub fn public_url(url: &Url) -> Result<()> {
    validate::https(url.as_str())?;
    ensure!(
        url.port_or_known_default() == Some(443),
        "download URLs must use HTTPS port 443"
    );
    let host = url.host_str().context("URL has no host")?;
    ensure!(
        host != "localhost" && !host.ends_with(".localhost") && !host.ends_with(".local"),
        "local download hosts are forbidden"
    );
    match url.host() {
        Some(url::Host::Ipv4(ip)) => {
            ensure!(public_ip(IpAddr::V4(ip)), "download IP is not public")
        }
        Some(url::Host::Ipv6(ip)) => {
            ensure!(public_ip(IpAddr::V6(ip)), "download IP is not public")
        }
        _ => {}
    }
    Ok(())
}

/// Each redirect is independently validated and resolved, then pinned to vetted public IPs.
/// Proxy environment variables are disabled so they cannot bypass those checks.
pub fn download(url: &str, destination: &mut impl Write, max_bytes: u64) -> Result<u64> {
    download_with_referer(url, destination, max_bytes, None, Default::default())
}

fn download_with_referer(
    url: &str,
    destination: &mut impl Write,
    max_bytes: u64,
    referer: Option<&str>,
    cookies: std::sync::Arc<reqwest::cookie::Jar>,
) -> Result<u64> {
    let mut url = validate::https(url)?;
    for _ in 0..=5 {
        public_url(&url)?;
        let host = url.host_str().unwrap();
        let addresses: Vec<_> = (host.trim_matches(['[', ']']), 443)
            .to_socket_addrs()?
            .collect();
        ensure!(
            !addresses.is_empty() && addresses.iter().all(|a| public_ip(a.ip())),
            "download host resolves to a non-public address"
        );
        let client = Client::builder()
            .no_proxy()
            .cookie_provider(cookies.clone())
            .https_only(true)
            .user_agent(concat!("catalogue/", env!("CARGO_PKG_VERSION")))
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(600))
            .resolve_to_addrs(host, &addresses)
            .build()?;
        let mut request = client
            .get(url.clone())
            .header("Accept-Encoding", "identity")
            .header("Cache-Control", "no-cache");
        if let Some(referer) = referer {
            request = request.header("Referer", referer);
        }
        let response = request.send().context("public asset download failed")?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get("location")
                .context("redirect missing Location")?
                .to_str()?;
            url = url.join(location)?;
            continue;
        }
        ensure!(
            response.status() == reqwest::StatusCode::OK,
            "download returned {} for {}{}",
            response.status(),
            url.host_str().unwrap_or_default(),
            url.path()
        );
        ensure!(
            response
                .headers()
                .get("content-encoding")
                .is_none_or(|v| v == "identity"),
            "encoded download bodies are not supported"
        );
        ensure!(
            response.content_length().is_none_or(|n| n <= max_bytes),
            "download exceeds size limit"
        );
        let mut limited = response.take(max_bytes + 1);
        let bytes = std::io::copy(&mut limited, destination)?;
        ensure!(
            bytes > 0 && bytes <= max_bytes,
            "download is empty or exceeds size limit"
        );
        return Ok(bytes);
    }
    anyhow::bail!("too many HTTPS redirects")
}

const PAGE_LIMIT: u64 = 512 * 1024;

/// Resolve a fresh query-bearing link without letting a page select another host or file.
pub fn resolve_download_link(page: &str, target: &str, html: &str) -> Result<String> {
    ensure!(
        html.len() as u64 <= PAGE_LIMIT,
        "download page exceeds size limit"
    );
    let page = validate::https(page)?;
    let target = validate::https(target)?;
    public_url(&page)?;
    public_url(&target)?;
    let document = scraper::Html::parse_document(html);
    let selector = scraper::Selector::parse("a[href]").expect("fixed selector");
    let mut matches = std::collections::BTreeSet::new();
    for link in document.select(&selector) {
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        let Ok(candidate) = page.join(href) else {
            continue;
        };
        if candidate.origin() == target.origin() && candidate.path() == target.path() {
            public_url(&candidate)?;
            matches.insert(candidate.to_string());
        }
    }
    ensure!(
        matches.len() == 1,
        "download page must contain exactly one distinct link matching the declared origin and path"
    );
    Ok(matches.into_iter().next().unwrap())
}

pub fn download_via_page(
    target: &str,
    page: &str,
    destination: &mut impl Write,
    max_bytes: u64,
) -> Result<u64> {
    let mut html = Vec::new();
    let cookies = std::sync::Arc::new(reqwest::cookie::Jar::default());
    download_with_referer(page, &mut html, PAGE_LIMIT, None, cookies.clone())?;
    let html = std::str::from_utf8(&html).context("download page must be UTF-8")?;
    let url = resolve_download_link(page, target, html)?;
    download_with_referer(&url, destination, max_bytes, Some(page), cookies)
}
