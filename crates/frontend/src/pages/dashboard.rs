use leptos::prelude::*;
use shared::{MarketSummaryDto, MoverDto, ScanRequest, ScanResultDto, SignalLevel, Universe};

use crate::api;
use crate::components::ui::{ErrorNote, NoData, PageHeader, SignalBadge, Skeleton, live_label, on_data_change};
use crate::format;

#[component]
pub fn Dashboard() -> impl IntoView {
    let presets = LocalResource::new(api::presets);
    let summary = LocalResource::new(api::market_summary);
    // Semua panel dimuat ulang otomatis setiap ada data harga baru.
    let refresh = RwSignal::new(0u32);
    let status = on_data_change(move || {
        summary.refetch();
        refresh.update(|n| *n += 1);
    });

    let as_of = move || {
        let live = match status.get() {
            Some(Ok(s)) => live_label(s.live.as_ref()),
            _ => None,
        };
        match (summary.get(), live) {
            (Some(Ok(s)), Some(live)) => format!("{} · {live}", format::date(&s.as_of)),
            (Some(Ok(s)), None) => format!("Penutupan {}", format::date(&s.as_of)),
            _ => "Ringkasan pasar".into(),
        }
    };

    view! {
        <PageHeader eyebrow=Signal::derive(as_of) title="Pasar hari ini">
            <a href="/scanner" class="btn-primary">"Buka scanner"</a>
        </PageHeader>

        {move || match summary.get() {
            None => view! { <Skeleton class="h-40" /> }.into_any(),
            Some(Err(e)) if e.status == 503 => view! { <NoData /> }.into_any(),
            Some(Err(e)) => view! { <ErrorNote message=e.message /> }.into_any(),
            Some(Ok(s)) => view! { <Summary s=s /> }.into_any(),
        }}

        <SignalBoards refresh=refresh />

        <section class="mt-10">
            <div class="mb-4 flex items-baseline justify-between">
                <h2 class="font-display text-xl font-semibold">"Strategi siap pakai"</h2>
                <p class="text-xs text-muted">"Klik untuk langsung menjalankan scan"</p>
            </div>
            <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
                {move || match presets.get() {
                    Some(Ok(list)) => list
                        .into_iter()
                        .map(|p| {
                            view! {
                                <a
                                    href=format!("/scanner?preset={}", p.id)
                                    class="panel group block p-4 transition-colors hover:border-brass"
                                >
                                    <p class="font-medium group-hover:text-brass">{p.name}</p>
                                    <p class="mt-1 text-sm leading-snug text-muted">{p.description}</p>
                                </a>
                            }
                        })
                        .collect_view()
                        .into_any(),
                    Some(Err(e)) => view! { <ErrorNote message=e.message /> }.into_any(),
                    None => (0..4).map(|_| view! { <Skeleton class="h-24" /> }).collect_view().into_any(),
                }}
            </div>
        </section>

        <section class="mt-10">
            <h2 class="mb-3 font-display text-xl font-semibold">"Status data"</h2>
            {move || match status.get() {
                Some(Ok(st)) => match st.history.first() {
                    Some(run) => view! {
                        <p class="text-sm text-muted">
                            "Update terakhir "
                            <span class="text-paper">{format::datetime(run.finished_at.as_deref().unwrap_or(&run.started_at))}</span>
                            " · " {run.ok_count} " saham berhasil"
                            {(!run.failed_tickers.is_empty())
                                .then(|| format!(", {} gagal", run.failed_tickers.len()))}
                            " · "
                            <a href="/data" class="text-brass hover:underline">"Kelola data"</a>
                        </p>
                    }
                        .into_any(),
                    None => view! { <p class="text-sm text-muted">"Belum pernah ada update data."</p> }.into_any(),
                },
                _ => ().into_any(),
            }}
        </section>
    }
}

#[component]
fn Summary(s: MarketSummaryDto) -> impl IntoView {
    let total = (s.advancers + s.decliners + s.unchanged).max(1) as f64;
    let share = |n: usize| format!("width: {:.2}%", n as f64 / total * 100.0);
    let market = s.market_strong.map(|strong| {
        let (label, tone, note) = if strong {
            ("Pasar kuat", "text-up", "IHSG di atas SMA 50. Best Buy aktif.")
        } else {
            ("Pasar melemah", "text-down", "IHSG di bawah SMA 50. Best Buy ditahan karena secara historis hasilnya jauh lebih buruk saat pasar lemah.")
        };
        view! {
            <p class="mb-4 text-sm">
                <span class=format!("font-medium {tone}")>{label}</span>
                <span class="text-muted">" · " {note}</span>
            </p>
        }
    });
    view! {
        <section class="panel p-5 sm:p-6">
            {market}
            <div class="flex flex-wrap items-baseline gap-x-10 gap-y-3">
                <Breadth label="Naik" count=s.advancers tone="text-up" />
                <Breadth label="Turun" count=s.decliners tone="text-down" />
                <Breadth label="Stagnan" count=s.unchanged tone="text-muted" />
            </div>
            <div class="mt-5 flex h-2 overflow-hidden rounded-full bg-line" role="img"
                aria-label=format!("{} naik, {} turun, {} stagnan", s.advancers, s.decliners, s.unchanged)>
                <div class="bg-up" style=share(s.advancers)></div>
                <div class="bg-muted/50" style=share(s.unchanged)></div>
                <div class="bg-down" style=share(s.decliners)></div>
            </div>
        </section>

        <div class="mt-6 grid gap-6 lg:grid-cols-3">
            <Movers title="Kenaikan terbesar" note="likuid ≥ Rp1 M/hari" rows=s.top_gainers metric=Metric::Change />
            <Movers title="Penurunan terbesar" note="likuid ≥ Rp1 M/hari" rows=s.top_losers metric=Metric::Change />
            <Movers title="Nilai transaksi" note="hari terakhir" rows=s.top_value metric=Metric::Value />
        </div>
    }
}

#[component]
fn Breadth(label: &'static str, count: usize, tone: &'static str) -> impl IntoView {
    view! {
        <div>
            <p class="eyebrow">{label}</p>
            <p class=format!("num font-display text-4xl font-bold {tone}")>{count}</p>
        </div>
    }
}

#[derive(Clone, Copy)]
enum Metric {
    Change,
    Value,
}

#[component]
fn Movers(title: &'static str, note: &'static str, rows: Vec<MoverDto>, metric: Metric) -> impl IntoView {
    view! {
        <section class="panel overflow-hidden">
            <div class="flex items-baseline justify-between border-b border-line px-4 py-3">
                <h2 class="font-medium">{title}</h2>
                <span class="text-[11px] text-muted">{note}</span>
            </div>
            {if rows.is_empty() {
                view! { <p class="px-4 py-6 text-sm text-muted">"Tidak ada saham yang memenuhi"</p> }.into_any()
            } else {
                view! {
                    <ul>
                        {rows
                            .into_iter()
                            .map(|m| {
                                let right = match metric {
                                    Metric::Change => view! { <span class=format!("num text-sm {}", format::tone(Some(m.change_pct)))>{format::pct(Some(m.change_pct))}</span> }.into_any(),
                                    Metric::Value => view! { <span class="num text-sm text-paper">{format::rupiah(m.value)}</span> }.into_any(),
                                };
                                view! {
                                    <li class="border-b border-line/60 last:border-0">
                                        <a href=format!("/stock/{}", m.ticker) class="flex items-center gap-3 px-4 py-2.5 hover:bg-raised">
                                            <span class="num w-14 font-medium text-brass">{m.ticker.clone()}</span>
                                            <span class="min-w-0 flex-1 truncate text-sm text-muted" title=m.name.clone()>{format::short_name(&m.name)}</span>
                                            <span class="num w-16 text-right text-sm">{format::price(m.close)}</span>
                                            <span class="w-20 text-right">{right}</span>
                                        </a>
                                    </li>
                                }
                            })
                            .collect_view()}
                    </ul>
                }
                    .into_any()
            }}
        </section>
    }
}

fn signal_request(level: SignalLevel) -> ScanRequest {
    let mut req = ScanRequest::new(vec![]);
    req.universe = Universe { min_avg_value_20d: Some(1e9), signals: vec![level], ..Universe::default() };
    req.limit = 8;
    req
}

/// Best Buy dan Best Sell hari ini untuk saham likuid.
#[component]
fn SignalBoards(refresh: RwSignal<u32>) -> impl IntoView {
    let buys = LocalResource::new(move || {
        refresh.track();
        async { api::scan(&signal_request(SignalLevel::BestBuy)).await }
    });
    let sells = LocalResource::new(move || {
        refresh.track();
        async { api::scan(&signal_request(SignalLevel::BestSell)).await }
    });
    view! {
        <section class="mt-6 grid gap-6 lg:grid-cols-2">
            <SignalBoard
                title="Best Buy hari ini"
                note="Pemicu naik baru muncul, tren dan IHSG mendukung"
                preset="best-buy"
                level=SignalLevel::BestBuy
                data=Signal::derive(move || buys.get())
            />
            <SignalBoard
                title="Best Sell hari ini"
                note="Pemicu turun baru muncul dekat puncak"
                preset="best-sell"
                level=SignalLevel::BestSell
                data=Signal::derive(move || sells.get())
            />
        </section>
    }
}

#[component]
fn SignalBoard(
    title: &'static str,
    note: &'static str,
    preset: &'static str,
    level: SignalLevel,
    data: Signal<Option<api::Res<ScanResultDto>>>,
) -> impl IntoView {
    view! {
        <section class="panel overflow-hidden">
            <div class="flex items-center justify-between gap-3 border-b border-line px-4 py-3">
                <div class="flex items-center gap-2">
                    <SignalBadge level=level />
                    <h2 class="font-medium">{title}</h2>
                </div>
                <a href=format!("/scanner?preset={preset}") class="text-xs text-brass hover:underline">"Lihat semua"</a>
            </div>
            {move || match data.get() {
                None => view! { <div class="p-4"><Skeleton class="h-32" /></div> }.into_any(),
                Some(Err(e)) if e.status == 503 => ().into_any(),
                Some(Err(e)) => view! { <div class="p-4"><ErrorNote message=e.message /></div> }.into_any(),
                Some(Ok(r)) if r.rows.is_empty() => view! {
                    <p class="px-4 py-8 text-center text-sm text-muted">{format!("Belum ada saham likuid dengan sinyal ini. {note}.")}</p>
                }
                    .into_any(),
                Some(Ok(r)) => view! {
                    <ul>
                        {r.rows
                            .into_iter()
                            .map(|row| {
                                let reason = row.signal.as_ref().and_then(|s| s.reasons.first().cloned()).unwrap_or_default();
                                let score = row.signal.as_ref().map(|s| s.score).unwrap_or_default();
                                view! {
                                    <li class="border-b border-line/60 last:border-0">
                                        <a href=format!("/stock/{}", row.ticker) class="flex items-center gap-3 px-4 py-2.5 hover:bg-raised">
                                            <span class="num w-14 font-medium text-brass">{row.ticker.clone()}</span>
                                            <span class="min-w-0 flex-1">
                                                <span class="block truncate text-sm text-paper">{format::short_name(&row.name)}</span>
                                                <span class="block truncate text-xs text-muted">{reason}</span>
                                            </span>
                                            <span class="num w-16 text-right text-sm">{format::price(row.close)}</span>
                                            <span class=format!("num w-10 text-right text-xs {}", format::tone(Some(f64::from(score))))>{format!("{score:+}")}</span>
                                        </a>
                                    </li>
                                }
                            })
                            .collect_view()}
                    </ul>
                }
                    .into_any(),
            }}
        </section>
    }
}
