//! The project site's live chart. Built for the web with Trunk: `trunk build --release` from this
//! directory, or `trunk serve` to work on the page.

#[cfg(target_arch = "wasm32")]
mod hero;

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast;

    console_error_panic_hook::set_once();
    eframe::WebLogger::init(log::LevelFilter::Info).ok();

    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document
        .get_element_by_id("hero_canvas")
        .unwrap()
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .unwrap();

    let focused_before = document.active_element();
    wasm_bindgen_futures::spawn_local(async move {
        let started = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(hero::HeroApp::new(&cc.egui_ctx)))),
            )
            .await;
        // The page shows the canvas only once the chart runs, and an explanation if it cannot.
        let state = match started {
            Ok(()) => {
                give_focus_back(&document, focused_before);
                "ready"
            }
            Err(err) => {
                log::error!("The live chart could not start: {err:?}");
                "failed"
            }
        };
        if let Some(figure) = document.get_element_by_id("hero") {
            figure.set_attribute("data-state", state).ok();
        }
    });
}

/// eframe focuses the chart when it starts, and keeps a hidden text input for typing next to it.
/// The chart has no text fields, so the page keeps its focus and the input stays out of the tab
/// order, which leaves the keyboard scrolling and tabbing through the page.
#[cfg(target_arch = "wasm32")]
fn give_focus_back(document: &web_sys::Document, focused_before: Option<web_sys::Element>) {
    use wasm_bindgen::JsCast;

    if let Ok(Some(input)) = document.query_selector("#hero input") {
        input.set_attribute("tabindex", "-1").ok();
        input.set_attribute("aria-hidden", "true").ok();
    }
    let hero = document.get_element_by_id("hero");
    if let Some(focused) = document
        .active_element()
        .filter(|el| {
            hero.as_ref()
                .is_some_and(|hero| hero.contains(Some(el.as_ref())))
        })
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
    {
        focused.blur().ok();
    }
    if let Some(before) = focused_before
        .filter(|el| el.tag_name() != "BODY")
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
    {
        before.focus().ok();
    } else if let Some(body) = document.body() {
        // Focusing the body moves the browser's tab starting point back to the top of the page;
        // after the blur it would stay in the chart, and Tab would skip the links above it.
        body.set_attribute("tabindex", "-1").ok();
        body.focus().ok();
        body.remove_attribute("tabindex").ok();
    }
}

// The site is built for the web only; this keeps `cargo check` working on other targets.
#[cfg(not(target_arch = "wasm32"))]
fn main() {}
