mod api;
mod components;
mod format;
mod pages;
mod scanner;

use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use components::layout::Shell;
use pages::{backtest::Backtest, dashboard::Dashboard, data::Data, news::News, scanner::Scanner, server::Server, stock::Stock, watchlist::Watchlist};

#[component]
fn NotFound() -> impl IntoView {
    view! {
        <div class="py-20 text-center">
            <p class="eyebrow">"404"</p>
            <h1 class="font-display text-3xl font-bold">"Halaman tidak ditemukan"</h1>
            <a href="/" class="btn-primary mt-6">"Kembali ke Pasar"</a>
        </div>
    }
}

#[component]
fn App() -> impl IntoView {
    view! {
        <Router>
            <Shell>
                <Routes fallback=NotFound>
                    <Route path=path!("/") view=Dashboard />
                    <Route path=path!("/scanner") view=Scanner />
                    <Route path=path!("/stock/:ticker") view=Stock />
                    <Route path=path!("/watchlist") view=Watchlist />
                    <Route path=path!("/backtest") view=Backtest />
                    <Route path=path!("/data") view=Data />
                    <Route path=path!("/berita") view=News />
                    <Route path=path!("/server") view=Server />
                </Routes>
            </Shell>
        </Router>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}
