#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, Document, Element, HtmlElement, IntersectionObserver, IntersectionObserverInit, IntersectionObserverEntry, Window};
use std::cell::RefCell;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

/// Get window and document handles
fn get_window_and_doc() -> (Window, Document) {
    let window = window().expect("no global window");
    let document = window.document().expect("no document");
    (window, document)
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let (_, document) = get_window_and_doc();

    init_scroll_spy()?;
    init_theme()?;
    init_sidebar_state()?;
    format_notes()?;
    highlight_sidebar()?;

    let closure = Closure::wrap(Box::new(move |_event: web_sys::Event| {
        let _ = init_sidebar_state();
        let _ = init_scroll_spy();
        let _ = format_notes();
        let _ = highlight_sidebar();
        let _ = attach_dynamic_listeners();
    }) as Box<dyn FnMut(_)>);

    document.body().expect("body").add_event_listener_with_callback("htmx:afterSwap", closure.as_ref().unchecked_ref())?;
    closure.forget();

    attach_static_listeners()?;
    attach_dynamic_listeners()?;
    Ok(())
}

fn attach_static_listeners() -> Result<(), JsValue> {
    let (_, document) = get_window_and_doc();
    if let Some(btn) = document.query_selector(".theme-toggle")? {
        let closure = Closure::wrap(Box::new(toggle_theme) as Box<dyn FnMut()>);
        btn.dyn_into::<HtmlElement>()?.set_onclick(Some(closure.as_ref().unchecked_ref()));
        closure.forget();
    }
    Ok(())
}

fn attach_dynamic_listeners() -> Result<(), JsValue> {
    let (_, document) = get_window_and_doc();
    if let Some(btn) = document.query_selector(".sidebar-toggle")? {
        let closure = Closure::wrap(Box::new(toggle_sidebar) as Box<dyn FnMut()>);
        btn.dyn_into::<HtmlElement>()?.set_onclick(Some(closure.as_ref().unchecked_ref()));
        closure.forget();
    }
    Ok(())
}

fn toggle_sidebar() {
    let (window, document) = get_window_and_doc();
    let mut is_closed = false;
    if let Ok(Some(sidebar)) = document.query_selector(".sidebar") {
        is_closed = sidebar.class_list().toggle("closed").unwrap_or(false);
    }
    if let Ok(Some(content)) = document.query_selector(".content") {
        let _ = content.class_list().toggle("expanded");
    }
    if let Ok(Some(storage)) = window.local_storage() {
        let _ = storage.set_item("sidebar_closed", if is_closed { "true" } else { "false" });
    }
}

fn init_sidebar_state() -> Result<(), JsValue> {
    let (window, document) = get_window_and_doc();
    let should_close = if let Ok(Some(storage)) = window.local_storage() {
        match storage.get_item("sidebar_closed") {
            Ok(Some(closed)) => closed != "false",
            _ => true,
        }
    } else {
        true
    };

    if should_close {
        if let Ok(Some(sidebar)) = document.query_selector(".sidebar") {
            let _ = sidebar.class_list().add_1("closed");
        }
        if let Ok(Some(content)) = document.query_selector(".content") {
            let _ = content.class_list().add_1("expanded");
        }
    }
    Ok(())
}

fn toggle_theme() {
    let (window, document) = get_window_and_doc();
    let html = document.document_element().expect("html element");
    let current = html.get_attribute("data-theme").unwrap_or_else(|| "light".into());
    let next = if current == "dark" { "light" } else { "dark" };
    let _ = html.set_attribute("data-theme", next);
    if let Ok(Some(storage)) = window.local_storage() {
        let _ = storage.set_item("theme", next);
    }
}

fn init_theme() -> Result<(), JsValue> {
    let (window, document) = get_window_and_doc();
    if let Ok(Some(storage)) = window.local_storage() {
        if let Ok(Some(saved)) = storage.get_item("theme") {
            let html = document.document_element().expect("html element");
            let _ = html.set_attribute("data-theme", &saved);
        }
    }
    Ok(())
}

fn format_notes() -> Result<(), JsValue> {
    let (_, document) = get_window_and_doc();
    let blockquotes = document.query_selector_all("blockquote")?;
    for i in 0..blockquotes.length() {
        if let Some(bq) = blockquotes.get(i) {
            let bq_el = bq.dyn_into::<Element>()?;
            if let Some(p) = bq_el.query_selector("p")? {
                let p_html = p.dyn_into::<HtmlElement>()?;
                let text = p_html.text_content().unwrap_or_default().trim().to_string();
                if text.starts_with("[!NOTE]") {
                    let _ = bq_el.class_list().add_1("note");
                    let inner_html = p_html.inner_html();
                    let new_html = inner_html.replacen("[!NOTE]", "<strong>NOTE</strong>", 1);
                    p_html.set_inner_html(&new_html);
                }
            }
        }
    }
    Ok(())
}

fn highlight_sidebar() -> Result<(), JsValue> {
    let (window, document) = get_window_and_doc();
    let current_path = window.location().pathname()?;
    let links = document.query_selector_all(".sidebar a")?;
    for i in 0..links.length() {
        if let Some(link) = links.get(i) {
            let a = link.dyn_into::<Element>()?;
            if let Some(href) = a.get_attribute("href") {
                if href == current_path {
                    let _ = a.class_list().add_1("active");
                } else {
                    let _ = a.class_list().remove_1("active");
                }
            }
        }
    }
    Ok(())
}

fn init_scroll_spy() -> Result<(), JsValue> {
    let (_, document) = get_window_and_doc();

    thread_local! {
        static OBSERVER: RefCell<Option<IntersectionObserver>> = RefCell::new(None);
    }

    OBSERVER.with(|obs| {
        if let Some(o) = obs.borrow().as_ref() {
            o.disconnect();
        }
    });

    let headers = document.query_selector_all(".content h1, .content h2, .content h3")?;
    let center_el = document.get_element_by_id("topbar-center");

    let callback = Closure::wrap(Box::new(move |entries: Vec<IntersectionObserverEntry>, _observer: IntersectionObserver| {
        for entry in entries {
            if entry.is_intersecting() {
                if let Some(text) = entry.target().text_content() {
                    if let Some(el) = &center_el {
                        el.set_text_content(Some(&text));
                    }
                }
            }
        }
    }) as Box<dyn FnMut(Vec<IntersectionObserverEntry>, IntersectionObserver)>);

    let options = IntersectionObserverInit::new();
    options.set_root_margin("0px 0px -80% 0px");
    options.set_threshold(&JsValue::from(0.1));

    let observer = IntersectionObserver::new_with_options(callback.as_ref().unchecked_ref(), &options)?;
    callback.forget();

    for i in 0..headers.length() {
        if let Some(h) = headers.get(i) {
            observer.observe(&h.dyn_into::<Element>()?);
        }
    }

    OBSERVER.with(|obs| {
        *obs.borrow_mut() = Some(observer);
    });

    Ok(())
}
