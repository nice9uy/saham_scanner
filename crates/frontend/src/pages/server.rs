use leptos::prelude::*;
use shared::ServerStatusDto;

use crate::api;
use crate::components::ui::{ErrorNote, PageHeader, Skeleton, every};
use crate::format;

/// Kondisi server dan aplikasi, diperbarui tiap 5 detik.
#[component]
pub fn Server() -> impl IntoView {
    let status = LocalResource::new(api::server_status);
    every(5, move || status.refetch());
    view! {
        <PageHeader eyebrow="Diperbarui tiap 5 detik" title="Server" />
        {move || match status.get() {
            None => view! { <Skeleton class="h-96" /> }.into_any(),
            Some(Err(e)) => view! { <ErrorNote message=e.message /> }.into_any(),
            Some(Ok(s)) => view! { <Report s=s /> }.into_any(),
        }}
    }
}

/// Batang persentase; berwarna kuning di atas 70% dan merah di atas 90%.
#[component]
fn Meter(pct: f64, #[prop(optional)] thin: bool) -> impl IntoView {
    let pct = pct.clamp(0.0, 100.0);
    let tone = if pct >= 90.0 { "bg-down" } else if pct >= 70.0 { "bg-brass" } else { "bg-up" };
    let h = if thin { "h-1.5" } else { "h-2" };
    view! {
        <div class=format!("{h} overflow-hidden rounded-full bg-line") role="progressbar"
            aria-valuemin="0" aria-valuemax="100" aria-valuenow=format!("{pct:.0}")>
            <div class=format!("h-full {tone} transition-all") style=format!("width: {pct:.1}%")></div>
        </div>
    }
}

fn row(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="flex items-baseline justify-between gap-3 border-b border-line/60 py-1.5 last:border-0">
            <dt class="text-sm text-muted">{label}</dt>
            <dd class="num truncate text-right text-sm">{value}</dd>
        </div>
    }
}

fn pct(v: f64) -> String {
    format!("{}%", format::number(v, 1))
}

#[component]
fn Report(s: ServerStatusDto) -> impl IntoView {
    let cpu = s.cpu;
    let mem = s.memory;
    let used = mem.total.saturating_sub(mem.available);
    let mem_pct = if mem.total > 0 { used as f64 / mem.total as f64 * 100.0 } else { 0.0 };
    let swap_used = mem.swap_total.saturating_sub(mem.swap_free);
    let db = s.database;
    let live = s.live.map(|l| match l.state.as_str() {
        "running" => format!("Mengambil harga ({}/{})", l.done, l.total),
        "waiting" => format!("Menunggu{}", l.next_at.map(|t| format!(" · berikutnya {t} WIB")).unwrap_or_default()),
        "cooldown" => format!("Dijeda karena rate limit{}", l.next_at.map(|t| format!(" sampai {t} WIB")).unwrap_or_default()),
        _ => "Bursa tutup".to_string(),
    });
    view! {
        <div class="grid gap-6 lg:grid-cols-2 xl:grid-cols-3">
            <section class="panel p-5">
                <h2 class="mb-3 font-display text-lg font-semibold">"Sistem"</h2>
                <dl>
                    {row("Hostname", s.hostname)}
                    {row("OS", s.os)}
                    {row("Kernel", s.kernel)}
                    {row("Server menyala", format::duration(s.uptime_secs))}
                    {row("Aplikasi menyala", format::duration(s.app_uptime_secs))}
                    {row("Versi aplikasi", s.app_version)}
                </dl>
            </section>

            <section class="panel p-5">
                <div class="mb-1 flex items-baseline justify-between">
                    <h2 class="font-display text-lg font-semibold">"CPU"</h2>
                    <p class="num font-display text-2xl font-bold">{cpu.usage_pct.map(pct).unwrap_or_else(|| "–".into())}</p>
                </div>
                <p class="mb-3 truncate text-xs text-muted" title=cpu.model.clone()>{format!("{} · {} core", cpu.model, cpu.cores)}</p>
                <Meter pct=cpu.usage_pct.unwrap_or(0.0) />
                <div class="mt-4 grid grid-cols-4 gap-x-3 gap-y-2 sm:grid-cols-6">
                    {cpu.per_core.iter().enumerate().map(|(i, p)| view! {
                        <div>
                            <p class="num mb-0.5 flex justify-between text-[10px] text-muted"><span>{format!("#{i}")}</span><span>{format!("{p:.0}%")}</span></p>
                            <Meter pct=*p thin=true />
                        </div>
                    }).collect_view()}
                </div>
                <dl class="mt-4">
                    {row("Dipakai aplikasi", cpu.app_pct.map(pct).unwrap_or_else(|| "–".into()))}
                    {row("Beban 1 / 5 / 15 menit", cpu.load.iter().map(|l| format::number(*l, 2)).collect::<Vec<_>>().join(" / "))}
                </dl>
            </section>

            <section class="panel p-5">
                <div class="mb-3 flex items-baseline justify-between">
                    <h2 class="font-display text-lg font-semibold">"Memori"</h2>
                    <p class="num font-display text-2xl font-bold">{pct(mem_pct)}</p>
                </div>
                <Meter pct=mem_pct />
                <dl class="mt-4">
                    {row("Terpakai", format!("{} dari {}", format::bytes(used), format::bytes(mem.total)))}
                    {row("Tersedia", format::bytes(mem.available))}
                    {row("Dipakai aplikasi", format::bytes(mem.app_rss))}
                    {row("Batas container", mem.limit.map(format::bytes).unwrap_or_else(|| "Tidak dibatasi".into()))}
                    {row("Swap", if mem.swap_total > 0 { format!("{} dari {}", format::bytes(swap_used), format::bytes(mem.swap_total)) } else { "Tidak ada".into() })}
                </dl>
            </section>

            <section class="panel p-5">
                <h2 class="mb-3 font-display text-lg font-semibold">"Penyimpanan"</h2>
                {s.disks.into_iter().map(|d| {
                    let used = d.total.saturating_sub(d.free);
                    let p = if d.total > 0 { used as f64 / d.total as f64 * 100.0 } else { 0.0 };
                    view! {
                        <div class="mb-4">
                            <p class="mb-1.5 flex justify-between text-sm">
                                <span class="text-muted">{format!("Disk {}", d.mount)}</span>
                                <span class="num">{format!("{} terpakai · sisa {}", format::bytes(used), format::bytes(d.free))}</span>
                            </p>
                            <Meter pct=p />
                            <p class="num mt-1 text-right text-xs text-muted">{format!("Kapasitas {} · {}", format::bytes(d.total), pct(p))}</p>
                        </div>
                    }
                }).collect_view()}
                <dl>
                    {row("Ukuran database", format::bytes(db.size))}
                    {row("PostgreSQL", db.version)}
                    {row("Koneksi aktif", db.connections.to_string())}
                </dl>
            </section>

            <section class="panel overflow-hidden xl:col-span-2">
                <h2 class="border-b border-line px-5 py-3 font-display text-lg font-semibold">"Tabel database"</h2>
                <div class="overflow-x-auto">
                    <table class="w-full whitespace-nowrap text-sm">
                        <thead class="border-b border-line text-left">
                            <tr>
                                <th scope="col" class="px-5 py-2 font-normal"><span class="eyebrow">"Tabel"</span></th>
                                <th scope="col" class="px-5 py-2 text-right font-normal"><span class="eyebrow">"Baris (perkiraan)"</span></th>
                                <th scope="col" class="px-5 py-2 text-right font-normal"><span class="eyebrow">"Ukuran"</span></th>
                            </tr>
                        </thead>
                        <tbody>
                            {db.tables.into_iter().map(|t| view! {
                                <tr class="border-b border-line/50 last:border-0">
                                    <td class="num px-5 py-2">{t.name}</td>
                                    <td class="num px-5 py-2 text-right">{format::number(t.rows as f64, 0)}</td>
                                    <td class="num px-5 py-2 text-right">{format::bytes(t.size)}</td>
                                </tr>
                            }).collect_view()}
                        </tbody>
                    </table>
                </div>
            </section>

            <section class="panel p-5">
                <h2 class="mb-3 font-display text-lg font-semibold">"Aplikasi"</h2>
                <dl>
                    {row("Saham di cache", format::number(s.cache.stocks as f64, 0))}
                    {row("Bar harga di cache", format::number(s.cache.bars as f64, 0))}
                    {row("Cache dimuat", s.cache.loaded_at.as_deref().map(format::datetime).unwrap_or_else(|| "–".into()))}
                    {row("Update harian terakhir", s.last_ingest.as_ref().map(|r| format!("{} · {}", format::datetime(&r.started_at), r.status)).unwrap_or_else(|| "Belum ada".into()))}
                    {row("Update intraday", live.unwrap_or_else(|| "Dimatikan".into()))}
                </dl>
            </section>
        </div>
    }
}
