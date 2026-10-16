use leptos::prelude::*;
use shared::{BacktestDto, BacktestStatsDto, SignalLevel};

use crate::api;
use crate::components::ui::{ErrorNote, NoData, PageHeader, SignalBadge, Skeleton};
use crate::format;

/// Selisih rata-rata (poin persen) terhadap pembanding yang dianggap berarti.
const EDGE: f64 = 0.5;

fn is_sell(level: Option<SignalLevel>) -> bool {
    matches!(level, Some(SignalLevel::Sell | SignalLevel::BestSell))
}

fn pct1(v: Option<f64>) -> String {
    match v {
        Some(v) if v.is_finite() => format!("{}%", format::number(v, 1)),
        _ => "–".into(),
    }
}

/// Kesimpulan satu sinyal dibanding membeli di sembarang hari.
fn verdict(level: SignalLevel, s: &BacktestStatsDto, base: &BacktestStatsDto) -> (String, &'static str) {
    let (Some(avg), Some(b)) = (s.avg, base.avg) else { return ("Belum cukup data".into(), "text-muted") };
    let edge = avg - b;
    let diff = format!("{} poin", format::number(edge.abs(), 2));
    if is_sell(Some(level)) {
        if edge <= -EDGE {
            (format!("Berguna: setelah sinyal ini harga rata-rata {diff} lebih lemah daripada hari biasa"), "text-up")
        } else {
            ("Tidak berguna: harga setelah sinyal ini tidak lebih lemah daripada hari biasa".into(), "text-down")
        }
    } else if edge >= EDGE {
        (format!("Berguna: hasil rata-rata {diff} lebih baik daripada membeli di sembarang hari"), "text-up")
    } else if edge <= -EDGE {
        (format!("Lebih buruk {diff} daripada membeli di sembarang hari"), "text-down")
    } else {
        ("Tidak lebih baik daripada membeli di sembarang hari".into(), "text-down")
    }
}

#[component]
pub fn Backtest() -> impl IntoView {
    let (liquid, set_liquid) = signal(true);
    let (horizon, set_horizon) = signal(2usize);
    let result = LocalResource::new(move || api::backtest(liquid.get()));

    let eyebrow = move || match result.get() {
        Some(Ok(d)) => format!("Uji historis · {} – {} · {} saham", format::date(&d.from), format::date(&d.to), d.tickers),
        _ => "Uji historis".into(),
    };
    let toggle = |active: bool| if active { "btn-primary" } else { "btn" };

    view! {
        <PageHeader eyebrow=Signal::derive(eyebrow) title="Uji Sinyal">
            <button class=move || toggle(liquid.get()) on:click=move |_| set_liquid.set(true)>"Saham likuid"</button>
            <button class=move || toggle(!liquid.get()) on:click=move |_| set_liquid.set(false)>"Semua saham"</button>
        </PageHeader>

        {move || match result.get() {
            None => view! {
                <div class="space-y-4">
                    <p class="text-sm text-muted">"Menghitung rating untuk setiap hari dalam 5 tahun terakhir…"</p>
                    <Skeleton class="h-64" />
                </div>
            }.into_any(),
            Some(Err(e)) if e.status == 503 => view! { <NoData /> }.into_any(),
            Some(Err(e)) => view! { <ErrorNote message=e.message /> }.into_any(),
            Some(Ok(d)) => view! { <Report d=d horizon=horizon set_horizon=set_horizon /> }.into_any(),
        }}
    }
}

#[component]
fn Report(d: BacktestDto, horizon: ReadSignal<usize>, set_horizon: WriteSignal<usize>) -> impl IntoView {
    let d = StoredValue::new(d);
    let base = move |k: usize| {
        d.with_value(|d| d.rows.iter().find(|r| r.level.is_none()).and_then(|r| r.stats.get(k).cloned()).unwrap_or_default())
    };
    let stats_of = move |level: SignalLevel, k: usize| {
        d.with_value(|d| d.rows.iter().find(|r| r.level == Some(level)).and_then(|r| r.stats.get(k).cloned()).unwrap_or_default())
    };
    let horizons = d.with_value(|d| d.horizons.clone());
    let cost = d.with_value(|d| d.cost_pct);

    view! {
        <div class="mb-5 flex flex-wrap items-center gap-2">
            <span class="eyebrow mr-1">"Dipegang"</span>
            {horizons.iter().enumerate().map(|(k, h)| view! {
                <button
                    class=move || if horizon.get() == k { "btn-primary" } else { "btn" }
                    on:click=move |_| set_horizon.set(k)
                >{format!("{h} hari")}</button>
            }).collect_view()}
        </div>

        <section class="mb-6 grid gap-4 md:grid-cols-2">
            {[SignalLevel::BestBuy, SignalLevel::BestSell].into_iter().map(|level| view! {
                <div class="panel p-5">
                    <div class="mb-3 flex items-center justify-between gap-3">
                        <SignalBadge level=level large=true />
                        <span class="num text-sm text-muted">{move || format!("{} sinyal", format::number(stats_of(level, horizon.get()).count as f64, 0))}</span>
                    </div>
                    {move || {
                        let k = horizon.get();
                        let (s, b) = (stats_of(level, k), base(k));
                        let (text, tone) = verdict(level, &s, &b);
                        let hit_label = if is_sell(Some(level)) { "Harga turun" } else { "Untung setelah biaya" };
                        view! {
                            <p class=format!("mb-4 font-medium {tone}")>{text}</p>
                            <dl class="grid grid-cols-2 gap-x-4 gap-y-2 text-sm">
                                <dt class="text-muted">"Rata-rata setelah sinyal"</dt>
                                <dd class=format!("num text-right {}", format::tone(s.avg))>{format::pct(s.avg)}</dd>
                                <dt class="text-muted">"Rata-rata hari biasa"</dt>
                                <dd class=format!("num text-right {}", format::tone(b.avg))>{format::pct(b.avg)}</dd>
                                <dt class="text-muted">{hit_label}</dt>
                                <dd class="num text-right">{pct1(s.hit_rate)}</dd>
                                <dt class="text-muted">"Terburuk"</dt>
                                <dd class="num text-right text-down">{format::pct(s.worst)}</dd>
                            </dl>
                        }
                    }}
                </div>
            }).collect_view()}
        </section>

        <section class="panel mb-6 overflow-hidden">
            <h2 class="border-b border-line px-4 py-3 font-display text-lg font-semibold">"Semua rating"</h2>
            <div class="overflow-x-auto">
                <table class="w-full whitespace-nowrap text-sm">
                    <thead class="border-b border-line text-left">
                        <tr>
                            {["Rating", "Sinyal", "Tepat", "Rata-rata", "Median", "Rata-rata naik", "Rata-rata turun", "Terburuk"]
                                .into_iter()
                                .enumerate()
                                .map(|(i, h)| view! {
                                    <th scope="col" class=if i == 0 { "px-4 py-2.5 font-normal" } else { "px-4 py-2.5 text-right font-normal" }>
                                        <span class="eyebrow">{h}</span>
                                    </th>
                                })
                                .collect_view()}
                        </tr>
                    </thead>
                    <tbody>
                        {move || {
                            let k = horizon.get();
                            d.with_value(|d| d.rows.clone()).into_iter().map(|r| {
                                let s = r.stats.get(k).cloned().unwrap_or_default();
                                let label = match r.level {
                                    Some(l) => view! { <SignalBadge level=l /> }.into_any(),
                                    None => view! { <span class="text-muted">"Pembanding: semua hari"</span> }.into_any(),
                                };
                                let row_class = if r.level.is_none() { "border-t-2 border-line" } else { "border-b border-line/50" };
                                view! {
                                    <tr class=row_class>
                                        <td class="px-4 py-2.5">{label}</td>
                                        <td class="num px-4 py-2.5 text-right">{format::number(s.count as f64, 0)}</td>
                                        <td class="num px-4 py-2.5 text-right">{pct1(s.hit_rate)}</td>
                                        <td class=format!("num px-4 py-2.5 text-right {}", format::tone(s.avg))>{format::pct(s.avg)}</td>
                                        <td class=format!("num px-4 py-2.5 text-right {}", format::tone(s.median))>{format::pct(s.median)}</td>
                                        <td class="num px-4 py-2.5 text-right text-up">{format::pct(s.avg_up)}</td>
                                        <td class="num px-4 py-2.5 text-right text-down">{format::pct(s.avg_down)}</td>
                                        <td class="num px-4 py-2.5 text-right text-down">{format::pct(s.worst)}</td>
                                    </tr>
                                }
                            }).collect_view()
                        }}
                    </tbody>
                </table>
            </div>
            <p class="border-t border-line px-4 py-3 text-xs leading-relaxed text-muted">
                {format!("Tepat: untuk rating beli dan pembanding, bagian yang untung setelah biaya {}%; untuk Sell dan Best Sell, bagian yang harganya turun.", format::number(cost, 1))}
            </p>
        </section>

        <ExitTable d=d />

        <YearlyTable d=d />

        <section class="panel p-5 text-sm leading-relaxed text-muted">
            <h2 class="mb-2 font-display text-lg font-semibold text-paper">"Cara membaca"</h2>
            <ul class="list-disc space-y-1.5 pl-5">
                <li>"Sinyal dihitung pada hari rating sebuah saham baru berubah menjadi level itu. Posisi dianggap dibeli di harga pembukaan hari berikutnya (sinyal baru diketahui setelah penutupan), lalu dijual di penutupan hari ke-5, 10, atau 20 setelah sinyal."</li>
                <li>"Pembanding adalah semua hari bursa pada saham yang sama. Sinyal baru berguna kalau hasilnya jelas lebih baik daripada pembanding; tingkat untung yang rendah wajar karena kebanyakan saham bergerak turun-naik tanpa arah."</li>
                <li>"Rata-rata bisa terangkat oleh sedikit saham yang melonjak; median menunjukkan hasil yang lebih umum."</li>
                <li>"200 hari pertama setiap saham dilewati supaya semua indikator sudah lengkap. Saham yang sudah tidak aktif ikut dihitung."</li>
                <li>"Hasil masa lalu tidak menjamin hasil ke depan."</li>
            </ul>
        </section>
    }
}

#[component]
fn YearlyTable(d: StoredValue<BacktestDto>) -> impl IntoView {
    let (years, h) = d.with_value(|d| {
        let mut years: Vec<i32> = d.yearly.iter().map(|y| y.year).collect();
        years.dedup();
        (years, d.yearly_horizon)
    });
    let get = move |year: i32, level: Option<SignalLevel>| {
        d.with_value(|d| d.yearly.iter().find(|y| y.year == year && y.level == level).map(|y| y.stats.clone()))
            .unwrap_or_default()
    };
    let cell = |s: &BacktestStatsDto| {
        view! {
            <td class=format!("num px-4 py-2.5 text-right {}", format::tone(s.avg))>{format::pct(s.avg)}</td>
            <td class="num px-4 py-2.5 text-right text-muted">{pct1(s.hit_rate)}</td>
        }
    };
    view! {
        <section class="panel mb-6 overflow-hidden">
            <h2 class="border-b border-line px-4 py-3 font-display text-lg font-semibold">
                {format!("Per tahun · dipegang {h} hari")}
            </h2>
            <div class="overflow-x-auto">
                <table class="w-full whitespace-nowrap text-sm">
                    <thead class="border-b border-line">
                        <tr>
                            <th scope="col" class="px-4 py-2.5 text-left font-normal"><span class="eyebrow">"Tahun"</span></th>
                            <th scope="col" colspan="2" class="px-4 py-2.5 text-right font-normal"><span class="eyebrow">"Hari biasa · tepat"</span></th>
                            <th scope="col" colspan="2" class="px-4 py-2.5 text-right font-normal"><span class="eyebrow">"Best Buy · tepat"</span></th>
                            <th scope="col" colspan="2" class="px-4 py-2.5 text-right font-normal"><span class="eyebrow">"Best Sell · tepat"</span></th>
                        </tr>
                    </thead>
                    <tbody>
                        {years.into_iter().map(|y| {
                            let (b, buy, sell) = (get(y, None), get(y, Some(SignalLevel::BestBuy)), get(y, Some(SignalLevel::BestSell)));
                            view! {
                                <tr class="border-b border-line/50 last:border-0">
                                    <td class="num px-4 py-2.5">{y}</td>
                                    {cell(&b)}
                                    {cell(&buy)}
                                    {cell(&sell)}
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
            <p class="border-t border-line px-4 py-3 text-xs text-muted">"Sinyal yang berguna seharusnya konsisten lebih baik daripada hari biasa di sebagian besar tahun, bukan hanya di satu tahun."</p>
        </section>
    }
}

#[component]
fn ExitTable(d: StoredValue<BacktestDto>) -> impl IntoView {
    let exits = d.with_value(|d| d.exits.clone());
    if exits.is_empty() {
        return ().into_any();
    }
    view! {
        <section class="panel mb-6 overflow-hidden">
            <h2 class="border-b border-line px-4 py-3 font-display text-lg font-semibold">"Cara keluar setelah Best Buy"</h2>
            <div class="overflow-x-auto">
                <table class="w-full whitespace-nowrap text-sm">
                    <thead class="border-b border-line text-left">
                        <tr>
                            {["Cara keluar", "Rata-rata", "Median", "Untung", "5% terburuk", "Terburuk", "Lama pegang"]
                                .into_iter()
                                .enumerate()
                                .map(|(i, h)| view! {
                                    <th scope="col" class=if i == 0 { "px-4 py-2.5 font-normal" } else { "px-4 py-2.5 text-right font-normal" }>
                                        <span class="eyebrow">{h}</span>
                                    </th>
                                })
                                .collect_view()}
                        </tr>
                    </thead>
                    <tbody>
                        {exits.into_iter().map(|e| {
                            let s = e.stats;
                            view! {
                                <tr class="border-b border-line/50 last:border-0">
                                    <td class="px-4 py-2.5">{e.name}</td>
                                    <td class=format!("num px-4 py-2.5 text-right {}", format::tone(s.avg))>{format::pct(s.avg)}</td>
                                    <td class=format!("num px-4 py-2.5 text-right {}", format::tone(s.median))>{format::pct(s.median)}</td>
                                    <td class="num px-4 py-2.5 text-right">{pct1(s.hit_rate)}</td>
                                    <td class="num px-4 py-2.5 text-right text-down">{format::pct(s.p5)}</td>
                                    <td class="num px-4 py-2.5 text-right text-down">{format::pct(s.worst)}</td>
                                    <td class="num px-4 py-2.5 text-right text-muted">
                                        {e.avg_days.map(|d| format!("{} hari", format::number(d, 1))).unwrap_or_else(|| "–".into())}
                                    </td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
            <p class="border-t border-line px-4 py-3 text-xs leading-relaxed text-muted">
                "Sinyal Best Buy yang sama, beda cara keluar. Trailing stop: jual di penutupan pertama yang berada di bawah harga tertinggi sejak beli − 3 × ATR 22, paling lama 60 hari. Biasanya lebih sering rugi kecil, tapi untung besar dibiarkan berjalan dan rugi besar terpotong."
            </p>
        </section>
    }
    .into_any()
}
