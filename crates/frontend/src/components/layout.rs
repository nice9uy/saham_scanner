use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;
use shared::MoverDto;

use crate::{api, format};

const NAV: &[(&str, &str, &str)] = &[
    ("/", "Pasar", "◎"),
    ("/scanner", "Scanner", "⌕"),
    ("/watchlist", "Watchlist", "☆"),
    ("/backtest", "Uji Sinyal", "∿"),
    ("/data", "Data", "⇅"),
    ("/berita", "Berita", "▤"),
    ("/server", "Server", "▣"),
];

#[component]
pub fn Shell(children: Children) -> impl IntoView {
    view! {
        <div class="min-h-screen lg:grid lg:grid-cols-[13rem_minmax(0,1fr)]">
            <aside class="sticky top-0 hidden h-screen flex-col border-r border-line px-5 py-6 lg:flex">
                <a href="/" class="mb-10 block">
                    <span class="font-display text-2xl font-bold tracking-tight">"Scanner"</span>
                    <span class="ml-1.5 rounded bg-brass px-1.5 py-0.5 font-mono text-[10px] font-medium text-ink">"IDX"</span>
                </a>
                <nav class="flex flex-col gap-1" aria-label="Navigasi utama">
                    {NAV
                        .iter()
                        .map(|(href, label, icon)| {
                            view! {
                                <A
                                    href=*href
                                    exact=true
                                    attr:class="flex items-center gap-3 rounded-md px-3 py-2 text-sm text-muted transition-colors hover:text-paper aria-[current=page]:bg-raised aria-[current=page]:text-paper"
                                >
                                    <span class="w-4 text-center font-mono text-brass" aria-hidden="true">{*icon}</span>
                                    {*label}
                                </A>
                            }
                        })
                        .collect_view()}
                </nav>
                <p class="mt-auto font-mono text-[10px] leading-relaxed text-muted">
                    "Data harian Yahoo Finance. Bukan rekomendasi jual/beli."
                </p>
            </aside>
            <div class="min-w-0 pb-20 lg:pb-0">
                // Tape dan pencarian menempel bersama di atas saat halaman digulir.
                <div class="sticky top-0 z-20">
                    <Tape />
                    <div class="flex items-center gap-3 border-b border-line bg-ink/95 px-4 py-3 backdrop-blur sm:px-6 lg:px-10">
                        <a href="/" class="font-display text-lg font-bold lg:hidden">"Scanner"</a>
                        <StockSearch />
                    </div>
                </div>
                <main class="px-4 py-6 sm:px-6 lg:px-10 lg:py-8">{children()}</main>
            </div>
            <nav
                class="fixed inset-x-0 bottom-0 z-20 grid grid-cols-7 border-t border-line bg-panel/95 backdrop-blur lg:hidden"
                aria-label="Navigasi"
            >
                {NAV
                    .iter()
                    .map(|(href, label, icon)| {
                        view! {
                            <A
                                href=*href
                                exact=true
                                attr:class="flex flex-col items-center gap-0.5 py-2.5 text-[11px] text-muted aria-[current=page]:text-brass"
                            >
                                <span class="font-mono text-base" aria-hidden="true">{*icon}</span>
                                {*label}
                            </A>
                        }
                    })
                    .collect_view()}
            </nav>
        </div>
    }
}

/// Running trade: saham penggerak hari ini berjalan di atas layar.
#[component]
fn Tape() -> impl IntoView {
    let summary = LocalResource::new(api::market_summary);
    // Harga intraday diperbarui backend tiap beberapa menit selama jam bursa.
    crate::components::ui::every(60, move || summary.refetch());
    let items = move || -> Vec<MoverDto> {
        let Some(Ok(s)) = summary.get() else { return vec![] };
        let mut seen = std::collections::HashSet::new();
        s.top_value
            .into_iter()
            .chain(s.top_gainers)
            .chain(s.top_losers)
            .filter(|m| seen.insert(m.ticker.clone()))
            .collect()
    };
    let cell = |m: &MoverDto| {
        let href = format!("/stock/{}", m.ticker);
        let tone = format::tone(Some(m.change_pct));
        let arrow = if m.change_pct > 0.0 { "▲" } else if m.change_pct < 0.0 { "▼" } else { "■" };
        view! {
            <a href=href class="flex items-baseline gap-2 px-5 font-mono text-xs hover:text-brass">
                <span class="font-medium text-paper">{m.ticker.clone()}</span>
                <span class="text-muted">{format::price(m.close)}</span>
                <span class=tone>{arrow} " " {format::pct(Some(m.change_pct))}</span>
            </a>
        }
    };
    view! {
        <div class="tape overflow-hidden border-b border-line bg-panel/95 backdrop-blur" aria-label="Penggerak pasar hari ini">
            {move || {
                let list = items();
                if list.is_empty() {
                    view! {
                        <p class="px-5 py-2 font-mono text-xs text-muted">"Running trade muncul setelah data diperbarui"</p>
                    }
                        .into_any()
                } else {
                    view! {
                        <div class="tape-track flex w-max py-2">
                            {list.iter().map(cell).collect_view()}
                            <span aria-hidden="true" class="contents">{list.iter().map(cell).collect_view()}</span>
                        </div>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

#[component]
fn StockSearch() -> impl IntoView {
    let (query, set_query) = signal(String::new());
    let results = LocalResource::new(move || {
        let q = query.get();
        async move { if q.trim().is_empty() { Ok(vec![]) } else { api::stocks(&q, 8).await } }
    });
    let navigate = use_navigate();
    let go = move |ticker: String| {
        set_query.set(String::new());
        navigate(&format!("/stock/{ticker}"), Default::default());
    };
    let go_first = go.clone();

    view! {
        <div class="relative w-full max-w-md">
            <label for="cari-saham" class="sr-only">"Cari saham"</label>
            <input
                id="cari-saham"
                type="search"
                autocomplete="off"
                placeholder="Cari kode atau nama saham…"
                class="w-full !bg-panel"
                prop:value=query
                on:input=move |ev| set_query.set(event_target_value(&ev))
                on:keydown=move |ev| {
                    if ev.key() == "Enter" {
                        if let Some(Ok(list)) = results.get()
                            && let Some(first) = list.first() {
                                go_first(first.ticker.clone());
                            }
                    } else if ev.key() == "Escape" {
                        set_query.set(String::new());
                    }
                }
            />
            {move || {
                let list = match results.get() {
                    Some(Ok(list)) if !query.get().trim().is_empty() => list,
                    _ => return None,
                };
                let go = go.clone();
                Some(view! {
                    <ul class="absolute z-30 mt-1 w-full overflow-hidden rounded-lg border border-line bg-raised shadow-xl">
                        {if list.is_empty() {
                            view! { <li class="px-3 py-2 text-sm text-muted">"Tidak ada saham yang cocok"</li> }.into_any()
                        } else {
                            list.into_iter()
                                .map(|s| {
                                    let go = go.clone();
                                    let t = s.ticker.clone();
                                    view! {
                                        <li>
                                            <button
                                                class="flex w-full items-baseline gap-3 px-3 py-2 text-left text-sm hover:bg-panel"
                                                on:click=move |_| go(t.clone())
                                            >
                                                <span class="num w-14 font-medium text-brass">{s.ticker.clone()}</span>
                                                <span class="truncate text-paper">{s.name.clone()}</span>
                                            </button>
                                        </li>
                                    }
                                })
                                .collect_view()
                                .into_any()
                        }}
                    </ul>
                })
            }}
        </div>
    }
}
