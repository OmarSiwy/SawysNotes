#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, Element, HtmlElement, IntersectionObserver, IntersectionObserverInit, IntersectionObserverEntry};
use std::cell::RefCell;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);

    #[wasm_bindgen(js_namespace = ["window", "MathJax"], js_name = typesetPromise)]
    fn math_jax_typeset_promise();
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    
    let window = window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");

    // Init Logic
    init_scroll_spy()?;
    init_theme()?;
    init_sidebar_state()?;
    format_notes()?;
    highlight_sidebar()?;
    
    // Setup htmx:afterSwap listener
    let closure = Closure::wrap(Box::new(move |_event: web_sys::Event| {
        // Unsafe block for FFI check if MathJax is defined
        let _ = js_sys::eval("if(window.MathJax) window.MathJax.typesetPromise()");

        // Re-apply sidebar/layout state
        let _ = init_sidebar_state();
        let _ = init_scroll_spy();
        let _ = format_notes();
        let _ = highlight_sidebar();
        
    }) as Box<dyn FnMut(_)>);

    document.body().expect("body").add_event_listener_with_callback("htmx:afterSwap", closure.as_ref().unchecked_ref())?;
    closure.forget(); // leakage is intended for global listener
    
    attach_click_listeners()?;

    Ok(())
}

fn attach_click_listeners() -> Result<(), JsValue> {
    let window = window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");

    // Sidebar Toggle
    if let Some(btn) = document.query_selector(".sidebar-toggle")? {
        let closure = Closure::wrap(Box::new(move || {
            toggle_sidebar();
        }) as Box<dyn FnMut()>);
        let html_btn = btn.dyn_into::<HtmlElement>()?;
        html_btn.set_onclick(Some(closure.as_ref().unchecked_ref()));
        closure.forget();
    }

    // Theme Toggle
    if let Some(btn) = document.query_selector(".theme-toggle")? {
        let closure = Closure::wrap(Box::new(move || {
            toggle_theme();
        }) as Box<dyn FnMut()>);
        let html_btn = btn.dyn_into::<HtmlElement>()?;
        html_btn.set_onclick(Some(closure.as_ref().unchecked_ref()));
        closure.forget();
    }

    Ok(())
}

fn toggle_sidebar() {
    let window = window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");

    let mut is_closed = false;
    if let Ok(Some(sidebar)) = document.query_selector(".sidebar") {
        is_closed = sidebar.class_list().toggle("closed").unwrap_or(false);
    }
    if let Ok(Some(content)) = document.query_selector(".content") {
        let _ = content.class_list().toggle("expanded");
    }
    
    // Save state
    if let Ok(Some(storage)) = window.local_storage() {
         let _ = storage.set_item("sidebar_closed", if is_closed { "true" } else { "false" });
    }
}

fn init_sidebar_state() -> Result<(), JsValue> {
    let window = window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    
    // Default to closed unless user has explicitly set it to open
    let should_close = if let Ok(Some(storage)) = window.local_storage() {
        match storage.get_item("sidebar_closed") {
            Ok(Some(closed)) => closed != "false", // Close unless explicitly set to "false"
            _ => true, // No preference stored, default to closed
        }
    } else {
        true // No storage available, default to closed
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
    let window = window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let html = document.document_element().expect("html element");

    let current = html.get_attribute("data-theme").unwrap_or_else(|| "light".into());
    let next = if current == "dark" { "light" } else { "dark" };
    
    let _ = html.set_attribute("data-theme", next);
    
    if let Ok(Some(storage)) = window.local_storage() {
        let _ = storage.set_item("theme", next);
    }
}

fn init_theme() -> Result<(), JsValue> {
    let window = window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    
    if let Ok(Some(storage)) = window.local_storage() {
        if let Ok(Some(saved)) = storage.get_item("theme") {
            let html = document.document_element().expect("html element");
            let _ = html.set_attribute("data-theme", &saved);
        }
    }
    Ok(())
}

fn format_notes() -> Result<(), JsValue> {
    let window = window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");

    let blockquotes = document.query_selector_all("blockquote")?;
    
    for i in 0..blockquotes.length() {
        if let Some(bq) = blockquotes.get(i) {
            let bq_el = bq.dyn_into::<Element>()?;
            if let Some(p) = bq_el.query_selector("p")? {
                let p_html = p.dyn_into::<HtmlElement>()?;
                let text = p_html.text_content().unwrap_or_default().trim().to_string();
                
                if text.starts_with("[!NOTE]") {
                    let _ = bq_el.class_list().add_1("note");
                    // Replace [!NOTE]
                    // We need to be careful with innerHTML replacement to matching JS behavior
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
    let window = window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let location = window.location();
    let current_path = location.pathname()?;

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
    let window = window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");

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
                 let target = entry.target();
                 if let Some(text) = target.text_content() {
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
    callback.forget(); // Leak closure

    for i in 0..headers.length() {
        if let Some(h) = headers.get(i) {
             let el = h.dyn_into::<Element>()?;
             observer.observe(&el);
        }
    }
    
    OBSERVER.with(|obs| {
        *obs.borrow_mut() = Some(observer);
    });

    Ok(())
}
