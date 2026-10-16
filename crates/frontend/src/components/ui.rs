use leptos::prelude::*;
use shared::SignalLevel;

use crate::format;

/// Perubahan harga berwarna (`+1,23%`).
#[component]
pub fn Change(#[prop(into)] value: Signal<Option<f64>>) -> impl IntoView {
    view! { <span class=move || format!("num {}", format::tone(value.get()))>{move || format::pct(value.get())}</span> }
}

#[component]
pub fn ErrorNote(#[prop(into)] message: String) -> impl IntoView {
    view! {
        <div role="alert" class="rounded-lg border border-down/40 bg-down/10 px-4 py-3 text-sm text-paper">
            {message}
        </div>
    }
}

#[component]
pub fn Skeleton(#[prop(into, default = "h-24".to_string())] class: String) -> impl IntoView {
    view! { <div class=format!("skeleton rounded-lg {class}")></div> }
}

#[component]
pub fn PageHeader(
    #[prop(into)] eyebrow: Signal<String>,
    #[prop(into)] title: String,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    view! {
        <header class="mb-6 flex flex-wrap items-end justify-between gap-4">
            <div>
                <p class="eyebrow">{move || eyebrow.get()}</p>
                <h1 class="font-display text-3xl font-bold tracking-tight sm:text-4xl">{title}</h1>
            </div>
            {children.map(|c| view! { <div class="flex flex-wrap items-center gap-2">{c()}</div> })}
        </header>
    }
}

/// Tanda bahwa data belum ada, dengan arahan ke halaman Data.
#[component]
pub fn NoData() -> impl IntoView {
    view! {
        <div class="panel px-6 py-10 text-center">
            <p class="font-display text-xl font-semibold">Belum ada data harga</p>
            <p class="mt-2 text-sm text-muted">
                "Jalankan update data dulu. Backfill 5 tahun biasanya selesai dalam kurang dari satu menit."
            </p>
            <a href="/data" class="btn-primary mt-5">"Buka halaman Data"</a>
        </div>
    }
}

/// Kelas warna untuk setiap rating sinyal.
pub fn signal_class(level: SignalLevel) -> &'static str {
    match level {
        SignalLevel::BestBuy => "border-up bg-up text-ink",
        SignalLevel::Buy => "border-up/60 text-up",
        SignalLevel::Hold => "border-brass/60 text-brass",
        SignalLevel::Neutral => "border-line text-muted",
        SignalLevel::Sell => "border-down/60 text-down",
        SignalLevel::BestSell => "border-down bg-down text-ink",
    }
}

/// Label rating sinyal; `title` berisi alasan kalau ada.
#[component]
pub fn SignalBadge(
    level: SignalLevel,
    #[prop(optional)] title: Option<String>,
    #[prop(optional)] large: bool,
) -> impl IntoView {
    let size = if large { "px-3 py-1 text-sm" } else { "px-2 py-0.5 text-[11px]" };
    view! {
        <span
            title=title
            class=format!("inline-block whitespace-nowrap rounded-full border font-medium {size} {}", signal_class(level))
        >
            {level.label()}
        </span>
    }
}

/// Menjalankan `f` setiap `secs` detik selama komponen pemanggil masih tampil.
pub fn every(secs: u64, f: impl Fn() + 'static) {
    let handle = set_interval_with_handle(f, std::time::Duration::from_secs(secs));
    on_cleanup(move || {
        if let Ok(h) = handle {
            h.clear();
        }
    });
}

/// Keterangan data intraday, mis. `Intraday 10:35 WIB · tertunda ±15 menit`; `None` di luar jam bursa.
pub fn live_label(live: Option<&shared::LiveStatusDto>) -> Option<String> {
    let live = live.filter(|l| l.state != "closed")?;
    let at = live.last_update.as_deref()?;
    Some(format!("Intraday {at} WIB · tertunda ±15 menit"))
}

/// Memantau status update data (tiap 20 detik, ringan) dan memanggil `f` setiap kali data harga
/// berubah: putaran intraday selesai atau update harian selesai. Pemuatan awal tidak memicu `f`.
/// Status yang dipantau dikembalikan supaya bisa ikut ditampilkan.
pub fn on_data_change(f: impl Fn() + 'static) -> LocalResource<crate::api::Res<shared::IngestStatusDto>> {
    let status = LocalResource::new(crate::api::ingest_status);
    every(20, move || status.refetch());
    Effect::new(move |prev: Option<Option<String>>| {
        let current = match status.get() {
            Some(Ok(s)) => Some(format!(
                "{:?}|{:?}",
                s.live.and_then(|l| l.last_update),
                s.history.first().map(|r| (r.id, r.status.clone()))
            )),
            _ => None,
        };
        let last = prev.flatten();
        if let (Some(before), Some(now)) = (&last, &current)
            && before != now
        {
            f();
        }
        current.or(last)
    });
    status
}

/// Kelas warna untuk setiap tingkat kesehatan fundamental.
pub fn quality_class(level: shared::QualityLevel) -> &'static str {
    use shared::QualityLevel::*;
    match level {
        Healthy => "border-up/60 text-up",
        Fair => "border-brass/60 text-brass",
        Weak => "border-line text-muted",
        Poor => "border-down/60 text-down",
    }
}

/// Label kesehatan fundamental; `None` = data tidak lengkap.
#[component]
pub fn QualityBadge(level: Option<shared::QualityLevel>, #[prop(optional)] title: Option<String>) -> impl IntoView {
    let (class, label) = match level {
        Some(l) => (quality_class(l), l.label()),
        None => ("border-line text-muted", "Data kurang"),
    };
    view! {
        <span title=title class=format!("inline-block whitespace-nowrap rounded-full border px-2 py-0.5 text-[11px] font-medium {class}")>
            {label}
        </span>
    }
}
