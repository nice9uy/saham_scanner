use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::WatchlistDto;

use crate::api;
use crate::components::ui::{Change, ErrorNote, PageHeader, SignalBadge, Skeleton, on_data_change};
use crate::format;

#[component]
pub fn Watchlist() -> impl IntoView {
    let lists = LocalResource::new(api::watchlists);
    // Harga, sinyal, dan trailing stop ikut diperbarui setiap ada data harga baru.
    on_data_change(move || lists.refetch());
    let selected = RwSignal::new(None::<i64>);
    // Disimpan di sini, bukan di `ListView`, supaya tidak hilang saat data dimuat ulang otomatis.
    let draft = RwSignal::new(String::new());
    let confirm = RwSignal::new(false);
    let (new_name, set_new_name) = signal(String::new());
    let (error, set_error) = signal(None::<String>);

    let current = move || -> Option<WatchlistDto> {
        let Some(Ok(all)) = lists.get() else { return None };
        let id = selected.get();
        all.iter().find(|w| Some(w.id) == id).or_else(|| all.first()).cloned()
    };

    let create = move || {
        let name = new_name.get_untracked();
        spawn_local(async move {
            match api::create_watchlist(&name).await {
                Ok(w) => {
                    set_new_name.set(String::new());
                    set_error.set(None);
                    selected.set(Some(w.id));
                    lists.refetch();
                }
                Err(e) => set_error.set(Some(e.message)),
            }
        });
    };

    view! {
        <PageHeader eyebrow="Pantauan pribadi" title="Watchlist">
            <form
                class="flex items-center gap-2"
                on:submit=move |ev| {
                    ev.prevent_default();
                    create();
                }
            >
                <input
                    type="text"
                    required
                    placeholder="Nama watchlist baru"
                    aria-label="Nama watchlist baru"
                    prop:value=new_name
                    on:input=move |ev| set_new_name.set(event_target_value(&ev))
                />
                <button type="submit" class="btn-primary">"Buat watchlist"</button>
            </form>
        </PageHeader>
        {move || error.get().map(|m| view! { <div class="mb-4"><ErrorNote message=m /></div> })}

        {move || match lists.get() {
            None => view! { <Skeleton class="h-64" /> }.into_any(),
            Some(Err(e)) => view! { <ErrorNote message=e.message /> }.into_any(),
            Some(Ok(all)) if all.is_empty() => view! {
                <div class="panel px-6 py-12 text-center">
                    <p class="font-display text-xl font-semibold">"Belum ada watchlist"</p>
                    <p class="mt-2 text-sm text-muted">"Buat watchlist di atas, lalu tambahkan saham dari sini atau dari halaman detail saham."</p>
                </div>
            }
                .into_any(),
            Some(Ok(all)) => {
                let active_id = current().map(|w| w.id);
                view! {
                    <div class="mb-4 flex flex-wrap gap-2" role="tablist">
                        {all
                            .into_iter()
                            .map(|w| {
                                let is_active = Some(w.id) == active_id;
                                view! {
                                    <button
                                        role="tab"
                                        aria-selected=is_active.to_string()
                                        class=if is_active { "chip chip-active" } else { "chip" }
                                        on:click=move |_| {
                                            selected.set(Some(w.id));
                                            confirm.set(false);
                                        }
                                    >
                                        {w.name.clone()}
                                        <span class="ml-1.5 font-mono text-muted">{w.items.len()}</span>
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>
                    {current().map(|w| view! { <ListView w=w lists=lists selected=selected draft=draft confirm=confirm /> })}
                }
                    .into_any()
            }
        }}
    }
}

#[component]
fn ListView(
    w: WatchlistDto,
    lists: LocalResource<api::Res<Vec<WatchlistDto>>>,
    selected: RwSignal<Option<i64>>,
    draft: RwSignal<String>,
    confirm: RwSignal<bool>,
) -> impl IntoView {
    let id = w.id;
    let (ticker, set_ticker) = draft.split();
    let (error, set_error) = signal(None::<String>);
    let (confirm, set_confirm) = confirm.split();

    let add = move || {
        let t = ticker.get_untracked().trim().to_uppercase();
        if t.is_empty() {
            return;
        }
        spawn_local(async move {
            match api::add_to_watchlist(id, &t).await {
                Ok(()) => {
                    set_ticker.set(String::new());
                    set_error.set(None);
                    lists.refetch();
                }
                Err(e) => set_error.set(Some(e.message)),
            }
        });
    };
    let remove = move |t: String| {
        spawn_local(async move {
            if api::remove_from_watchlist(id, &t).await.is_ok() {
                lists.refetch();
            }
        });
    };
    let delete = move || {
        spawn_local(async move {
            if api::delete_watchlist(id).await.is_ok() {
                selected.set(None);
                lists.refetch();
            }
        });
    };

    view! {
        <section class="panel overflow-hidden">
            <div class="flex flex-wrap items-center justify-between gap-3 border-b border-line px-4 py-3">
                <form
                    class="flex items-center gap-2"
                    on:submit=move |ev| {
                        ev.prevent_default();
                        add();
                    }
                >
                    <input
                        type="text"
                        placeholder="Kode saham, mis. BBCA"
                        aria-label="Kode saham"
                        class="num w-44 uppercase"
                        prop:value=ticker
                        on:input=move |ev| set_ticker.set(event_target_value(&ev))
                    />
                    <button type="submit" class="btn">"Tambah saham"</button>
                </form>
                <div class="flex items-center gap-2">
                    <a href=format!("/scanner?watchlist={id}") class="btn-primary">"Scan watchlist ini"</a>
                    <Show
                        when=move || confirm.get()
                        fallback=move || view! {
                            <button class="btn hover:!border-down hover:!text-down" on:click=move |_| set_confirm.set(true)>"Hapus watchlist"</button>
                        }
                    >
                        <span class="text-xs text-muted">"Hapus beserta isinya?"</span>
                        <button class="btn !border-down !text-down" on:click=move |_| delete()>"Ya, hapus"</button>
                        <button class="btn" on:click=move |_| set_confirm.set(false)>"Batal"</button>
                    </Show>
                </div>
            </div>
            {move || error.get().map(|m| view! { <p class="px-4 pt-3 text-sm text-down">{m}</p> })}
            {if w.items.is_empty() {
                view! { <p class="px-4 py-10 text-center text-sm text-muted">"Watchlist ini masih kosong. Tambahkan kode saham di atas."</p> }.into_any()
            } else {
                view! {
                    <div class="overflow-x-auto">
                        <table class="w-full whitespace-nowrap text-sm">
                            <thead class="border-b border-line text-left">
                                <tr>
                                    <th scope="col" class="px-4 py-2.5 font-normal"><span class="eyebrow">"Kode"</span></th>
                                    <th scope="col" class="px-4 py-2.5 font-normal"><span class="eyebrow">"Nama"</span></th>
                                    <th scope="col" class="px-4 py-2.5 text-right font-normal"><span class="eyebrow">"Harga"</span></th>
                                    <th scope="col" class="px-4 py-2.5 text-right font-normal"><span class="eyebrow">"1H"</span></th>
                                    <th scope="col" class="px-4 py-2.5 text-right font-normal"><span class="eyebrow">"Volume"</span></th>
                                    <th scope="col" class="px-4 py-2.5 text-left font-normal"><span class="eyebrow">"Sinyal"</span></th>
                                    <th scope="col" class="px-4 py-2.5 text-right font-normal" title="Batas keluar untuk posisi yang sedang dipegang: high 22 hari − 3 × ATR 22">
                                        <span class="eyebrow">"Trailing stop"</span>
                                    </th>
                                    <th scope="col" class="px-4 py-2.5"><span class="sr-only">"Aksi"</span></th>
                                </tr>
                            </thead>
                            <tbody>
                                {w.items
                                    .into_iter()
                                    .map(|i| {
                                        let t = i.ticker.clone();
                                        view! {
                                            <tr class="border-b border-line/50 last:border-0 hover:bg-raised">
                                                <td class="px-4 py-2.5">
                                                    <a href=format!("/stock/{}", i.ticker) class="num font-medium text-brass hover:underline">{i.ticker.clone()}</a>
                                                </td>
                                                <td class="max-w-64 truncate px-4 py-2.5 text-muted" title=i.name.clone()>{format::short_name(&i.name)}</td>
                                                <td class="num px-4 py-2.5 text-right">{i.close.map(format::price).unwrap_or_else(|| "–".into())}</td>
                                                <td class="px-4 py-2.5 text-right"><Change value=i.change_pct /></td>
                                                <td class="num px-4 py-2.5 text-right text-muted">{i.volume.map(format::compact).unwrap_or_else(|| "–".into())}</td>
                                                <td class="px-4 py-2.5">{i.signal.map(|level| view! { <SignalBadge level=level /> })}</td>
                                                <td class="num px-4 py-2.5 text-right">{trailing_cell(i.close, i.trailing_stop)}</td>
                                                <td class="px-4 py-2.5 text-right">
                                                    <button
                                                        class="text-xs text-muted hover:text-down"
                                                        aria-label=format!("Hapus {} dari watchlist", i.ticker)
                                                        on:click=move |_| remove(t.clone())
                                                    >
                                                        "Hapus"
                                                    </button>
                                                </td>
                                            </tr>
                                        }
                                    })
                                    .collect_view()}
                            </tbody>
                        </table>
                    </div>
                }
                    .into_any()
            }}
        </section>
    }
}

/// Trailing stop dengan jaraknya dari harga; merah dengan tanda "keluar" kalau harga sudah di bawahnya.
fn trailing_cell(close: Option<f64>, stop: Option<f64>) -> impl IntoView {
    match (close, stop) {
        (Some(c), Some(s)) if c > 0.0 && s > 0.0 => {
            let below = c < s;
            let gap = (s / c - 1.0) * 100.0;
            view! {
                <span class=if below { "text-down" } else { "" }>{format::price(s)}</span>
                <span class=if below { "ml-1.5 text-xs font-medium text-down" } else { "ml-1.5 text-xs text-muted" }>
                    {if below { "di bawah · keluar".to_string() } else { format::pct(Some(gap)) }}
                </span>
            }
                .into_any()
        }
        _ => view! { <span class="text-muted">"–"</span> }.into_any(),
    }
}
