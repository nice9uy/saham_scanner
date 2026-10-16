//! Chart candlestick lewat `public/chart.js` (TradingView Lightweight Charts).

use leptos::html::Div;
use leptos::prelude::*;
use shared::CandlesDto;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ScannerChart, js_name = render)]
    fn chart_render(el: &web_sys::HtmlElement, json: &str);
    #[wasm_bindgen(js_namespace = ScannerChart, js_name = destroy)]
    fn chart_destroy(el: &web_sys::HtmlElement);
}

#[component]
pub fn PriceChart(#[prop(into)] data: Signal<Option<CandlesDto>>, #[prop(into)] height: Signal<u32>) -> impl IntoView {
    let node = NodeRef::<Div>::new();
    Effect::new(move |_| {
        if let (Some(el), Some(d)) = (node.get(), data.get())
            && let Ok(json) = serde_json::to_string(&d) {
                chart_render(&el, &json);
            }
    });
    on_cleanup(move || {
        if let Some(el) = node.get_untracked() {
            chart_destroy(&el);
        }
    });
    view! { <div node_ref=node class="w-full" style=move || format!("height: {}px", height.get())></div> }
}
