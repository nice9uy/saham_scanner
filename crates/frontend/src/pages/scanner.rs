use std::collections::BTreeSet;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;
use leptos_router::hooks::{use_navigate, use_query_map};
use shared::{
    Logic, PresetDto, QualityLevel, SaveScanDto, SavedScanDto, ScanRequest, ScanResultDto, ScanRowDto, SignalLevel, Universe,
    WinRateDto,
};

use crate::api::{self, ApiFailure};
use crate::components::ui::{
    Change, ErrorNote, NoData, PageHeader, QualityBadge, SignalBadge, Skeleton, on_data_change, quality_class, signal_class,
};
use crate::format;
use crate::scanner::builder::{ConditionRow, Row, default_condition};

const DEFAULT_LIMIT: u32 = 200;

#[derive(Clone, Copy)]
struct ScanState {
    rows: RwSignal<Vec<Row>>,
    next_id: RwSignal<u32>,
    logic: RwSignal<Logic>,
    universe: RwSignal<Universe>,
    preset: RwSignal<Option<String>>,
    saved: RwSignal<Option<SavedScanDto>>,
}

impl ScanState {
    fn new() -> Self {
        Self {
            rows: RwSignal::new(vec![Row { id: 0, cond: default_condition() }]),
            next_id: RwSignal::new(1),
            logic: RwSignal::new(Logic::All),
            universe: RwSignal::new(Universe { min_avg_value_20d: Some(1e9), ..Universe::default() }),
            preset: RwSignal::new(None),
            saved: RwSignal::new(None),
        }
    }

    fn request(&self) -> ScanRequest {
        let mut req = ScanRequest::new(self.rows.get_untracked().into_iter().map(|r| r.cond).collect());
        req.logic = self.logic.get_untracked();
        req.universe = self.universe.get_untracked();
        req.limit = DEFAULT_LIMIT;
        req
    }

    fn load(&self, req: &ScanRequest) {
        let start = self.next_id.get_untracked();
        let rows: Vec<Row> = req
            .conditions
            .iter()
            .enumerate()
            .map(|(i, c)| Row { id: start + i as u32, cond: c.clone() })
            .collect();
        self.next_id.set(start + rows.len() as u32);
        self.rows.set(rows);
        self.logic.set(req.logic);
        self.universe.set(req.universe.clone());
    }

    fn add(&self) {
        let id = self.next_id.get_untracked();
        self.next_id.set(id + 1);
        self.rows.update(|v| v.push(Row { id, cond: default_condition() }));
    }
}

#[component]
pub fn Scanner() -> impl IntoView {
    let state = ScanState::new();
    let result = RwSignal::new(None::<Result<ScanResultDto, ApiFailure>>);
    let running = RwSignal::new(false);
    let presets = LocalResource::new(api::presets);
    let navigate = use_navigate();

    // Urutan tabel hasil disimpan di sini supaya tetap saat hasil diperbarui otomatis.
    let sort = RwSignal::new(None::<(SortKey, bool)>);
    // Scan terakhir yang dijalankan; diulang otomatis setiap ada data harga baru.
    let last_req = RwSignal::new(None::<ScanRequest>);
    on_data_change(move || {
        if let Some(req) = last_req.get_untracked() {
            spawn_local(async move {
                let r = api::scan(&req).await;
                // Pertahankan hasil lama kalau server sedang bermasalah sesaat.
                if r.is_ok() {
                    result.set(Some(r));
                }
            });
        }
    });

    let run = move || {
        let req = state.request();
        last_req.set(Some(req.clone()));
        sort.set(None);
        running.set(true);
        let encoded = String::from(js_sys::encode_uri_component(&serde_json::to_string(&req).unwrap_or_default()));
        navigate(&format!("/scanner?q={encoded}"), NavigateOptions { replace: true, scroll: false, ..Default::default() });
        spawn_local(async move {
            let r = api::scan(&req).await;
            result.set(Some(r));
            running.set(false);
        });
    };

    // Isi awal dari URL: ?q=<json> (bookmark), ?preset=<id>, atau ?watchlist=<id>.
    let query = use_query_map().get_untracked();
    let initial_q = query.get("q").and_then(|q| serde_json::from_str::<ScanRequest>(&q).ok());
    let initial_preset = query.get("preset");
    let initial_watchlist = query.get("watchlist").and_then(|w| w.parse::<i64>().ok());
    if let Some(req) = &initial_q {
        state.load(req);
        run();
    } else if let Some(id) = initial_watchlist {
        state.universe.update(|u| {
            u.watchlist_id = Some(id);
            u.min_avg_value_20d = None;
        });
    }
    if let Some(id) = initial_preset {
        let run = run.clone();
        Effect::new(move |done: Option<bool>| {
            if done == Some(true) {
                return true;
            }
            match presets.get() {
                Some(Ok(list)) => {
                    if let Some(p) = list.iter().find(|p| p.id == id) {
                        state.load(&p.request);
                        state.preset.set(Some(p.id.clone()));
                        run();
                    }
                    true
                }
                _ => false,
            }
        });
    }

    let run_click = run.clone();
    let run_preset = run.clone();
    view! {
        <PageHeader eyebrow="Penyaringan teknikal" title="Scanner">
            <SavedScans state=state />
        </PageHeader>

        <div class="grid gap-6 xl:grid-cols-[minmax(0,26rem)_minmax(0,1fr)]">
            <div class="space-y-5 xl:sticky xl:top-28 xl:self-start">
                <section aria-label="Preset strategi">
                    <p class="eyebrow mb-2">"Mulai dari preset"</p>
                    <div class="flex flex-wrap gap-2">
                        {move || match presets.get() {
                            Some(Ok(list)) => list
                                .into_iter()
                                .map(|p: PresetDto| {
                                    let run = run_preset.clone();
                                    let id = p.id.clone();
                                    let active = move || state.preset.get().as_deref() == Some(id.as_str());
                                    let req = p.request.clone();
                                    let pid = p.id.clone();
                                    view! {
                                        <button
                                            title=p.description.clone()
                                            class=move || if active() { "chip chip-active" } else { "chip" }
                                            on:click=move |_| {
                                                state.load(&req);
                                                state.preset.set(Some(pid.clone()));
                                                state.saved.set(None);
                                                run();
                                            }
                                        >
                                            {p.name.clone()}
                                        </button>
                                    }
                                })
                                .collect_view()
                                .into_any(),
                            _ => ().into_any(),
                        }}
                    </div>
                </section>

                <section class="panel p-4" aria-label="Kondisi">
                    <div class="mb-3 flex items-center justify-between gap-3">
                        <h2 class="font-display text-lg font-semibold">"Kondisi"</h2>
                        <div class="flex rounded-md border border-line p-0.5 text-xs" role="group" aria-label="Gabungkan kondisi">
                            <LogicButton state=state logic=Logic::All label="Semua terpenuhi" />
                            <LogicButton state=state logic=Logic::Any label="Salah satu" />
                        </div>
                    </div>
                    <ul class="space-y-2.5">
                        <For
                            each=move || state.rows.get()
                            key=|r| r.id
                            children=move |r| {
                                let id = r.id;
                                let index = Signal::derive(move || {
                                    state.rows.with(|v| v.iter().position(|x| x.id == id).unwrap_or(0))
                                });
                                view! { <ConditionRow id=id index=index rows=state.rows /> }
                            }
                        />
                    </ul>
                    <Show when=move || state.rows.with(Vec::is_empty)>
                        <p class="rounded-lg border border-dashed border-line px-3 py-4 text-center text-sm text-muted">
                            "Tambahkan minimal satu kondisi, atau pilih filter sinyal di bawah"
                        </p>
                    </Show>
                    <div class="mt-3 flex items-center justify-between">
                        <button
                            class="btn"
                            disabled=move || state.rows.with(|v| v.len() >= 10)
                            on:click=move |_| state.add()
                        >
                            "+ Tambah kondisi"
                        </button>
                        <button
                            class="btn-primary"
                            disabled=move || running.get() || (state.rows.with(Vec::is_empty) && state.universe.with(|u| u.signals.is_empty() && u.quality.is_empty() && u.min_win_rate.is_none()))
                            on:click=move |_| {
                                state.preset.set(None);
                                run_click();
                            }
                        >
                            {move || if running.get() { "Menjalankan…" } else { "Jalankan scan" }}
                        </button>
                    </div>
                </section>

                <UniverseFilters state=state />
            </div>

            <section aria-live="polite" class="min-w-0">
                {move || match result.get() {
                    None if running.get() => view! { <Skeleton class="h-96" /> }.into_any(),
                    None => view! {
                        <div class="panel flex h-full min-h-64 flex-col items-center justify-center px-6 py-12 text-center">
                            <p class="font-display text-2xl font-semibold">"Susun kondisi, lalu jalankan scan"</p>
                            <p class="mt-2 max-w-md text-sm text-muted">
                                "Atau pilih preset di kiri. Hasil dihitung dari 300 hari bursa terakhir semua saham yang tersimpan."
                            </p>
                        </div>
                    }
                        .into_any(),
                    Some(Err(e)) if e.status == 503 => view! { <NoData /> }.into_any(),
                    Some(Err(e)) => view! { <ErrorNote message=e.message /> }.into_any(),
                    Some(Ok(r)) => view! { <Results result=r dim=running sort=sort /> }.into_any(),
                }}
            </section>
        </div>
    }
}

#[component]
fn LogicButton(state: ScanState, logic: Logic, label: &'static str) -> impl IntoView {
    view! {
        <button
            class=move || {
                if state.logic.get() == logic {
                    "rounded bg-raised px-2.5 py-1 text-paper"
                } else {
                    "rounded px-2.5 py-1 text-muted hover:text-paper"
                }
            }
            aria-pressed=move || (state.logic.get() == logic).to_string()
            on:click=move |_| state.logic.set(logic)
        >
            {label}
        </button>
    }
}

fn parse_opt(s: &str) -> Option<f64> {
    let s = s.trim().replace('.', "").replace(',', ".");
    if s.is_empty() { None } else { s.parse().ok() }
}

#[component]
fn UniverseFilters(state: ScanState) -> impl IntoView {
    let stocks = LocalResource::new(|| api::stocks("", 1000));
    let watchlists = LocalResource::new(api::watchlists);
    let sectors = move || -> Vec<String> {
        match stocks.get() {
            Some(Ok(list)) => list.into_iter().filter_map(|s| s.sector).collect::<BTreeSet<_>>().into_iter().collect(),
            _ => vec![],
        }
    };
    let u = state.universe;
    let num_input = move |label: &'static str, get: fn(&Universe) -> Option<f64>, set: fn(&mut Universe, Option<f64>), scale: f64| {
        view! {
            <label class="flex flex-col gap-1 text-xs text-muted">
                {label}
                <input
                    type="text"
                    inputmode="decimal"
                    placeholder="bebas"
                    class="num"
                    prop:value=move || u.with(|u| get(u).map(|v| format::number(v / scale, 0)).unwrap_or_default())
                    on:change=move |ev| {
                        let v = parse_opt(&event_target_value(&ev)).map(|v| v * scale);
                        u.update(|u| set(u, v));
                    }
                />
            </label>
        }
    };

    view! {
        <section class="panel p-4" aria-label="Filter saham">
            <h2 class="mb-3 font-display text-lg font-semibold">"Saham yang dipindai"</h2>
            <div class="grid grid-cols-3 gap-2">
                {num_input("Harga min", |u| u.min_price, |u, v| u.min_price = v, 1.0)}
                {num_input("Harga maks", |u| u.max_price, |u, v| u.max_price = v, 1.0)}
                {num_input("Nilai/hari min (Rp M)", |u| u.min_avg_value_20d, |u, v| u.min_avg_value_20d = v, 1e9)}
            </div>
            <label class="mt-3 flex flex-col gap-1 text-xs text-muted">
                "Batasi ke watchlist"
                <select
                    prop:value=move || u.with(|u| u.watchlist_id.map(|i| i.to_string()).unwrap_or_default())
                    on:change=move |ev| {
                        let id = event_target_value(&ev).parse::<i64>().ok();
                        u.update(|u| u.watchlist_id = id);
                    }
                >
                    <option value="">"Semua saham"</option>
                    {move || match watchlists.get() {
                        Some(Ok(list)) => list
                            .into_iter()
                            .map(|w| view! { <option value=w.id.to_string()>{w.name}</option> })
                            .collect_view()
                            .into_any(),
                        _ => ().into_any(),
                    }}
                </select>
            </label>
            <p class="mt-3 mb-1.5 text-xs text-muted">"Sinyal"</p>
            <div class="flex flex-wrap gap-1.5">
                {SignalLevel::ALL
                    .into_iter()
                    .map(|level| {
                        let on = Signal::derive(move || u.with(|u| u.signals.contains(&level)));
                        view! {
                            <button
                                class=move || if on.get() { format!("chip chip-active {}", signal_class(level)) } else { "chip".to_string() }
                                aria-pressed=move || on.get().to_string()
                                on:click=move |_| {
                                    u.update(|u| {
                                        if let Some(i) = u.signals.iter().position(|x| *x == level) {
                                            u.signals.remove(i);
                                        } else {
                                            u.signals.push(level);
                                        }
                                    })
                                }
                            >
                                {level.label()}
                            </button>
                        }
                    })
                    .collect_view()}
            </div>
            <p class="mt-3 mb-1.5 text-xs text-muted">"Fundamental"</p>
            <div class="flex flex-wrap gap-1.5">
                {QualityLevel::ALL
                    .into_iter()
                    .map(|level| {
                        let on = Signal::derive(move || u.with(|u| u.quality.contains(&level)));
                        view! {
                            <button
                                class=move || if on.get() { format!("chip chip-active {}", quality_class(level)) } else { "chip".to_string() }
                                aria-pressed=move || on.get().to_string()
                                on:click=move |_| {
                                    u.update(|u| {
                                        if let Some(i) = u.quality.iter().position(|x| *x == level) {
                                            u.quality.remove(i);
                                        } else {
                                            u.quality.push(level);
                                        }
                                    })
                                }
                            >
                                {level.label()}
                            </button>
                        }
                    })
                    .collect_view()}
            </div>
            <div class="mt-3 grid grid-cols-2 gap-2">
                {num_input("Win rate min (%)", |u| u.min_win_rate, |u, v| u.min_win_rate = v, 1.0)}
                <label class="flex flex-col gap-1 text-xs text-muted">
                    "Minimal sinyal"
                    <input
                        type="text"
                        inputmode="numeric"
                        class="num"
                        placeholder=WinRateDto::MIN_SIGNALS.to_string()
                        prop:value=move || u.with(|u| u.min_win_signals.map(|v| v.to_string()).unwrap_or_default())
                        on:change=move |ev| {
                            let v = event_target_value(&ev).trim().parse::<u32>().ok();
                            u.update(|u| u.min_win_signals = v);
                        }
                    />
                </label>
            </div>
            <p class="mt-1 text-[11px] leading-snug text-muted">
                "Win rate: bagian sinyal beli (Buy/Best Buy) di saham itu yang untung setelah 20 hari. Hasil uji: win rate masa lalu tidak memprediksi hasil berikutnya, jadi pakai sebagai informasi, bukan jaminan."
            </p>
            <p class="mt-3 mb-1.5 text-xs text-muted">"Sektor"</p>
            <div class="flex flex-wrap gap-1.5">
                {move || {
                    sectors()
                        .into_iter()
                        .map(|s| {
                            let name = s.clone();
                            let on = {
                                let s = s.clone();
                                Signal::derive(move || u.with(|u| u.sectors.contains(&s)))
                            };
                            view! {
                                <button
                                    class=move || if on.get() { "chip chip-active" } else { "chip" }
                                    aria-pressed=move || on.get().to_string()
                                    on:click=move |_| {
                                        u.update(|u| {
                                            if let Some(i) = u.sectors.iter().position(|x| *x == name) {
                                                u.sectors.remove(i);
                                            } else {
                                                u.sectors.push(name.clone());
                                            }
                                        })
                                    }
                                >
                                    {s}
                                </button>
                            }
                        })
                        .collect_view()
                }}
            </div>
        </section>
    }
}

#[component]
fn SavedScans(state: ScanState) -> impl IntoView {
    let list = LocalResource::new(api::saved_scans);
    let (naming, set_naming) = signal(false);
    let (name, set_name) = signal(String::new());
    let (message, set_message) = signal(None::<(bool, String)>);

    let save = move || {
        let body = SaveScanDto { name: name.get_untracked(), rules: state.request() };
        let existing = state.saved.get_untracked().filter(|s| s.name == body.name);
        spawn_local(async move {
            let r = match existing {
                Some(s) => api::update_scan(s.id, &body).await,
                None => api::save_scan(&body).await,
            };
            match r {
                Ok(s) => {
                    set_message.set(Some((true, format!("Tersimpan sebagai “{}”", s.name))));
                    state.saved.set(Some(s));
                    set_naming.set(false);
                    list.refetch();
                }
                Err(e) => set_message.set(Some((false, e.message))),
            }
        });
    };
    let remove = move || {
        if let Some(s) = state.saved.get_untracked() {
            spawn_local(async move {
                if api::delete_scan(s.id).await.is_ok() {
                    state.saved.set(None);
                    set_message.set(Some((true, format!("“{}” dihapus", s.name))));
                    list.refetch();
                }
            });
        }
    };

    view! {
        <select
            aria-label="Scan tersimpan"
            class="min-w-44"
            prop:value=move || state.saved.get().map(|s| s.id.to_string()).unwrap_or_default()
            on:change=move |ev| {
                let id = event_target_value(&ev).parse::<i64>().ok();
                if let Some(Ok(all)) = list.get_untracked()
                    && let Some(s) = all.into_iter().find(|s| Some(s.id) == id) {
                        state.load(&s.rules);
                        state.preset.set(None);
                        set_name.set(s.name.clone());
                        state.saved.set(Some(s));
                    }
            }
        >
            <option value="">"Scan tersimpan…"</option>
            {move || match list.get() {
                Some(Ok(all)) => all
                    .into_iter()
                    .map(|s| view! { <option value=s.id.to_string()>{s.name}</option> })
                    .collect_view()
                    .into_any(),
                _ => ().into_any(),
            }}
        </select>
        <Show
            when=move || naming.get()
            fallback=move || {
                view! {
                    <button class="btn" on:click=move |_| {
                        set_message.set(None);
                        set_naming.set(true);
                    }>"Simpan scan"</button>
                    <Show when=move || state.saved.get().is_some()>
                        <button class="btn hover:!border-down hover:!text-down" on:click=move |_| remove()>"Hapus"</button>
                    </Show>
                }
            }
        >
            <form
                class="flex items-center gap-2"
                on:submit=move |ev| {
                    ev.prevent_default();
                    save();
                }
            >
                <input
                    type="text"
                    placeholder="Nama scan"
                    aria-label="Nama scan"
                    required
                    prop:value=name
                    on:input=move |ev| set_name.set(event_target_value(&ev))
                />
                <button type="submit" class="btn-primary">"Simpan"</button>
                <button type="button" class="btn" on:click=move |_| set_naming.set(false)>"Batal"</button>
            </form>
        </Show>
        {move || message.get().map(|(ok, m)| view! {
            <span class=if ok { "text-xs text-up" } else { "text-xs text-down" }>{m}</span>
        })}
    }
}

#[derive(Clone, PartialEq)]
enum SortKey {
    Ticker,
    Close,
    Change,
    Volume,
    Value,
    Column(String),
    Signal,
    Quality,
    WinRate,
}

fn sort_value(row: &ScanRowDto, key: &SortKey) -> Option<f64> {
    match key {
        SortKey::Ticker => None,
        SortKey::Close => Some(row.close),
        SortKey::Change => row.change_pct,
        SortKey::Volume => Some(row.volume),
        SortKey::Value => row.avg_value_20,
        SortKey::Column(k) => row.values.get(k).copied().flatten(),
        SortKey::Signal => row.signal.as_ref().map(|s| f64::from(s.score)),
        // Sehat paling atas saat diurutkan menurun.
        SortKey::WinRate => row.win_rate.filter(|w| w.signals >= WinRateDto::MIN_SIGNALS).map(|w| w.rate()),
        SortKey::Quality => row.quality.map(|q| 3.0 - QualityLevel::ALL.iter().position(|x| *x == q).unwrap_or(3) as f64),
    }
}

#[component]
fn Results(result: ScanResultDto, dim: RwSignal<bool>, sort: RwSignal<Option<(SortKey, bool)>>) -> impl IntoView {
    let columns = result.columns.clone();
    let rows = result.rows.clone();
    let sorted = move || {
        let mut rows = rows.clone();
        if let Some((key, desc)) = sort.get() {
            rows.sort_by(|a, b| {
                let o = if key == SortKey::Ticker {
                    a.ticker.cmp(&b.ticker)
                } else {
                    match (sort_value(a, &key), sort_value(b, &key)) {
                        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                        (Some(_), None) => std::cmp::Ordering::Greater,
                        (None, Some(_)) => std::cmp::Ordering::Less,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                };
                if desc { o.reverse() } else { o }
            });
        }
        rows
    };
    let header = move |label: String, key: SortKey, align: &'static str| {
        let k = key.clone();
        let indicator = move || match sort.get() {
            Some((s, desc)) if s == k => if desc { " ↓" } else { " ↑" },
            _ => "",
        };
        view! {
            <th scope="col" class=format!("px-3 py-2.5 font-normal {align}")>
                <button
                    class="eyebrow whitespace-nowrap hover:text-paper"
                    on:click=move |_| {
                        let desc = match sort.get_untracked() {
                            Some((s, d)) if s == key => !d,
                            _ => key != SortKey::Ticker,
                        };
                        sort.set(Some((key.clone(), desc)));
                    }
                >
                    {label}
                    {indicator}
                </button>
            </th>
        }
    };

    let total = result.total_matched;
    let shown = result.rows.len();
    view! {
        <div class=move || if dim.get() { "opacity-50 transition-opacity" } else { "transition-opacity" }>
            <div class="mb-3 flex flex-wrap items-baseline justify-between gap-2">
                <p>
                    <span class="num font-display text-3xl font-bold text-brass">{total}</span>
                    <span class="ml-2 text-muted">"saham cocok"</span>
                </p>
                <p class="text-xs text-muted">
                    {format!("Per penutupan {}", format::date(&result.as_of))}
                    {(total > shown).then(|| format!(" · menampilkan {shown} teratas"))}
                </p>
            </div>
            {if total == 0 {
                view! {
                    <div class="panel px-6 py-12 text-center">
                        <p class="font-display text-xl font-semibold">"Tidak ada saham yang memenuhi semua kondisi"</p>
                        <p class="mt-2 text-sm text-muted">
                            "Longgarkan salah satu kondisi, perpanjang “dalam N hari”, atau turunkan filter nilai transaksi."
                        </p>
                    </div>
                }
                    .into_any()
            } else {
                view! {
                    <div class="panel overflow-x-auto">
                        <table class="w-full whitespace-nowrap text-sm">
                            <thead class="border-b border-line text-left">
                                <tr>
                                    {header("Kode".into(), SortKey::Ticker, "text-left")}
                                    <th scope="col" class="px-3 py-2.5 font-normal"><span class="eyebrow">"Nama"</span></th>
                                    {header("Harga".into(), SortKey::Close, "text-right")}
                                    {header("1H".into(), SortKey::Change, "text-right")}
                                    {header("Volume".into(), SortKey::Volume, "text-right")}
                                    {header("Nilai 20H".into(), SortKey::Value, "text-right")}
                                    {header("Sinyal".into(), SortKey::Signal, "text-left")}
                                    {header("Fundamental".into(), SortKey::Quality, "text-left")}
                                    {header("Win rate".into(), SortKey::WinRate, "text-right")}
                                    <th scope="col" class="px-3 py-2.5 text-left font-normal"><span class="eyebrow">"Risiko"</span></th>
                                    {columns
                                        .iter()
                                        .map(|c| header(c.label.clone(), SortKey::Column(c.key.clone()), "text-right"))
                                        .collect_view()}
                                </tr>
                            </thead>
                            <tbody>
                                {move || {
                                    let cols = columns.clone();
                                    sorted()
                                        .into_iter()
                                        .map(|r| {
                                            let href = format!("/stock/{}", r.ticker);
                                            view! {
                                                <tr class="border-b border-line/50 last:border-0 hover:bg-raised">
                                                    <td class="px-3 py-2">
                                                        <a href=href class="num font-medium text-brass hover:underline">{r.ticker.clone()}</a>
                                                    </td>
                                                    <td class="max-w-56 truncate px-3 py-2 text-muted" title=r.name.clone()>{format::short_name(&r.name)}</td>
                                                    <td class="num px-3 py-2 text-right">{format::price(r.close)}</td>
                                                    <td class="px-3 py-2 text-right"><Change value=r.change_pct /></td>
                                                    <td class="num px-3 py-2 text-right text-muted">{format::compact(r.volume)}</td>
                                                    <td class="num px-3 py-2 text-right text-muted">
                                                        {r.avg_value_20.map(format::rupiah).unwrap_or_else(|| "–".into())}
                                                    </td>
                                                    <td class="px-3 py-2">
                                                        {r.signal.clone().map(|s| view! { <SignalBadge level=s.level title=format!("Skor {}: {}", s.score, s.reasons.join("; ")) /> })}
                                                    </td>
                                                    <td class="px-3 py-2">
                                                        {r.quality.map(|q| view! { <QualityBadge level=Some(q) /> })}
                                                    </td>
                                                    <td class="px-3 py-2 text-right">{win_rate_cell(r.win_rate)}</td>
                                                    <td class="px-3 py-2"><RiskBadges flags=r.risk.clone() /></td>
                                                    {cols
                                                        .iter()
                                                        .map(|c| {
                                                            let v = r.values.get(&c.key).copied().flatten();
                                                            view! { <td class="num px-3 py-2 text-right">{format::value(v)}</td> }
                                                        })
                                                        .collect_view()}
                                                </tr>
                                            }
                                        })
                                        .collect_view()
                                }}
                            </tbody>
                        </table>
                    </div>
                }
                    .into_any()
            }}
        </div>
    }
}

/// Label risiko ringkas di tabel; detail lengkap ada di tooltip dan halaman saham.
#[component]
fn RiskBadges(flags: Vec<shared::RiskFlagDto>) -> impl IntoView {
    if flags.is_empty() {
        return view! { <span class="text-xs text-muted">"–"</span> }.into_any();
    }
    let extra = flags.len().saturating_sub(2);
    view! {
        <span class="flex items-center gap-1">
            {flags
                .into_iter()
                .take(2)
                .map(|f| {
                    let (tone, _) = crate::pages::stock::risk_style(f.level);
                    let tooltip = format!("{}: {}", f.title, f.detail);
                    view! {
                        <span title=tooltip class=format!("rounded border px-1.5 py-0.5 text-[11px] {tone}")>
                            {f.title}
                        </span>
                    }
                })
                .collect_view()}
            {(extra > 0).then(|| view! { <span class="text-[11px] text-muted">{format!("+{extra}")}</span> })}
        </span>
    }
        .into_any()
}

/// `58% (7/12)`; redup kalau sinyalnya kurang dari minimal (angkanya belum bermakna).
fn win_rate_cell(w: Option<WinRateDto>) -> impl IntoView {
    w.map(|w| {
        let enough = w.signals >= WinRateDto::MIN_SIGNALS;
        let class = if !enough { "num text-muted" } else if w.rate() >= 50.0 { "num text-up" } else { "num" };
        view! {
            <span class=class title=format!("{} dari {} sinyal beli untung setelah 20 hari; rata-rata {}", w.wins, w.signals, format::pct(Some(w.avg_return)))>
                {format!("{:.0}%", w.rate())}
                <span class="ml-1 text-[11px] text-muted">{format!("({}/{})", w.wins, w.signals)}</span>
            </span>
        }
    })
}
