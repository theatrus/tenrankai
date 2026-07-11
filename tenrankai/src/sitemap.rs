//! Automatic `sitemap.xml` generation for publicly visible resources.
//!
//! The sitemap is generated per site (resolved from the request via
//! [`ResolvedState`]). It covers static pages (`pages/*.html.liquid`), publicly
//! viewable gallery folders and image detail pages, and posts. When the total
//! number of URLs exceeds [`MAX_URLS_PER_SITEMAP`] the response at `/sitemap.xml`
//! becomes a sitemap index that points at `/sitemap/<chunk>.xml` files instead
//! of a single `<urlset>`.
//!
//! Generated documents are rendered once and written to a per-site temporary
//! directory; requests serve those files until the [`SITEMAP_CACHE_TTL`]
//! expires. This keeps crawler traffic from re-walking the gallery tree and
//! keeps the (potentially large) rendered XML out of resident memory.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, OnceLock, PoisonError},
    time::{Duration, Instant},
};

use axum::{
    extract::Path,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use tempfile::TempDir;

use crate::{AppState, permissions::resolve_permissions_for_path, site::ResolvedState};

/// Maximum number of `<url>` entries in a single sitemap file. The sitemap
/// protocol allows up to 50,000; we keep headroom for safety.
const MAX_URLS_PER_SITEMAP: usize = 45_000;

/// How long a generated sitemap is reused before being rebuilt. Keeping this
/// short bounds staleness; keeping it non-zero lets a crawler fetch the sitemap
/// index and every chunk without re-walking the gallery tree on each request.
const SITEMAP_CACHE_TTL: Duration = Duration::from_secs(300);

const SITEMAP_XMLNS: &str = "http://www.sitemaps.org/schemas/sitemap/0.9";
const XML_CONTENT_TYPE: &str = "application/xml; charset=utf-8";

/// A single `<url>` entry in a sitemap.
struct UrlEntry {
    loc: String,
    /// W3C datetime string, if known.
    lastmod: Option<String>,
}

impl UrlEntry {
    fn new(loc: String) -> Self {
        Self { loc, lastmod: None }
    }

    fn with_lastmod(loc: String, lastmod: String) -> Self {
        Self {
            loc,
            lastmod: Some(lastmod),
        }
    }
}

/// What document a request wants from the generated sitemap.
enum SitemapTarget {
    /// `/sitemap.xml` — either a single `<urlset>` or a `<sitemapindex>`.
    Index,
    /// `/sitemap/<id>.xml` — one `<urlset>` chunk.
    Chunk(String),
}

/// `GET /sitemap.xml`
///
/// Returns a `<urlset>` directly when everything fits in one file, or a
/// `<sitemapindex>` referencing `/sitemap/<chunk>.xml` files otherwise.
pub async fn sitemap_index_handler(ResolvedState(app_state): ResolvedState) -> Response {
    let Some(base_url) = site_base_url(&app_state) else {
        return base_url_missing();
    };
    match sitemap_bytes(&app_state, &base_url, &SitemapTarget::Index).await {
        Some(bytes) => xml_response(bytes),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// `GET /sitemap/{file}` — serves an individual sitemap chunk referenced by the
/// sitemap index. Only present when the sitemap is large enough to be split.
pub async fn sitemap_chunk_handler(
    ResolvedState(app_state): ResolvedState,
    Path(file): Path<String>,
) -> Response {
    let Some(base_url) = site_base_url(&app_state) else {
        return base_url_missing();
    };
    let Some(id) = file.strip_suffix(".xml") else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match sitemap_bytes(&app_state, &base_url, &SitemapTarget::Chunk(id.to_string())).await {
        Some(bytes) => xml_response(bytes),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// A generated sitemap persisted to a per-site temporary directory. The
/// directory holds `index.xml` plus one `<id>.xml` per chunk; it is removed
/// when this value (and any in-flight reads holding a clone) are dropped.
struct CachedSitemap {
    base_url: String,
    built_at: Instant,
    dir: TempDir,
    /// Chunk ids with a file on disk; empty unless the sitemap was split.
    chunk_ids: Vec<String>,
}

impl CachedSitemap {
    fn path_for(&self, target: &SitemapTarget) -> Option<std::path::PathBuf> {
        match target {
            SitemapTarget::Index => Some(self.dir.path().join("index.xml")),
            SitemapTarget::Chunk(id) => self
                .chunk_ids
                .iter()
                .any(|known| known == id)
                .then(|| self.dir.path().join(format!("{id}.xml"))),
        }
    }
}

fn sitemap_cache() -> &'static Mutex<HashMap<String, Arc<CachedSitemap>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<CachedSitemap>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn fresh_cached_sitemap(app_state: &AppState, base_url: &str) -> Option<Arc<CachedSitemap>> {
    let cache = sitemap_cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let entry = cache.get(app_state.app_name())?;
    (entry.base_url == base_url && entry.built_at.elapsed() < SITEMAP_CACHE_TTL)
        .then(|| entry.clone())
}

fn store_cached_sitemap(app_state: &AppState, entry: Arc<CachedSitemap>) {
    sitemap_cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(app_state.app_name().to_string(), entry);
}

/// Return the bytes of the requested sitemap document, using the per-site cache
/// when fresh and rebuilding (and re-persisting) otherwise. Returns `None` only
/// when a chunk that does not exist was requested. If the temporary directory
/// cannot be written, the freshly rendered document is served directly without
/// caching.
async fn sitemap_bytes(
    app_state: &AppState,
    base_url: &str,
    target: &SitemapTarget,
) -> Option<Vec<u8>> {
    if let Some(entry) = fresh_cached_sitemap(app_state, base_url) {
        match entry.path_for(target) {
            Some(path) => {
                if let Ok(bytes) = std::fs::read(&path) {
                    return Some(bytes);
                }
                // Fall through and rebuild on a read error (e.g. directory was
                // pruned from under us).
            }
            None => return None,
        }
    }

    let (index_doc, chunk_docs) = render_documents(app_state, base_url).await;

    if let Some(entry) = persist_documents(base_url, &index_doc, &chunk_docs) {
        let bytes = entry
            .path_for(target)
            .and_then(|path| std::fs::read(path).ok());
        store_cached_sitemap(app_state, entry);
        if let Some(bytes) = bytes {
            return Some(bytes);
        }
    }

    // Persistence failed (or the read of what we just wrote failed): serve from
    // the in-memory render so the request still succeeds.
    match target {
        SitemapTarget::Index => Some(index_doc.into_bytes()),
        SitemapTarget::Chunk(id) => chunk_docs
            .into_iter()
            .find(|(chunk_id, _)| chunk_id == id)
            .map(|(_, doc)| doc.into_bytes()),
    }
}

/// Render the sitemap into its document strings: the `/sitemap.xml` document and
/// (when the sitemap is split) one `<urlset>` per chunk id.
async fn render_documents(app_state: &AppState, base_url: &str) -> (String, Vec<(String, String)>) {
    let chunks = build_chunks(app_state, base_url).await;
    match chunks.len() {
        0 => (render_urlset(&[]), Vec::new()),
        1 => (render_urlset(&chunks[0].1), Vec::new()),
        _ => {
            let index = render_sitemap_index(base_url, &chunks);
            let chunk_docs = chunks
                .iter()
                .map(|(id, urls)| (id.clone(), render_urlset(urls)))
                .collect();
            (index, chunk_docs)
        }
    }
}

/// Write the rendered documents to a fresh temporary directory. Returns `None`
/// if any write fails (the caller falls back to serving from memory).
fn persist_documents(
    base_url: &str,
    index_doc: &str,
    chunk_docs: &[(String, String)],
) -> Option<Arc<CachedSitemap>> {
    let dir = match tempfile::Builder::new()
        .prefix("tenrankai-sitemap-")
        .tempdir()
    {
        Ok(dir) => dir,
        Err(e) => {
            tracing::warn!("Failed to create sitemap temp directory: {}", e);
            return None;
        }
    };
    if let Err(e) = std::fs::write(dir.path().join("index.xml"), index_doc) {
        tracing::warn!("Failed to write sitemap index file: {}", e);
        return None;
    }
    let mut chunk_ids = Vec::with_capacity(chunk_docs.len());
    for (id, doc) in chunk_docs {
        if let Err(e) = std::fs::write(dir.path().join(format!("{id}.xml")), doc) {
            tracing::warn!("Failed to write sitemap chunk file {}: {}", id, e);
            return None;
        }
        chunk_ids.push(id.clone());
    }
    Some(Arc::new(CachedSitemap {
        base_url: base_url.to_string(),
        built_at: Instant::now(),
        dir,
        chunk_ids,
    }))
}

fn site_base_url(app_state: &AppState) -> Option<String> {
    let base = app_state.base_url()?.trim_end_matches('/');
    if base.is_empty() {
        None
    } else {
        Some(base.to_string())
    }
}

fn base_url_missing() -> Response {
    (
        StatusCode::NOT_FOUND,
        "sitemap unavailable: base_url is not configured for this site\n",
    )
        .into_response()
}

/// Split the site's URLs into `(chunk-id, urls)` pairs. URLs are collected in a
/// deterministic order so chunk ids stay stable between the sitemap index and
/// the chunk endpoints.
async fn build_chunks(app_state: &AppState, base_url: &str) -> Vec<(String, Vec<UrlEntry>)> {
    let all_urls = collect_urls(app_state, base_url).await;
    if all_urls.is_empty() {
        return Vec::new();
    }

    let mut chunks: Vec<(String, Vec<UrlEntry>)> = Vec::new();
    let mut iter = all_urls.into_iter().peekable();
    let mut part = 0usize;
    while iter.peek().is_some() {
        part += 1;
        let urls: Vec<UrlEntry> = iter.by_ref().take(MAX_URLS_PER_SITEMAP).collect();
        chunks.push((format!("sitemap-{part}"), urls));
    }
    chunks
}

async fn collect_urls(app_state: &AppState, base_url: &str) -> Vec<UrlEntry> {
    let mut urls = Vec::new();

    collect_page_urls(app_state, base_url, &mut urls).await;

    let mut gallery_names: Vec<&String> = app_state.galleries().keys().collect();
    gallery_names.sort();
    for name in gallery_names {
        collect_gallery_urls(app_state, base_url, name, &mut urls).await;
    }

    let mut posts_names: Vec<&String> = app_state.posts_managers().keys().collect();
    posts_names.sort();
    for name in posts_names {
        collect_posts_urls(app_state, base_url, name, &mut urls).await;
    }

    urls
}

async fn collect_page_urls(app_state: &AppState, base_url: &str, urls: &mut Vec<UrlEntry>) {
    let engine = app_state.template_engine();

    if engine.template_exists("pages/index.html.liquid").await {
        urls.push(UrlEntry::new(format!("{base_url}/")));
    }

    for route in engine.list_page_routes().await {
        // Skip internal pages such as `_login/...`.
        if route.split('/').next().unwrap_or("").starts_with('_') {
            continue;
        }
        urls.push(UrlEntry::new(format!(
            "{base_url}/{}",
            encode_path_segments(&route)
        )));
    }
}

async fn collect_gallery_urls(
    app_state: &AppState,
    base_url: &str,
    name: &str,
    urls: &mut Vec<UrlEntry>,
) {
    let Some(gallery) = app_state.galleries().get(name) else {
        return;
    };
    let url_prefix = gallery.get_config().url_prefix.clone();

    let mut folder_urls: Vec<UrlEntry> = Vec::new();
    let mut image_urls: Vec<UrlEntry> = Vec::new();

    // Breadth-first walk of the (already cached) folder tree. Hidden folders are
    // omitted from `subdirectories`, so they are never visited.
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(String::new());
    while let Some(path) = queue.pop_front() {
        let Some(cached) = gallery.get_cached_folder_data(&path).await else {
            continue;
        };

        // Queue children regardless of this folder's visibility: permissions are
        // resolved per folder, so a private folder may still contain public
        // children reachable by direct URL.
        for subdir in &cached.subdirectories {
            let child = if path.is_empty() {
                subdir.clone()
            } else {
                format!("{path}/{subdir}")
            };
            queue.push_back(child);
        }

        let viewable = matches!(
            resolve_permissions_for_path(app_state, name, &path, None).await,
            Ok(perms) if perms.permissions.can_view
        );
        if !viewable {
            continue;
        }

        let folder_loc = if path.is_empty() {
            format!("{base_url}{url_prefix}")
        } else {
            format!("{base_url}{url_prefix}/{}", encode_path_segments(&path))
        };
        folder_urls.push(UrlEntry::new(folder_loc));

        let hidden_images: &[String] = cached
            .metadata
            .as_ref()
            .map(|m| m.config.hidden_images.as_slice())
            .unwrap_or(&[]);

        // When grouping is active, only primary images appear in the gallery;
        // mirror that here.
        let image_paths: Vec<&str> = if cached.image_groups.is_empty() {
            cached.images.iter().map(String::as_str).collect()
        } else {
            cached
                .image_groups
                .iter()
                .map(|group| group.primary_path.as_str())
                .collect()
        };

        for image_path in image_paths {
            let filename = image_path.rsplit('/').next().unwrap_or(image_path);
            if hidden_images.iter().any(|hidden| hidden == filename) {
                continue;
            }
            let url_id = gallery.build_url_identifier(image_path).await;
            image_urls.push(UrlEntry::new(format!(
                "{base_url}{url_prefix}/detail/{}",
                encode_path_segments(&url_id)
            )));
        }
    }

    urls.append(&mut folder_urls);
    urls.append(&mut image_urls);
}

async fn collect_posts_urls(
    app_state: &AppState,
    base_url: &str,
    name: &str,
    urls: &mut Vec<UrlEntry>,
) {
    let Some(manager) = app_state.posts_managers().get(name) else {
        return;
    };
    let config = manager.get_config();

    urls.push(UrlEntry::new(format!("{base_url}{}", config.url_prefix)));

    // Publicly visible category index pages
    for category in manager.get_categories(None).await {
        urls.push(UrlEntry::new(format!(
            "{base_url}{}/category/{}",
            config.url_prefix, category.slug
        )));
    }

    let total_pages = manager.get_total_pages(None, None).await;
    for page in 0..total_pages {
        for post in manager.get_posts_page(page, None, None).await {
            urls.push(UrlEntry::with_lastmod(
                format!("{base_url}{}", post.url),
                post.date.to_rfc3339(),
            ));
        }
    }
}

/// Percent-encode each `/`-separated segment of a path. Segments never contain
/// `/`, so re-joining them is lossless.
fn encode_path_segments(path: &str) -> String {
    path.split('/')
        .map(|segment| urlencoding::encode(segment).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn xml_response(body: Vec<u8>) -> Response {
    ([(header::CONTENT_TYPE, XML_CONTENT_TYPE)], body).into_response()
}

fn render_urlset(urls: &[UrlEntry]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!("<urlset xmlns=\"{SITEMAP_XMLNS}\">\n"));
    for url in urls {
        out.push_str("  <url><loc>");
        out.push_str(&xml_escape(&url.loc));
        out.push_str("</loc>");
        if let Some(lastmod) = &url.lastmod {
            out.push_str("<lastmod>");
            out.push_str(&xml_escape(lastmod));
            out.push_str("</lastmod>");
        }
        out.push_str("</url>\n");
    }
    out.push_str("</urlset>\n");
    out
}

fn render_sitemap_index(base_url: &str, chunks: &[(String, Vec<UrlEntry>)]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!("<sitemapindex xmlns=\"{SITEMAP_XMLNS}\">\n"));
    for (id, _) in chunks {
        out.push_str("  <sitemap><loc>");
        out.push_str(&xml_escape(&format!("{base_url}/sitemap/{id}.xml")));
        out.push_str("</loc></sitemap>\n");
    }
    out.push_str("</sitemapindex>\n");
    out
}

pub(crate) fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_path_segments_encodes_each_segment() {
        assert_eq!(encode_path_segments("vacation/2024"), "vacation/2024");
        assert_eq!(
            encode_path_segments("My Trips/Rock & Roll"),
            "My%20Trips/Rock%20%26%20Roll"
        );
    }

    #[test]
    fn xml_escape_escapes_markup() {
        assert_eq!(xml_escape("a&b<c>"), "a&amp;b&lt;c&gt;");
        assert_eq!(xml_escape("\"x'y\""), "&quot;x&apos;y&quot;");
    }

    #[test]
    fn render_urlset_emits_locs_and_lastmod() {
        let urls = vec![
            UrlEntry::new("https://example.com/".to_string()),
            UrlEntry::with_lastmod(
                "https://example.com/blog/post".to_string(),
                "2024-01-02T00:00:00+00:00".to_string(),
            ),
        ];
        let xml = render_urlset(&urls);
        assert!(xml.contains("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">"));
        assert!(xml.contains("<loc>https://example.com/</loc>"));
        assert!(xml.contains(
            "<loc>https://example.com/blog/post</loc><lastmod>2024-01-02T00:00:00+00:00</lastmod>"
        ));
        assert!(xml.trim_end().ends_with("</urlset>"));
    }

    #[test]
    fn render_sitemap_index_lists_chunks() {
        let chunks = vec![
            ("sitemap-1".to_string(), Vec::new()),
            ("sitemap-2".to_string(), Vec::new()),
        ];
        let xml = render_sitemap_index("https://example.com", &chunks);
        assert!(xml.contains("<sitemapindex"));
        assert!(xml.contains("<loc>https://example.com/sitemap/sitemap-1.xml</loc>"));
        assert!(xml.contains("<loc>https://example.com/sitemap/sitemap-2.xml</loc>"));
    }
}
