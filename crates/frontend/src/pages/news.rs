use leptos::prelude::*;
use shared::{BriefDto, BriefItemDto, BriefSectionDto, IndexQuoteDto};

use crate::api;
use crate::components::ui::{Change, ErrorNote, NoData, PageHeader, SignalBadge, Skeleton, live_label, on_data_change};
use crate::format;

/// Ringkasan pasar harian yang disusun otomatis dari data aplikasi.
#[component]
pub fn News() -> impl IntoView {
    let brief = LocalResource::new(api::brief);
    let status = on_data_change(move || brief.refetch());

    let eyebrow = move || {
        let live = match status.get() {
            Some(Ok(s)) => live_label(s.live.as_ref()),
            _ => None,
        };
        match (brief.get(), live) {
            (Some(Ok(b)), Some(live)) => format!("Ringkasan otomatis · {} · {live}", format::date(&b.as_of)),
            (Some(Ok(b)), None) => format!("Ringkasan otomatis · penutupan {}", format::date(&b.as_of)),
            _ => "Ringkasan otomatis".into(),
        }
    };

    view! {
        <PageHeader eyebrow=Signal::derive(eyebrow) title="Berita" />
        {move || match brief.get() {
            None => view! { <Skeleton class="h-96" /> }.into_any(),
            Some(Err(e)) if e.status == 503 => view! { <NoData /> }.into_any(),
            Some(Err(e)) => view! { <ErrorNote message=e.message /> }.into_any(),
            Some(Ok(b)) => view! { <Brief b=b /> }.into_any(),
        }}
    }
}

#[component]
fn Brief(b: BriefDto) -> impl IntoView {
    let tone = match b.market_strong {
        Some(true) => "text-up",
        Some(false) => "text-down",
        None => "text-paper",
    };
    view! {
        {b.ihsg.map(|q| view! { <IhsgQuote q=q /> })}
        <h2 class=format!("mb-6 font-display text-2xl font-bold leading-snug sm:text-3xl {tone}")>{b.headline}</h2>
        <div class="grid gap-6 xl:grid-cols-2">
            {b.sections.into_iter().map(|s| view! { <Section s=s /> }).collect_view()}
        </div>
    }
}

/// IHSG: nilai, perubahan poin, dan persen, mis. `5.228,53 ▼ −635,25 (−10,83%)`.
#[component]
fn IhsgQuote(q: IndexQuoteDto) -> impl IntoView {
    let (arrow, tone) = if q.change > 0.0 {
        ("▲", "text-up")
    } else if q.change < 0.0 {
        ("▼", "text-down")
    } else {
        ("■", "text-muted")
    };
    let signed = |v: f64| format!("{}{}", if v > 0.0 { "+" } else if v < 0.0 { "−" } else { "" }, format::number(v.abs(), 2));
    view! {
        <p class="mb-3 flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <span class="eyebrow">"IHSG"</span>
            <span class="num font-display text-3xl font-bold sm:text-4xl">{format::number(q.close, 2)}</span>
            <span class=format!("num text-lg font-medium sm:text-xl {tone}")>
                {format!("{arrow} {} ({}%)", signed(q.change), signed(q.change_pct))}
            </span>
        </p>
    }
}

#[component]
fn Section(s: BriefSectionDto) -> impl IntoView {
    let accent = match s.tone.as_str() {
        "up" => "border-l-up",
        "down" => "border-l-down",
        "warn" => "border-l-brass",
        _ => "border-l-line",
    };
    view! {
        <section class=format!("panel overflow-hidden border-l-4 {accent}")>
            <h3 class="border-b border-line px-4 py-3 font-display text-lg font-semibold">{s.title}</h3>
            <div class="space-y-2 px-4 py-3 text-sm leading-relaxed text-paper/90">
                {s.paragraphs.into_iter().map(|p| view! { <p>{p}</p> }).collect_view()}
            </div>
            {(!s.items.is_empty()).then(|| view! {
                <ul class="border-t border-line">
                    {s.items.into_iter().map(|i| view! { <Item i=i /> }).collect_view()}
                </ul>
            })}
        </section>
    }
}

#[component]
fn Item(i: BriefItemDto) -> impl IntoView {
    view! {
        <li class="flex items-start gap-3 border-b border-line/50 px-4 py-2.5 last:border-0 hover:bg-raised">
            <a href=format!("/stock/{}", i.ticker) class="num w-14 shrink-0 pt-0.5 font-medium text-brass hover:underline">{i.ticker.clone()}</a>
            <div class="min-w-0 flex-1">
                <p class="flex flex-wrap items-center gap-2 text-sm">
                    <span class="truncate" title=i.name.clone()>{format::short_name(&i.name)}</span>
                    {i.signal.map(|level| view! { <SignalBadge level=level /> })}
                </p>
                {(!i.note.is_empty()).then(|| view! { <p class="mt-0.5 text-xs leading-snug text-muted">{i.note}</p> })}
            </div>
            <div class="shrink-0 text-right text-sm">
                <p class="num">{format::price(i.close)}</p>
                <Change value=i.change_pct />
            </div>
        </li>
    }
}
