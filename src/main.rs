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

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let mut app = Router::new()
        .route("/", get(index_handler))
        .route("/:category", get(category_handler))
        .route("/:category/:chapter", get(chapter_handler))
        .route("/:category/:chapter/:topic", get(topic_handler))
        .nest_service("/assets", ServeDir::new("assets"))
        .nest_service("/content", ServeDir::new("assets/content"))
        .nest_service("/dist", ServeDir::new("dist"))
        .nest_service("/style", ServeDir::new("style"));

    // If SITE_URL is set (e.g. /SawysNotes), nest the app under that path
    if let Ok(site_url) = std::env::var("SITE_URL") {
        if !site_url.is_empty() {
             app = Router::new().nest(&site_url, app);
        }
    }

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("listening on {}", addr);
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


use regex::Regex;
use walkdir::WalkDir;
use std::collections::BTreeMap;

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
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let relative_path = match path.strip_prefix(content_dir) {
            Ok(p) => p,
            Err(_) => continue,
        };
        
        let components: Vec<_> = relative_path.components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();

        // Skip if in images folder
        if components.iter().any(|c| c == "images") {
            continue;
        }

        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let modified = match metadata.modified() {
            Ok(t) => t,
            Err(_) => continue,
        };

        let file_stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let title = format_title(&file_stem);
        let site_url = std::env::var("SITE_URL").unwrap_or_default();
        let link = format!("{}{}", site_url, format!("/{}", relative_path.with_extension("").to_string_lossy()));
        let category = format_title(&components[0]);


        let datetime: DateTime<Local> = modified.into();
        let date = datetime.format("%b %d, %Y %H:%M").to_string();

        items.push((modified, RecentlyAddedItem {
            title,
            path: link,
            category,
            date,
        }));
    }

    // Sort by modification time, most recent first
    items.sort_by(|a, b| b.0.cmp(&a.0));

    // Return top 10 items
    items.into_iter().take(10).map(|(_, item)| item).collect()
}

fn generate_sidebar() -> Vec<SidebarItem> {
    // Structure: category -> chapter -> topics
    let mut categories: BTreeMap<String, BTreeMap<String, Vec<SidebarItem>>> = BTreeMap::new();
    let content_dir = "assets/content";

    // Auto-discover categories by scanning top-level folders (excluding images)
    if let Ok(entries) = std::fs::read_dir(content_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let folder_name = path.file_name().unwrap().to_string_lossy().to_string();
                if folder_name != "images" {
                    categories.insert(folder_name, BTreeMap::new());
                }
            }
        }
    }

    for entry in WalkDir::new(content_dir).min_depth(1).sort_by_file_name() {
        let entry = entry.unwrap();
        let path = entry.path();
        
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let relative_path = path.strip_prefix(content_dir).unwrap();
            let components: Vec<_> = relative_path.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();

            // Skip category index files (like analog.md, digital.md at root)
            if components.len() == 1 {
                continue;
            }

            let file_stem = path.file_stem().unwrap().to_string_lossy().to_string();
            let title = format_title(&file_stem);
            let site_url = std::env::var("SITE_URL").unwrap_or_default();
            let link = format!("{}{}", site_url, format!("/{}", relative_path.with_extension("").to_string_lossy()));


            if components.len() == 3 {
                // Full path: category/chapter/topic.md
                let category = components[0].clone();
                let chapter = components[1].clone();
                
                categories
                    .entry(category)
                    .or_default()
                    .entry(chapter)
                    .or_default()
                    .push(SidebarItem {
                        title,
                        path: link,
                        children: vec![],
                    });
            } else if components.len() == 2 {
                // Direct category file: category/topic.md (like analog/overview.md)
                let category = components[0].clone();
                
                categories
                    .entry(category)
                    .or_default()
                    .entry("_direct".to_string())
                    .or_default()
                    .push(SidebarItem {
                        title,
                        path: link,
                        children: vec![],
                    });
            }
        }
    }

    let mut sidebar_items = Vec::new();
    for (category, chapters) in categories {
        if category == "images" { continue; }
        
        let mut chapter_items = Vec::new();
        
        for (chapter, topics) in chapters {
            if chapter == "_direct" {
                // Direct topics under category
                chapter_items.extend(topics);
            } else if chapter == "images" {
                continue;
            } else {
                // Chapter with nested topics
                let chapter_title = format_title(&chapter);
                let site_url = std::env::var("SITE_URL").unwrap_or_default();
                let chapter_path = format!("{}{}", site_url, format!("/{}/{}", category, chapter));

                
                chapter_items.push(SidebarItem {
                    title: chapter_title,
                    path: chapter_path,
                    children: topics,
                });
            }
        }
        
        let category_title = format_title(&category);
        let site_url = std::env::var("SITE_URL").unwrap_or_default();
        let category_path = format!("{}{}", site_url, format!("/{}", category));

        
        sidebar_items.push(SidebarItem {
            title: category_title,
            path: category_path,
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
    s.split('_')
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
    // Enable standard behavior which usually passes HTML through in cmark 0.10
    // unless strictly configured otherwise.
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    
    let parser = Parser::new_ext(&markdown_input, options);
    
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);

    let mut current_fig = 1;


    // Regex to match img tags with src and alt in ANY order
    // Matches: <img [stuff] src="..." [stuff] alt="..." [stuff] > OR <img [stuff] alt="..." [stuff] src="..." [stuff] >
    // We also want to capture 'title' if present.
    // Simplifying: Just capture the whole tag and parse attributes manually with simpler regexes is more robust.
    let img_tag_regex = Regex::new(r#"<img\s+([^>]+)/?>"#).unwrap();
    let src_regex = Regex::new(r#"src="([^"]+)""#).unwrap();
    let alt_regex = Regex::new(r#"alt="([^"]*)""#).unwrap();
    let title_regex = Regex::new(r#"title="([^"]*)""#).unwrap();

    let html_string = html_output;
    let html_output = img_tag_regex.replace_all(&html_string, |caps: &regex::Captures| {
        let attrs = &caps[1];
        
        let src = src_regex.captures(attrs).map(|c| c[1].to_string()).unwrap_or_default();
        let alt = alt_regex.captures(attrs).map(|c| c[1].to_string()).unwrap_or_default();
        let title = title_regex.captures(attrs).map(|c| c[1].to_string()).unwrap_or_default();
        
        let style = if !title.is_empty() {
            format!("width: {};", title)
        } else {
            String::new()
        };

        let img_html = format!(r#"<img src="{}" alt="{}" style="{}">"#, src, alt, style);

        if !alt.is_empty() {
            let num = current_fig;
            current_fig += 1;
            format!(
                r#"<figure class="image-container">
                    {}
                    <figcaption><strong>Fig. {}:</strong> {}</figcaption>
                   </figure>"#,
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

    let site_url = std::env::var("SITE_URL").unwrap_or_default();

    let layout = LayoutTemplate {
        title: "Sawy's Notes",
        page_title,
        sidebar: &sidebar.render().unwrap(),
        content: &final_content,
        theme: "light", // Default to light, JS handles toggle
        site_url: &site_url,
    };

    Html(layout.render().unwrap()).into_response()
}


