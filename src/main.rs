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

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/:chapter", get(chapter_handler))
        .route("/:chapter/:topic", get(topic_handler))
        .nest_service("/assets", ServeDir::new("assets"))
        .nest_service("/content", ServeDir::new("assets/content"))
        .nest_service("/dist", ServeDir::new("dist"))
        .nest_service("/style", ServeDir::new("style"));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("listening on {}", addr);
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
    items: Vec<SidebarItem>,
}

fn generate_sidebar() -> Vec<SidebarItem> {
    let mut structure: BTreeMap<String, Vec<SidebarItem>> = BTreeMap::new();
    let content_dir = "assets/content";

    for entry in WalkDir::new(content_dir).min_depth(1).sort_by_file_name() {
        let entry = entry.unwrap();
        let path = entry.path();
        
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let relative_path = path.strip_prefix(content_dir).unwrap();
            let components: Vec<_> = relative_path.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();

            if components.len() == 1 {
                // Top level file (like index.md), skip or handle separately? 
                // Index is usually home, so maybe skip for now or add as Home
                continue;
            }

            let chapter = components[0].clone();
            let file_stem = path.file_stem().unwrap().to_string_lossy().to_string();
            
            let title = format_title(&file_stem);
            let link = format!("/{}", relative_path.with_extension("").to_string_lossy());
             
             // Very basic 2-level structure assumption for now based on current layout
            if components.len() == 2 {
                structure.entry(chapter).or_default().push(SidebarItem {
                    title,
                    path: link,
                    children: vec![],
                });
            }
        }
    }

    let mut sidebar_items = Vec::new();
    for (chapter, children) in structure {
        if chapter == "images" { continue; }
        
        let title = format_title(&chapter);
        let path = format!("/{}", chapter);
        
        sidebar_items.push(SidebarItem {
            title,
            path,
            children,
        });
    }
    
    sidebar_items
}

async fn index_handler(headers: HeaderMap) -> impl IntoResponse {
    render_page("index", None, headers).await
}

async fn chapter_handler(Path(chapter): Path<String>, headers: HeaderMap) -> impl IntoResponse {
    render_page(&chapter, None, headers).await
}

async fn topic_handler(Path((chapter, topic)): Path<(String, String)>, headers: HeaderMap) -> impl IntoResponse {
    render_page(&chapter, Some(&topic), headers).await
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

async fn render_page(chapter: &str, topic: Option<&str>, headers: HeaderMap) -> Response {
    let file_path = if let Some(t) = topic {
        format!("assets/content/{}/{}.md", chapter, t)
    } else {
        format!("assets/content/{}.md", chapter)
    };

    let markdown_input = match fs::read_to_string(&file_path).await {
        Ok(content) => content,
        Err(_) => return Html("<h1>404 Not Found</h1>".to_string()).into_response(),
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
    // Debug: Print HTML to see what we are matching against
    println!("DEBUG HTML: {}", html_output);

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

    let sidebar = SidebarTemplate {
        active_path: format!("/{}{}", chapter, topic.map(|t| format!("/{}", t)).unwrap_or_default()),
        items: generate_sidebar(),
    };
    
    let page_title = if let Some(t) = topic {
        format_title(t)
    } else {
        if chapter == "index" {
            "Sawy's Notes".to_string()
        } else {
            format_title(chapter)
        }
    };

    let layout = LayoutTemplate {
        title: "Sawy's Notes",
        page_title,
        sidebar: &sidebar.render().unwrap(),
        content: &html_output,
        theme: "light", // Default to light, JS handles toggle
    };

    Html(layout.render().unwrap()).into_response()
}


