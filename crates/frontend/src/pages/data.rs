use leptos::html::Input;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;

use crate::api;
use crate::components::ui::{ErrorNote, PageHeader, Skeleton, every};
use crate::format;

fn status_label(s: &str) -> (&'static str, &'static str) {
    match s {
        "success" => ("Berhasil", "text-up"),
        "partial" => ("Sebagian gagal", "text-brass"),
        "failed" => ("Gagal", "text-down"),
        "running" => ("Berjalan", "text-paper"),
        _ => ("–", "text-muted"),
    }
}

fn trigger_label(t: &str) -> &'static str {
    match t {
        "schedule" => "Terjadwal",
        "manual" => "Manual",
        "backfill" => "Awal (5 tahun)",
        _ => "–",
    }
}

#[component]
pub fn Data() -> impl IntoView {
    let status = LocalResource::new(api::ingest_status);
    let (notice, set_notice) = signal(None::<(bool, String)>);

    // Selama update berjalan, status dimuat ulang tiap 2 detik.
    let handle = set_interval_with_handle(
        move || {
            if matches!(status.get_untracked(), Some(Ok(ref s)) if s.current.is_some()) {
                status.refetch();
            }
        },
        std::time::Duration::from_secs(2),
    );
    on_cleanup(move || {
        if let Ok(h) = handle {
            h.clear();
        }
    });
    // Status update intraday berubah lebih lambat.
    every(15, move || status.refetch());

    let run = move || {
        spawn_local(async move {
            match api::ingest_run().await {
                Ok(_) => set_notice.set(Some((true, "Update data dimulai".into()))),
                Err(e) => set_notice.set(Some((false, e.message))),
            }
            status.refetch();
        });
    };
    let running = move || matches!(status.get(), Some(Ok(ref s)) if s.current.is_some());

    view! {
        <PageHeader eyebrow="Sumber: Yahoo Finance, harian + intraday" title="Data">
            <button class="btn-primary" disabled=running on:click=move |_| run()>
                {move || if running() { "Update berjalan…" } else { "Update sekarang" }}
            </button>
        </PageHeader>
        {move || notice.get().map(|(ok, m)| view! {
            <p class=if ok { "mb-4 text-sm text-up" } else { "mb-4 text-sm text-down" }>{m}</p>
        })}

        {move || match status.get() {
            Some(Ok(s)) => s.current.map(|p| {
                let pct = if p.total == 0 { 0.0 } else { p.done as f64 / p.total as f64 * 100.0 };
                view! {
                    <section class="panel mb-6 p-5">
                        <div class="mb-2 flex items-baseline justify-between">
                            <p class="font-medium">"Mengambil data harga"</p>
                            <p class="num text-sm text-muted">{format!("{} / {} saham", p.done, p.total)}
                                {(p.failed > 0).then(|| format!(" · {} gagal", p.failed))}
                            </p>
                        </div>
                        <div class="h-2 overflow-hidden rounded-full bg-line" role="progressbar"
                            aria-valuemin="0" aria-valuemax=p.total.to_string() aria-valuenow=p.done.to_string()>
                            <div class="h-full bg-brass transition-all" style=format!("width: {pct:.1}%")></div>
                        </div>
                    </section>
                }
            }).into_any(),
            _ => ().into_any(),
        }}

        {move || match status.get() {
            Some(Ok(s)) => s.live.map(|l| view! { <LivePanel live=l /> }).into_any(),
            _ => ().into_any(),
        }}

        <div class="grid gap-6 xl:grid-cols-[minmax(0,1fr)_22rem]">
            <section class="panel overflow-hidden">
                <h2 class="border-b border-line px-4 py-3 font-display text-lg font-semibold">"Riwayat update"</h2>
                {move || match status.get() {
                    None => view! { <div class="p-4"><Skeleton class="h-40" /></div> }.into_any(),
                    Some(Err(e)) => view! { <div class="p-4"><ErrorNote message=e.message /></div> }.into_any(),
                    Some(Ok(s)) if s.history.is_empty() => view! {
                        <p class="px-4 py-10 text-center text-sm text-muted">"Belum ada update. Klik “Update sekarang” untuk mengambil data 5 tahun terakhir."</p>
                    }.into_any(),
                    Some(Ok(s)) => view! {
                        <div class="overflow-x-auto">
                            <table class="w-full whitespace-nowrap text-sm">
                                <thead class="border-b border-line text-left">
                                    <tr>
                                        <th scope="col" class="px-4 py-2.5 font-normal"><span class="eyebrow">"Waktu"</span></th>
                                        <th scope="col" class="px-4 py-2.5 font-normal"><span class="eyebrow">"Jenis"</span></th>
                                        <th scope="col" class="px-4 py-2.5 font-normal"><span class="eyebrow">"Status"</span></th>
                                        <th scope="col" class="px-4 py-2.5 text-right font-normal"><span class="eyebrow">"Berhasil"</span></th>
                                        <th scope="col" class="px-4 py-2.5 font-normal"><span class="eyebrow">"Gagal"</span></th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {s.history.into_iter().map(|r| {
                                        let (label, tone) = status_label(&r.status);
                                        view! {
                                            <tr class="border-b border-line/50 align-top last:border-0">
                                                <td class="num px-4 py-2.5 text-muted">{format::datetime(&r.started_at)}</td>
                                                <td class="px-4 py-2.5">{trigger_label(&r.trigger)}</td>
                                                <td class=format!("px-4 py-2.5 {tone}")>{label}
                                                    {r.error.map(|e| view! { <span class="block text-xs text-muted">{e}</span> })}
                                                </td>
                                                <td class="num px-4 py-2.5 text-right">{r.ok_count}</td>
                                                <td class="px-4 py-2.5">
                                                    {if r.failed_tickers.is_empty() {
                                                        view! { <span class="text-muted">"–"</span> }.into_any()
                                                    } else {
                                                        view! {
                                                            <details>
                                                                <summary class="cursor-pointer text-down">{format!("{} saham", r.failed_tickers.len())}</summary>
                                                                <p class="num mt-1 text-xs leading-relaxed text-muted">{r.failed_tickers.join(" ")}</p>
                                                            </details>
                                                        }.into_any()
                                                    }}
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any(),
                }}
            </section>
            <ImportStocks />
        </div>
    }
}

#[component]
fn ImportStocks() -> impl IntoView {
    let input = NodeRef::<Input>::new();
    let (busy, set_busy) = signal(false);
    let (notice, set_notice) = signal(None::<(bool, String)>);

    let upload = move || {
        let Some(el) = input.get_untracked() else { return };
        let el: web_sys::HtmlInputElement = el.unchecked_into();
        let Some(file) = el.files().and_then(|f| f.get(0)) else {
            set_notice.set(Some((false, "Pilih file CSV dulu".into())));
            return;
        };
        set_busy.set(true);
        spawn_local(async move {
            match api::import_csv(&file).await {
                Ok(r) => set_notice.set(Some((
                    true,
                    format!("{} emiten baru, {} diperbarui. Jalankan “Update sekarang” untuk mengambil harganya.", r.inserted, r.updated),
                ))),
                Err(e) => set_notice.set(Some((false, e.message))),
            }
            set_busy.set(false);
        });
    };

    view! {
        <section class="panel p-4">
            <h2 class="font-display text-lg font-semibold">"Daftar emiten"</h2>
            <p class="mt-1 text-sm leading-relaxed text-muted">
                "Daftar seluruh emiten IDX diperbarui otomatis dari TradingView setiap kali update berjalan, termasuk IPO baru. "
                "Impor CSV hanya perlu kalau ada emiten yang belum tercantum. Unduh “Daftar Saham” dari idx.co.id, simpan sebagai CSV, lalu impor di sini. Kolom yang dikenali: "
                <span class="num text-paper">"Kode"</span>", "<span class="num text-paper">"Nama Perusahaan"</span>
                ", sektor, dan papan pencatatan."
            </p>
            <form
                class="mt-4 flex flex-col gap-3"
                on:submit=move |ev| {
                    ev.prevent_default();
                    upload();
                }
            >
                <input node_ref=input type="file" accept=".csv,text/csv" aria-label="File CSV emiten"
                    class="text-sm file:mr-3 file:rounded file:border-0 file:bg-raised file:px-3 file:py-1 file:text-paper" />
                <button type="submit" class="btn self-start" disabled=busy>
                    {move || if busy.get() { "Mengimpor…" } else { "Impor CSV" }}
                </button>
            </form>
            {move || notice.get().map(|(ok, m)| view! {
                <p class=if ok { "mt-3 text-sm text-up" } else { "mt-3 text-sm text-down" }>{m}</p>
            })}
        </section>
    }
}

#[component]
fn LivePanel(live: shared::LiveStatusDto) -> impl IntoView {
    let (label, tone) = match live.state.as_str() {
        "running" => ("Mengambil harga", "text-up"),
        "waiting" => ("Menunggu putaran berikutnya", "text-paper"),
        "cooldown" => ("Dijeda", "text-brass"),
        _ => ("Bursa tutup", "text-muted"),
    };
    let progress = (live.state == "running").then(|| format!("{} / {} saham", live.done, live.total));
    view! {
        <section class="panel mb-6 p-5">
            <div class="flex flex-wrap items-baseline justify-between gap-2">
                <p class="font-medium">"Update intraday " <span class=tone>{label}</span></p>
                <p class="num text-sm text-muted">
                    {progress}
                    {live.last_update.map(|t| format!(" · terakhir {t} WIB"))}
                    {live.next_at.map(|t| format!(" · berikutnya {t} WIB"))}
                </p>
            </div>
            {live.note.map(|n| view! { <p class="mt-1 text-sm text-brass">{n}</p> })}
            <p class="mt-2 text-xs leading-relaxed text-muted">
                "Selama jam bursa (Senin–Jumat 09:00–16:15 WIB), saham watchlist dan saham likuid diperbarui setiap putaran, sedangkan saham lain bergiliran sebagian demi sebagian. Satu request tiap beberapa detik supaya tidak dibatasi Yahoo. Harga dari Yahoo tertunda ±15 menit. Semua saham diperbarui final saat update harian pukul 17:00."
            </p>
        </section>
    }
}
