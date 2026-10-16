use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;
use shared::{CandlesDto, QualityDto, RiskDto, RiskLevel, SignalDto, StockStatsDto};

use crate::api;
use crate::components::chart::PriceChart;
use crate::components::ui::{Change, ErrorNote, QualityBadge, SignalBadge, Skeleton, on_data_change};
use crate::format;

const RANGES: &[(&str, &str)] = &[("3m", "3B"), ("6m", "6B"), ("1y", "1T"), ("3y", "3T"), ("5y", "5T")];

/// (id parameter API, label tombol, aktif secara default)
const OVERLAYS: &[(&str, &str, bool)] = &[
    ("sma:20", "SMA 20", true),
    ("sma:50", "SMA 50", true),
    ("sma:200", "SMA 200", false),
    ("ema:20", "EMA 20", false),
    ("bb:20:20", "Bollinger", false),
    ("supertrend:10:30", "Supertrend", false),
    ("ichimoku:9:26:52", "Ichimoku", false),
    ("psar:20:200", "PSAR", false),
];
const PANES: &[(&str, &str, bool)] = &[
    ("rsi:14", "RSI", true),
    ("macd:12:26:9", "MACD", true),
    ("stoch:14:3:3", "Stochastic", false),
    ("adx:14", "ADX", false),
    ("obv", "OBV", false),
    ("cmf:20", "CMF", false),
    ("mfi:14", "MFI", false),
    ("rvol:20", "Vol. relatif", false),
];

#[component]
pub fn Stock() -> impl IntoView {
    let params = use_params_map();
    let ticker = Memo::new(move |_| params.get().get("ticker").unwrap_or_default().to_uppercase());
    let range = RwSignal::new("1y".to_string());
    let enabled = RwSignal::new(
        OVERLAYS.iter().chain(PANES).filter(|(_, _, on)| *on).map(|(id, _, _)| id.to_string()).collect::<Vec<_>>(),
    );

    let detail = LocalResource::new(move || {
        let t = ticker.get();
        async move { api::stock(&t).await }
    });
    let candles = LocalResource::new(move || {
        let (t, r) = (ticker.get(), range.get());
        // Urutan tetap supaya panel bawah tidak berpindah-pindah.
        let on = enabled.get();
        let spec = OVERLAYS.iter().chain(PANES).filter(|(id, _, _)| on.iter().any(|x| x == id)).map(|(id, _, _)| *id).collect::<Vec<_>>().join(",");
        async move { api::candles(&t, &r, &spec).await }
    });
    // Harga, sinyal, risiko, dan grafik ikut diperbarui setiap ada data harga baru. Grafik
    // diperbarui di tempat, jadi zoom dan posisi geser tidak ter-reset.
    on_data_change(move || {
        detail.refetch();
        candles.refetch();
    });
    let chart_data = Signal::derive(move || candles.get().and_then(Result::ok));
    let chart_height = Signal::derive(move || {
        let panes = enabled.with(|e| PANES.iter().filter(|(id, _, _)| e.iter().any(|x| x == id)).count()) as u32;
        420 + panes * 115
    });

    let toggle = move |id: &'static str| {
        enabled.update(|e| {
            if let Some(i) = e.iter().position(|x| x == id) {
                e.remove(i);
            } else {
                e.push(id.to_string());
            }
        })
    };
    let chips = move |items: &'static [(&'static str, &'static str, bool)]| {
        items
            .iter()
            .map(|(id, label, _)| {
                let on = move || enabled.with(|e| e.iter().any(|x| x == id));
                view! {
                    <button
                        class=move || if on() { "chip chip-active" } else { "chip" }
                        aria-pressed=move || on().to_string()
                        on:click=move |_| toggle(id)
                    >
                        {*label}
                    </button>
                }
            })
            .collect_view()
    };

    view! {
        {move || match detail.get() {
            None => view! { <Skeleton class="h-20 mb-6" /> }.into_any(),
            Some(Err(e)) => view! { <ErrorNote message=e.message /> }.into_any(),
            Some(Ok(d)) => {
                let stats = d.stats.clone();
                view! {
                    <header class="mb-6 flex flex-wrap items-end justify-between gap-4">
                        <div>
                            <p class="eyebrow">
                                {d.stock.sector.clone().unwrap_or_else(|| "Saham".into())}
                                {d.stock.board.clone().map(|b| format!(" · Papan {b}"))}
                            </p>
                            <h1 class="font-display text-4xl font-bold tracking-tight">
                                {d.stock.ticker.clone()}
                                <span class="ml-3 align-middle font-sans text-base font-normal text-muted">{d.stock.name.clone()}</span>
                            </h1>
                        </div>
                        <div class="flex items-end gap-6">
                            {stats.clone().map(|s| view! {
                                <div class="text-right">
                                    <p class="num font-display text-4xl font-bold">{format::price(s.close)}</p>
                                    <p class="text-sm"><Change value=s.change_1d />
                                        <span class="ml-2 text-xs text-muted">{format::date(&s.last_date)}</span>
                                    </p>
                                </div>
                            })}
                            <AddToWatchlist ticker=d.stock.ticker.clone() />
                        </div>
                    </header>
                    {stats.map(|s| view! { <OhlcStrip s=s /> })}
                }
                    .into_any()
            }
        }}

        <div class="grid gap-6 xl:grid-cols-[minmax(0,1fr)_17rem]">
            <section class="panel min-w-0 p-3 sm:p-4">
                <div class="mb-3 flex flex-wrap items-center justify-between gap-3">
                    <div class="flex rounded-md border border-line p-0.5 text-xs" role="group" aria-label="Rentang waktu">
                        {RANGES
                            .iter()
                            .map(|(id, label)| {
                                view! {
                                    <button
                                        class=move || if range.get() == *id { "rounded bg-raised px-2.5 py-1 text-paper" } else { "rounded px-2.5 py-1 text-muted hover:text-paper" }
                                        aria-pressed=move || (range.get() == *id).to_string()
                                        on:click=move |_| range.set(id.to_string())
                                    >
                                        {*label}
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>
                    <div class="flex flex-wrap gap-1.5">{chips(OVERLAYS)}</div>
                </div>
                <div class="mb-3 flex flex-wrap items-center gap-1.5">
                    <span class="eyebrow mr-1">"Panel"</span>
                    {chips(PANES)}
                </div>
                {move || match candles.get() {
                    Some(Err(e)) => view! { <ErrorNote message=e.message /> }.into_any(),
                    Some(Ok(c)) if c.candles.is_empty() => view! {
                        <p class="px-4 py-16 text-center text-sm text-muted">"Belum ada data harga untuk saham ini. Jalankan update di halaman Data."</p>
                    }.into_any(),
                    None => view! { <Skeleton class="h-[420px]" /> }.into_any(),
                    _ => ().into_any(),
                }}
                <Show when=move || chart_data.with(|d| d.as_ref().is_some_and(|c: &CandlesDto| !c.candles.is_empty()))>
                    <PriceChart data=chart_data height=chart_height />
                </Show>
            </section>

            <aside class="space-y-4">
                {move || match detail.get() {
                    Some(Ok(d)) => {
                        let close = d.stats.as_ref().map(|s| s.close);
                        view! {
                            {d.signal.map(|s| view! { <SignalPanel s=s /> })}
                            {d.quality.map(|q| view! { <FundamentalPanel q=q /> })}
                            {d.win_rate.map(|w| view! { <WinRatePanel w=w /> })}
                            {d.risk.map(|r| view! { <RiskPanel r=r close=close /> })}
                            {d.stats.map(|s| view! { <Stats s=s /> })}
                        }
                            .into_any()
                    }
                    _ => view! { <Skeleton class="h-72" /> }.into_any(),
                }}
            </aside>
        </div>
    }
}

/// Riwayat sinyal beli saham ini.
#[component]
fn WinRatePanel(w: shared::WinRateDto) -> impl IntoView {
    let enough = w.signals >= shared::WinRateDto::MIN_SIGNALS;
    view! {
        <section class="panel p-4" aria-label="Win rate">
            <div class="mb-2 flex items-baseline justify-between">
                <h2 class="font-display text-lg font-semibold">"Win rate sinyal"</h2>
                <p class=if enough { "num font-display text-2xl font-bold" } else { "num font-display text-2xl font-bold text-muted" }>
                    {format!("{:.0}%", w.rate())}
                </p>
            </div>
            <p class="text-sm">
                {format!("{} dari {} sinyal beli (Buy/Best Buy) untung setelah 20 hari bursa; rata-rata ", w.wins, w.signals)}
                <Change value=Some(w.avg_return) />
                " per sinyal."
            </p>
            {(!enough).then(|| view! {
                <p class="mt-2 text-xs text-brass">{format!("Kurang dari {} sinyal: angka ini belum bermakna.", shared::WinRateDto::MIN_SIGNALS)}</p>
            })}
            <p class="mt-3 text-[11px] leading-relaxed text-muted">
                "Dari seluruh riwayat (±5 tahun), masuk di open hari berikutnya, setelah biaya 0,4%. Uji 2022–2024 → 2025–2026 menunjukkan win rate masa lalu tidak memprediksi hasil berikutnya (korelasi ±0,04)."
            </p>
        </section>
    }
}

/// Kesehatan fundamental: laba, ROE, PER dibanding sektor, PBV, dividen.
#[component]
fn FundamentalPanel(q: QualityDto) -> impl IntoView {
    let row = |label: &'static str, value: String| {
        view! {
            <div class="flex items-baseline justify-between border-b border-line/60 py-1.5 last:border-0">
                <dt class="text-sm text-muted">{label}</dt>
                <dd class="num text-sm">{value}</dd>
            </div>
        }
    };
    let ratio = |v: Option<f64>, suffix: &str| v.map(|v| format!("{}{suffix}", format::number(v, 1))).unwrap_or_else(|| "–".into());
    let pe = match (q.pe, q.sector_pe) {
        (Some(pe), Some(s)) => format!("{} (sektor {})", format::number(pe, 1), format::number(s, 1)),
        (pe, _) => ratio(pe, ""),
    };
    view! {
        <section class="panel p-4" aria-label="Fundamental">
            <div class="mb-2 flex items-center justify-between">
                <h2 class="font-display text-lg font-semibold">"Fundamental"</h2>
                <QualityBadge level=q.level />
            </div>
            <dl>
                {row("PER", pe)}
                {row("PBV", ratio(q.pbv, "×"))}
                {row("ROE", ratio(q.roe, "%"))}
                {row("Dividen", ratio(q.dividend_yield, "%"))}
                {row("Kapitalisasi", q.market_cap.map(format::rupiah).unwrap_or_else(|| "–".into()))}
            </dl>
            <ul class="mt-3 space-y-1 text-sm">
                {q.reasons.into_iter().map(|r| view! { <li class="flex gap-2"><span class="text-brass">"›"</span>{r}</li> }).collect_view()}
            </ul>
            <p class="mt-3 text-[11px] leading-relaxed text-muted">
                "Dari data laba dan nilai buku terkini di Yahoo Finance, diperbarui setiap update harian. Belum memeriksa utang, arus kas, atau kualitas laba, dan tidak bisa diuji dengan backtest karena riwayatnya tidak tersedia."
            </p>
        </section>
    }
}

/// Open, high, low, close bar terakhir, diwarnai terhadap penutupan sebelumnya.
#[component]
fn OhlcStrip(s: StockStatsDto) -> impl IntoView {
    let prev = s.prev_close;
    let tone = move |v: Option<f64>| match (v, prev) {
        (Some(v), Some(p)) if v > p => "text-up",
        (Some(v), Some(p)) if v < p => "text-down",
        _ => "text-paper",
    };
    let price = |v: Option<f64>| v.map(format::price).unwrap_or_else(|| "–".into());
    let cell = |label: &'static str, value: String, class: &'static str| {
        view! {
            <div class="min-w-0 px-3 py-2">
                <dt class="eyebrow">{label}</dt>
                <dd class=format!("num mt-0.5 truncate text-base font-medium {class}")>{value}</dd>
            </div>
        }
    };
    let value = s.volume.map(|v| format::rupiah(v * s.close)).unwrap_or_else(|| "–".into());
    view! {
        <dl class="panel mb-6 grid grid-cols-2 divide-line sm:grid-cols-4 lg:grid-cols-7" aria-label="Open, high, low, close">
            {cell("Open", price(s.open), tone(s.open))}
            {cell("High", price(s.high), tone(s.high))}
            {cell("Low", price(s.low), tone(s.low))}
            {cell("Close", price(Some(s.close)), tone(Some(s.close)))}
            {cell("Sebelumnya", price(prev), "text-muted")}
            {cell("Volume", s.volume.map(format::compact).unwrap_or_else(|| "–".into()), "text-paper")}
            {cell("Nilai", value, "text-paper")}
        </dl>
    }
}

#[component]
fn Stats(s: StockStatsDto) -> impl IntoView {
    let row = |label: &'static str, value: AnyView| {
        view! {
            <div class="flex items-baseline justify-between border-b border-line/60 py-2 last:border-0">
                <dt class="text-sm text-muted">{label}</dt>
                <dd class="num text-sm">{value}</dd>
            </div>
        }
    };
    let range_pos = match (s.low_52w, s.high_52w) {
        (Some(lo), Some(hi)) if hi > lo => Some(((s.close - lo) / (hi - lo) * 100.0).clamp(0.0, 100.0)),
        _ => None,
    };
    view! {
        <section class="panel p-4">
            <h2 class="mb-2 font-display text-lg font-semibold">"Kinerja"</h2>
            <dl>
                {row("1 hari", view! { <Change value=s.change_1d /> }.into_any())}
                {row("1 bulan", view! { <Change value=s.change_1m /> }.into_any())}
                {row("3 bulan", view! { <Change value=s.change_3m /> }.into_any())}
                {row("1 tahun", view! { <Change value=s.change_1y /> }.into_any())}
                {row("Nilai transaksi 20H", view! { <span>{s.avg_value_20.map(format::rupiah).unwrap_or_else(|| "–".into())}</span> }.into_any())}
            </dl>
            {range_pos.map(|pos| view! {
                <div class="mt-4">
                    <p class="eyebrow mb-2">"Rentang 52 minggu"</p>
                    <div class="relative h-1.5 rounded-full bg-line">
                        <div class="absolute -top-1 h-3.5 w-1 rounded bg-brass" style=format!("left: calc({pos:.1}% - 2px)")></div>
                    </div>
                    <div class="mt-1.5 flex justify-between font-mono text-xs text-muted">
                        <span>{s.low_52w.map(format::price)}</span>
                        <span>{s.high_52w.map(format::price)}</span>
                    </div>
                </div>
            })}
            <p class="mt-4 text-[11px] leading-relaxed text-muted">
                "Harga sudah disesuaikan dengan dividen dan stock split, jadi bisa sedikit berbeda dari harga yang tercatat di bursa."
            </p>
        </section>
    }
}

/// Nilai pilihan dropdown untuk membuat watchlist baru.
const NEW_WATCHLIST: &str = "new";

#[component]
fn AddToWatchlist(ticker: String) -> impl IntoView {
    let lists = LocalResource::new(api::watchlists);
    let (message, set_message) = signal(None::<(bool, String)>);
    let ticker = StoredValue::new(ticker);
    view! {
        <div class="flex flex-col items-end gap-1">
            <select
                aria-label="Tambah ke watchlist"
                prop:value=""
                on:change=move |ev| {
                    let value = event_target_value(&ev);
                    // Kembalikan ke "Tambah ke watchlist" supaya bisa dipakai lagi.
                    event_target::<web_sys::HtmlSelectElement>(&ev).set_value("");
                    let t = ticker.get_value();
                    if value == NEW_WATCHLIST {
                        let Some(name) = window()
                            .prompt_with_message_and_default("Nama watchlist baru", "Favorit")
                            .ok()
                            .flatten()
                            .map(|n| n.trim().to_string())
                            .filter(|n| !n.is_empty())
                        else {
                            return;
                        };
                        spawn_local(async move {
                            let result = match api::create_watchlist(&name).await {
                                Ok(w) => api::add_to_watchlist(w.id, &t).await,
                                Err(e) => Err(e),
                            };
                            match result {
                                Ok(()) => set_message.set(Some((true, format!("Watchlist {name} dibuat, {t} ditambahkan")))),
                                Err(e) => set_message.set(Some((false, e.message))),
                            }
                            lists.refetch();
                        });
                        return;
                    }
                    let Ok(id) = value.parse::<i64>() else { return };
                    let name = match lists.get_untracked() {
                        Some(Ok(all)) => all.into_iter().find(|w| w.id == id).map(|w| w.name).unwrap_or_default(),
                        _ => String::new(),
                    };
                    spawn_local(async move {
                        match api::add_to_watchlist(id, &t).await {
                            Ok(()) => set_message.set(Some((true, format!("Ditambahkan ke {name}")))),
                            Err(e) => set_message.set(Some((false, e.message))),
                        }
                        lists.refetch();
                    });
                }
            >
                <option value="">"☆ Tambah ke watchlist"</option>
                {move || match lists.get() {
                    Some(Ok(all)) => all
                        .into_iter()
                        .map(|w| {
                            let has = w.items.iter().any(|i| i.ticker == ticker.get_value());
                            view! { <option value=w.id.to_string() disabled=has>{w.name}{has.then_some(" ✓")}</option> }
                        })
                        .collect_view()
                        .into_any(),
                    _ => ().into_any(),
                }}
                <option value=NEW_WATCHLIST>"+ Watchlist baru…"</option>
            </select>
            {move || message.get().map(|(ok, m)| view! { <span class=if ok { "text-xs text-up" } else { "text-xs text-down" }>{m}</span> })}
        </div>
    }
}

/// Kelas warna dan label untuk setiap level risiko.
pub fn risk_style(level: RiskLevel) -> (&'static str, &'static str) {
    match level {
        RiskLevel::High => ("border-down/50 bg-down/10 text-down", "Tinggi"),
        RiskLevel::Medium => ("border-brass/50 bg-brass/10 text-brass", "Sedang"),
        RiskLevel::Info => ("border-line bg-raised text-muted", "Info"),
    }
}

#[component]
fn RiskPanel(r: RiskDto, close: Option<f64>) -> impl IntoView {
    let stop = match (r.stop_loss, close) {
        (Some(stop), Some(c)) if c > 0.0 => Some((stop, (stop / c - 1.0) * 100.0)),
        _ => None,
    };
    let trail = match (r.trailing_stop, close) {
        (Some(t), Some(c)) if c > 0.0 && t > 0.0 => Some((t, (t / c - 1.0) * 100.0, c < t)),
        _ => None,
    };
    view! {
        <section class="panel p-4" aria-label="Peringatan risiko">
            <h2 class="mb-3 font-display text-lg font-semibold">"Peringatan risiko"</h2>
            {if r.flags.is_empty() {
                view! {
                    <p class="rounded-lg border border-up/40 bg-up/10 px-3 py-2 text-sm text-up">
                        "Tidak ada tanda bahaya dari pola harga dan volume."
                    </p>
                }
                    .into_any()
            } else {
                view! {
                    <ul class="space-y-2">
                        {r.flags
                            .into_iter()
                            .map(|f| {
                                let (tone, level) = risk_style(f.level);
                                view! {
                                    <li class=format!("rounded-lg border px-3 py-2 {tone}")>
                                        <p class="flex items-baseline justify-between gap-2 text-sm font-medium">
                                            {f.title}
                                            <span class="font-mono text-[10px] uppercase tracking-wider">{level}</span>
                                        </p>
                                        <p class="mt-0.5 text-xs leading-relaxed text-paper/80">{f.detail}</p>
                                    </li>
                                }
                            })
                            .collect_view()}
                    </ul>
                }
                    .into_any()
            }}
            {stop.map(|(price, pct)| view! {
                <div class="mt-4 border-t border-line pt-3">
                    <p class="eyebrow">"Saran stop loss (2 × ATR 14)"</p>
                    <p class="mt-1 flex items-baseline gap-2">
                        <span class="num font-display text-2xl font-bold">{format::price(price)}</span>
                        <span class="num text-sm text-down">{format::pct(Some(pct))}</span>
                    </p>
                    <p class="mt-1 text-[11px] leading-relaxed text-muted">
                        "Batas keluar kalau harga bergerak melawan posisi. Sesuaikan dengan modal dan rencana trading-mu."
                    </p>
                </div>
            })}
            {trail.map(|(price, pct, below)| view! {
                <div class="mt-4 border-t border-line pt-3">
                    <p class="eyebrow">"Trailing stop (high 22 hari − 3 × ATR 22)"</p>
                    <p class="mt-1 flex items-baseline gap-2">
                        <span class="num font-display text-2xl font-bold">{format::price(price)}</span>
                        <span class="num text-sm text-down">
                            {if below { "harga sudah di bawah".to_string() } else { format::pct(Some(pct)) }}
                        </span>
                    </p>
                    <p class="mt-1 text-[11px] leading-relaxed text-muted">
                        "Untuk posisi yang sudah berjalan: jual kalau harga penutupan di bawah angka ini. Batasnya naik mengikuti harga tertinggi, jadi untung dibiarkan berjalan dan rugi besar terpotong. Lihat hasil ujinya di halaman Uji Sinyal."
                    </p>
                </div>
            })}
            <p class="mt-3 text-[11px] leading-relaxed text-muted">
                "Dihitung dari harga dan volume saja. Ini tanda yang perlu diwaspadai, bukan bukti adanya bandar atau manipulasi."
            </p>
        </section>
    }
}

#[component]
fn SignalPanel(s: SignalDto) -> impl IntoView {
    // Posisi skor −100..100 pada bar 0..100%.
    let pos = f64::from(s.score.clamp(-100, 100) + 100) / 2.0;
    let tone = if s.score > 0 { "text-up" } else if s.score < 0 { "text-down" } else { "text-muted" };
    view! {
        <section class="panel p-4" aria-label="Sinyal teknikal">
            <div class="flex items-center justify-between gap-3">
                <h2 class="font-display text-lg font-semibold">"Sinyal"</h2>
                <SignalBadge level=s.level large=true />
            </div>
            <div class="mt-4">
                <div class="flex items-baseline justify-between">
                    <span class="eyebrow">"Skor gabungan"</span>
                    <span class=format!("num font-display text-2xl font-bold {tone}")>{format!("{:+}", s.score)}</span>
                </div>
                <div class="relative mt-2 h-1.5 rounded-full bg-gradient-to-r from-down via-line to-up" aria-hidden="true">
                    <div class="absolute -top-1 h-3.5 w-1 rounded bg-paper" style=format!("left: calc({pos:.1}% - 2px)")></div>
                </div>
                <div class="mt-1 flex justify-between font-mono text-[10px] text-muted">
                    <span>"−100 jual"</span>
                    <span>"+100 beli"</span>
                </div>
            </div>
            <ul class="mt-4 space-y-1.5">
                {s.reasons
                    .into_iter()
                    .map(|r| view! { <li class="flex gap-2 text-sm leading-snug"><span class="text-brass" aria-hidden="true">"›"</span>{r}</li> })
                    .collect_view()}
            </ul>
            <p class="mt-3 text-[11px] leading-relaxed text-muted">
                "Gabungan 12 indikator tren, momentum, dan aliran dana. Best Buy dan Best Sell hanya muncul saat pemicunya baru terjadi (≤ 3 hari). Bukan jaminan arah harga."
            </p>
        </section>
    }
}
