use axum::{
    extract::Path,
    http::HeaderMap,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use std::net::SocketAddr;
use tower_http::services::ServeDir;
use askama::Template;
use pulldown_cmark::{Parser, Options, html};
use tokio::fs;
use chrono::{DateTime, Local};
use regex::Regex;
use walkdir::WalkDir;
use std::collections::BTreeMap;
use std::sync::LazyLock;

// Compiled regexes for image tag processing
static IMG_TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"<img\s+([^>]+)/?>"#).unwrap());
static SRC_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"src="([^"]+)""#).unwrap());
static ALT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"alt="([^"]*)""#).unwrap());
static TITLE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"title="([^"]*)""#).unwrap());

/// Get the SITE_URL environment variable or empty string
fn get_site_url() -> String {
    std::env::var("SITE_URL").unwrap_or_default()
}

/// Parse a numbered name like "1) MosFETs" into (sort_key, clean_name)
fn parse_numbered_name(name: &str) -> (i32, String) {
    if let Some(pos) = name.find(')') {
        if let Ok(num) = name[..pos].trim().parse::<i32>() {
            return (num, name[pos + 1..].trim().to_string());
        }
    }
    (i32::MAX, name.to_string())
}

/// Build a link path with site_url prefix
fn build_link(path: &str) -> String {
    format!("{}{}", get_site_url(), path)
}

#[tokio::main]
async fn main() {
    let mut app = Router::new()
        .route("/", get(index_handler))
        .route("/:category", get(category_handler))
        .route("/:category/:chapter", get(chapter_handler))
        .route("/:category/:chapter/:topic", get(topic_handler))
        .nest_service("/assets", ServeDir::new("assets"))
        .nest_service("/content", ServeDir::new("assets/content"))
        .nest_service("/dist", ServeDir::new("dist"))
        .nest_service("/style", ServeDir::new("style"));

    if let Ok(site_url) = std::env::var("SITE_URL") {
        if !site_url.is_empty() {
            app = Router::new().nest(&site_url, app);
        }
    }

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[derive(Template)]
#[template(path = "layout.html")]
struct LayoutTemplate<'a> {
    title: &'a str,
    page_title: String,
    sidebar: &'a str,
    content: &'a str,
    theme: &'a str,
    site_url: &'a str,
}

#[derive(Clone, Debug)]
struct SidebarItem {
    title: String,
    path: String,
    children: Vec<SidebarItem>,
}

#[derive(Template)]
#[template(path = "sidebar.html")]
struct SidebarTemplate {
    active_path: String,
    current_category: String,
    items: Vec<SidebarItem>,
}

#[derive(Clone, Debug)]
struct RecentlyAddedItem {
    title: String,
    path: String,
    category: String,
    date: String,
}

fn generate_recently_added() -> Vec<RecentlyAddedItem> {
    let content_dir = "assets/content";
    let mut items: Vec<(std::time::SystemTime, RecentlyAddedItem)> = Vec::new();

    for entry in WalkDir::new(content_dir).min_depth(1).sort_by_file_name() {
        let Ok(entry) = entry else { continue };
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let Ok(relative_path) = path.strip_prefix(content_dir) else { continue };
        let components: Vec<_> = relative_path.components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();

        if components.iter().any(|c| c == "images") {
            continue;
        }

        let Ok(metadata) = std::fs::metadata(path) else { continue };
        let Ok(modified) = metadata.modified() else { continue };

        let file_stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let title = format_title(&file_stem);
        let link = build_link(&format!("/{}", relative_path.with_extension("").to_string_lossy()));
        let category = format_title(&components[0]);
        let datetime: DateTime<Local> = modified.into();

        items.push((modified, RecentlyAddedItem {
            title,
            path: link,
            category,
            date: datetime.format("%b %d, %Y %H:%M").to_string(),
        }));
    }

    items.sort_by(|a, b| b.0.cmp(&a.0));
    items.into_iter().take(10).map(|(_, item)| item).collect()
}

fn generate_sidebar() -> Vec<SidebarItem> {
    let content_dir = "assets/content";
    // Structure: (sort_key, category_name) -> (sort_key, chapter_name) -> topics
    let mut categories: BTreeMap<(i32, String), BTreeMap<(i32, String), Vec<SidebarItem>>> = BTreeMap::new();

    // Auto-discover categories
    if let Ok(entries) = std::fs::read_dir(content_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let folder_name = path.file_name().unwrap().to_string_lossy().to_string();
                if folder_name != "images" {
                    let key = parse_numbered_name(&folder_name);
                    categories.insert(key, BTreeMap::new());
                }
            }
        }
    }

    for entry in WalkDir::new(content_dir).min_depth(1).sort_by_file_name() {
        let Ok(entry) = entry else { continue };
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let relative_path = path.strip_prefix(content_dir).unwrap();
        let components: Vec<_> = relative_path.components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();

        if components.len() == 1 {
            continue;  // Skip category index files
        }

        let file_stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let title = format_title(&file_stem);
        let link = build_link(&format!("/{}", relative_path.with_extension("").to_string_lossy()));

        if components.len() == 3 {
            let cat_key = parse_numbered_name(&components[0]);
            let chap_key = parse_numbered_name(&components[1]);
            categories.entry(cat_key).or_default().entry(chap_key).or_default()
                .push(SidebarItem { title, path: link, children: vec![] });
        } else if components.len() == 2 {
            let cat_key = parse_numbered_name(&components[0]);
            categories.entry(cat_key).or_default()
                .entry((i32::MIN, "_direct".to_string())).or_default()
                .push(SidebarItem { title, path: link, children: vec![] });
        }
    }

    let mut sidebar_items = Vec::new();
    for ((_, category_name), chapters) in categories {
        if category_name == "images" { continue; }

        let mut chapter_items = Vec::new();
        for ((_, chapter_name), topics) in chapters {
            if chapter_name == "_direct" {
                chapter_items.extend(topics);
            } else if chapter_name == "images" {
                continue;
            } else {
                chapter_items.push(SidebarItem {
                    title: format_title(&chapter_name),
                    path: build_link(&format!("/{}/{}", category_name, chapter_name)),
                    children: topics,
                });
            }
        }

        sidebar_items.push(SidebarItem {
            title: format_title(&category_name),
            path: build_link(&format!("/{}", category_name)),
            children: chapter_items,
        });
    }

    sidebar_items
}

async fn index_handler(headers: HeaderMap) -> impl IntoResponse {
    render_page("index", None, None, headers).await
}

async fn category_handler(Path(category): Path<String>, headers: HeaderMap) -> impl IntoResponse {
    render_page(&category, None, None, headers).await
}

async fn chapter_handler(Path((category, chapter)): Path<(String, String)>, headers: HeaderMap) -> impl IntoResponse {
    render_page(&category, Some(&chapter), None, headers).await
}

async fn topic_handler(Path((category, chapter, topic)): Path<(String, String, String)>, headers: HeaderMap) -> impl IntoResponse {
    render_page(&category, Some(&chapter), Some(&topic), headers).await
}

fn format_title(s: &str) -> String {
    // Strip numeric prefix like "1) " first
    let (_, clean_name) = parse_numbered_name(s);
    let name = if clean_name.is_empty() { s } else { &clean_name };

    name.split('_')
        .map(|word| {
            let mut c = word.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

async fn render_page(category: &str, chapter: Option<&str>, topic: Option<&str>, _headers: HeaderMap) -> Response {
    let file_path = match (chapter, topic) {
        (Some(ch), Some(t)) => format!("assets/content/{}/{}/{}.md", category, ch, t),
        (Some(ch), None) => format!("assets/content/{}/{}.md", category, ch),
        (None, _) => format!("assets/content/{}.md", category),
    };



    let markdown_input = match fs::read_to_string(&file_path).await {
        Ok(content) => content,
        Err(_) => {
            return Html("<h1>404 Not Found</h1>".to_string()).into_response();
        }
    };

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    options.insert(Options::ENABLE_MATH);
    
    let parser = Parser::new_ext(&markdown_input, options);
    
    // Transform events to handle math
    let parser = parser.map(|event| {
        match event {
            pulldown_cmark::Event::InlineMath(cow) => {
                // Try render with defaults, which is displayMode: false usually? 
                // Actually katex-rs 'render' might be display mode. 
                // Let's assume 'render' works and returns HTML.
                // We'll trust the error message about 'render_inline' and just use 'render' for now.
                let html = katex::render(&cow).unwrap_or_else(|_| cow.to_string());
                pulldown_cmark::Event::Html(html.into())
            }
            pulldown_cmark::Event::DisplayMath(cow) => {
                let html = katex::render(&cow).unwrap_or_else(|_| cow.to_string());
                pulldown_cmark::Event::Html(html.into())
            }
            _ => event,
        }
    });

    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);

    let mut current_fig = 1;
    let html_string = html_output;
    let html_output = IMG_TAG_RE.replace_all(&html_string, |caps: &regex::Captures| {
        let attrs = &caps[1];
        let src = SRC_RE.captures(attrs).map(|c| c[1].to_string()).unwrap_or_default();
        let alt = ALT_RE.captures(attrs).map(|c| c[1].to_string()).unwrap_or_default();
        let title = TITLE_RE.captures(attrs).map(|c| c[1].to_string()).unwrap_or_default();
        let style = if !title.is_empty() { format!("width: {};", title) } else { String::new() };
        let img_html = format!(r#"<img src="{}" alt="{}" style="{}">"#, src, alt, style);

        if !alt.is_empty() {
            let num = current_fig;
            current_fig += 1;
            format!(
                r#"<figure class="image-container">\n{}\n<figcaption><strong>Fig. {}:</strong> {}</figcaption>\n</figure>"#,
                img_html, num, alt
            )
        } else {
            img_html
        }
    }).to_string();

    let active_path = match (chapter, topic) {
        (Some(ch), Some(t)) => format!("/{}/{}/{}", category, ch, t),
        (Some(ch), None) => format!("/{}/{}", category, ch),
        (None, _) => format!("/{}", category),
    };

    let current_category = if category == "index" {
        String::new()
    } else {
        category.to_string()
    };

    let sidebar = SidebarTemplate {
        active_path,
        current_category,
        items: generate_sidebar(),
    };
    
    let page_title = if let Some(t) = topic {
        format_title(t)
    } else if let Some(ch) = chapter {
        format_title(ch)
    } else {
        if category == "index" {
            "Sawy's Notes".to_string()
        } else {
            format_title(category)
        }
    };

    // Inject recently added section for index page
    let final_content = if category == "index" {
        let recent_items = generate_recently_added();
        let mut recently_added_html = String::from(r#"
<h2>📝 Recently Added</h2>
<table>
<thead>
<tr><th>Page</th><th>Category</th><th>Date Added</th></tr>
</thead>
<tbody>
"#);
        for item in recent_items {
            recently_added_html.push_str(&format!(
                r#"<tr><td><a href="{}">{}</a></td><td>{}</td><td>{}</td></tr>
"#,
                item.path, item.title, item.category, item.date
            ));
        }
        recently_added_html.push_str("</tbody>\n</table>");
        
        format!("{}\n{}", html_output, recently_added_html)
    } else {
        html_output
    };

    let site_url = get_site_url();
    let layout = LayoutTemplate {
        title: "Sawy's Notes",
        page_title,
        sidebar: &sidebar.render().unwrap(),
        content: &final_content,
        theme: "light",
        site_url: &site_url,
    };

    Html(layout.render().unwrap()).into_response()
}
