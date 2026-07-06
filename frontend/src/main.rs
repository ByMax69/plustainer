use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use yew::prelude::*;

#[derive(Clone, PartialEq, Deserialize, Serialize)]
pub struct Stack {
    #[serde(rename = "Id")]    pub id: u32,
    #[serde(rename = "Name")]  pub name: String,
    #[serde(rename = "Status")] pub status: u32,
}

#[derive(Clone, PartialEq, Deserialize, Serialize)]
pub struct PortInfo {
    #[serde(rename = "PrivatePort")]  pub private_port: u16,
    #[serde(rename = "PublicPort")]   pub public_port: Option<u16>,
    #[serde(rename = "Type")]         pub port_type: Option<String>,
    #[serde(rename = "IP")]           pub ip: Option<String>,
}

#[derive(Clone, PartialEq, Deserialize, Serialize)]
pub struct Container {
    #[serde(rename = "Id")]     pub id: String,
    #[serde(rename = "Names")]  pub names: Vec<String>,
    #[serde(rename = "State")]  pub state: String,
    #[serde(rename = "Labels")] pub labels: Option<std::collections::HashMap<String, String>>,
    #[serde(rename = "Ports")]  pub ports: Option<Vec<PortInfo>>,
}

#[derive(Clone, PartialEq, Deserialize, Serialize)]
pub struct FavoriteGroup {
    pub id: String,
    pub name: String,
    pub stacks: Vec<String>,
}

#[derive(Clone, PartialEq, Deserialize, Serialize)]
pub struct AppConfig {
    pub server_local_ip: String,
}

// Legacy favorites field kept for migration; pages is ignored (unknown fields are dropped by serde)
#[derive(Deserialize, Serialize)]
pub struct AppSettings {
    #[serde(default)]
    pub favorite_groups: Vec<FavoriteGroup>,
    pub custom_order: Vec<String>,
    pub sort_order: String,
    #[serde(default)]
    pub favorites: Vec<String>,
}

#[derive(Clone, PartialEq)]
pub struct Notification {
    pub id: u32,
    pub message: String,
    pub success: bool,
}

fn save_to_backend(groups: Vec<FavoriteGroup>, custom_order: Vec<String>, sort_order: String) {
    let settings = AppSettings {
        favorite_groups: groups,
        custom_order,
        sort_order,
        favorites: vec![],
    };
    wasm_bindgen_futures::spawn_local(async move {
        let _ = Request::post("/api/settings").json(&settings).unwrap().send().await;
    });
}

fn make_group_id(name: &str, existing: &[FavoriteGroup]) -> String {
    let base: String = name.to_lowercase().replace(' ', "-")
        .chars().filter(|c| c.is_alphanumeric() || *c == '-').collect();
    if !existing.iter().any(|g| g.id == base) { return base.clone(); }
    (2..).find_map(|i| {
        let candidate = format!("{}-{}", base, i);
        if !existing.iter().any(|g| g.id == candidate) { Some(candidate) } else { None }
    }).unwrap_or(base)
}

#[function_component(App)]
fn app() -> Html {
    let stacks           = use_state(Vec::<Stack>::new);
    let containers       = use_state(Vec::<Container>::new);
    let favorite_groups  = use_state(Vec::<FavoriteGroup>::new);
    let custom_order     = use_state(Vec::<String>::new);
    let sort_order       = use_state(|| "custom".to_string());
    let settings_open    = use_state(|| false);
    let error            = use_state(|| None::<String>);
    let initial_fetch_done = use_state(|| false);
    let search_query     = use_state(|| String::new());
    let server_ip        = use_state(|| String::new());
    let notifications    = use_state(Vec::<Notification>::new);
    let notif_counter    = use_mut_ref(|| 0u32);
    // None = all stacks; Some(group_id) = filter to that group
    let active_filter    = use_state(|| None::<String>);
    // Some(stack_name) = star popup open for that stack
    let star_popup       = use_state(|| None::<String>);
    // Create / rename group modals
    let show_create_group = use_state(|| false);
    let rename_group      = use_state(|| None::<FavoriteGroup>);
    let dragged_stack    = use_state(|| None::<String>);
    let drag_over_stack  = use_state(|| None::<String>);

    // ── Notifications ─────────────────────────────────────────────────────────
    let dismiss_notification = {
        let notifications = notifications.clone();
        Callback::from(move |id: u32| {
            let mut n = (*notifications).clone();
            n.retain(|x| x.id != id);
            notifications.set(n);
        })
    };
    let push_notification = {
        let notifications = notifications.clone();
        let counter = notif_counter.clone();
        let dismiss = dismiss_notification.clone();
        Callback::from(move |(msg, ok): (String, bool)| {
            let id = { let mut c = counter.borrow_mut(); *c += 1; *c };
            let mut n = (*notifications).clone();
            n.push(Notification { id, message: msg, success: ok });
            notifications.set(n);
            let d = dismiss.clone();
            gloo_timers::callback::Timeout::new(4000, move || d.emit(id)).forget();
        })
    };

    // ── Fetch ─────────────────────────────────────────────────────────────────
    let fetch_data = {
        let stacks = stacks.clone(); let containers = containers.clone();
        let favorite_groups = favorite_groups.clone(); let custom_order = custom_order.clone();
        let sort_order = sort_order.clone(); let error = error.clone();
        let initial_fetch_done = initial_fetch_done.clone(); let server_ip = server_ip.clone();
        Callback::from(move |_: ()| {
            let stacks = stacks.clone(); let containers = containers.clone();
            let favorite_groups = favorite_groups.clone(); let custom_order = custom_order.clone();
            let sort_order = sort_order.clone(); let error = error.clone();
            let initial_fetch_done = initial_fetch_done.clone(); let server_ip = server_ip.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if !*initial_fetch_done {
                    if let Ok(res) = Request::get("/api/config").send().await {
                        if let Ok(d) = res.json::<AppConfig>().await { server_ip.set(d.server_local_ip); }
                    }
                    if let Ok(res) = Request::get("/api/settings").send().await {
                        if let Ok(data) = res.json::<AppSettings>().await {
                            // Migration: legacy `favorites` → default group
                            if data.favorite_groups.is_empty() && !data.favorites.is_empty() {
                                favorite_groups.set(vec![FavoriteGroup {
                                    id: "favorites".to_string(),
                                    name: "Favoris".to_string(),
                                    stacks: data.favorites,
                                }]);
                            } else {
                                favorite_groups.set(data.favorite_groups);
                            }
                            custom_order.set(data.custom_order);
                            sort_order.set(data.sort_order);
                        }
                    }
                    initial_fetch_done.set(true);
                }
                match Request::get("/api/stacks").send().await {
                    Ok(res) => { if let Ok(d) = res.json::<Vec<Stack>>().await { stacks.set(d); } }
                    Err(e) => error.set(Some(e.to_string())),
                }
                match Request::get("/api/containers").send().await {
                    Ok(res) => { if let Ok(d) = res.json::<Vec<Container>>().await { containers.set(d); } }
                    Err(e) => error.set(Some(e.to_string())),
                }
            });
        })
    };
    { let fd = fetch_data.clone();
      use_effect_with((), move |_| {
          fd.emit(());
          let iv = gloo_timers::callback::Interval::new(5000, move || fd.emit(()));
          || drop(iv)
      });
    }

    // ── Stack / container actions ─────────────────────────────────────────────
    let toggle_stack = {
        let fetch_data = fetch_data.clone(); let push_notification = push_notification.clone();
        Callback::from(move |(id, name, running): (u32, String, bool)| {
            let fd = fetch_data.clone(); let pn = push_notification.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let action = if running { "stop" } else { "start" };
                let ok = Request::post(&format!("/api/stacks/{}/{}", id, action)).send().await.is_ok();
                let msg = if running {
                    if ok { format!("Stack '{}' arrêtée", name) } else { format!("Erreur : impossible d'arrêter '{}'", name) }
                } else {
                    if ok { format!("Stack '{}' démarrée", name) } else { format!("Erreur : impossible de démarrer '{}'", name) }
                };
                pn.emit((msg, ok)); fd.emit(());
            });
        })
    };
    let toggle_container = {
        let fetch_data = fetch_data.clone(); let push_notification = push_notification.clone();
        Callback::from(move |(id, name, action): (String, String, String)| {
            let fd = fetch_data.clone(); let pn = push_notification.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let ok = Request::post(&format!("/api/containers/{}/{}", id, action)).send().await.is_ok();
                let word = match action.as_str() { "start" => "démarré", "stop" => "arrêté", _ => "redémarré" };
                let msg = if ok { format!("Conteneur '{}' {}", name, word) } else { format!("Erreur sur '{}'", name) };
                pn.emit((msg, ok)); fd.emit(());
            });
        })
    };

    // ── Drag & drop ───────────────────────────────────────────────────────────
    let handle_drop = {
        let custom_order = custom_order.clone(); let sort_order = sort_order.clone();
        let dragged_stack = dragged_stack.clone(); let drag_over_stack = drag_over_stack.clone();
        let all_stacks = stacks.clone(); let favorite_groups = favorite_groups.clone();
        Callback::from(move |target: String| {
            if let Some(dragged) = (*dragged_stack).clone() {
                if dragged != target {
                    let mut order = (*custom_order).clone();
                    if order.is_empty() { order = all_stacks.iter().map(|s| s.name.clone()).collect(); }
                    if let Some(di) = order.iter().position(|x| *x == dragged) {
                        order.remove(di);
                        if let Some(ti) = order.iter().position(|x| *x == target) { order.insert(ti, dragged); }
                        else { order.push(dragged); }
                    } else if let Some(ti) = order.iter().position(|x| *x == target) { order.insert(ti, dragged); }
                    else { order.push(dragged); }
                    let new_order = order.clone();
                    let new_sort  = "custom".to_string();
                    custom_order.set(new_order.clone());
                    sort_order.set(new_sort.clone());
                    save_to_backend((*favorite_groups).clone(), new_order, new_sort);
                }
            }
            dragged_stack.set(None); drag_over_stack.set(None);
        })
    };

    // ── Toggle stack in group ─────────────────────────────────────────────────
    let toggle_stack_in_group = {
        let favorite_groups = favorite_groups.clone();
        let custom_order = custom_order.clone(); let sort_order = sort_order.clone();
        Callback::from(move |(stack_name, group_id): (String, String)| {
            let name_lower = stack_name.to_lowercase();
            let mut groups = (*favorite_groups).clone();
            if let Some(g) = groups.iter_mut().find(|g| g.id == group_id) {
                if g.stacks.contains(&name_lower) { g.stacks.retain(|s| s != &name_lower); }
                else { g.stacks.push(name_lower); }
            }
            favorite_groups.set(groups.clone());
            save_to_backend(groups, (*custom_order).clone(), (*sort_order).clone());
        })
    };

    // ── Filtering ─────────────────────────────────────────────────────────────
    let active_group = (*active_filter).as_ref()
        .and_then(|id| favorite_groups.iter().find(|g| &g.id == id).cloned());

    let mut filtered_stacks: Vec<Stack> = if let Some(ref g) = active_group {
        stacks.iter().filter(|s| g.stacks.contains(&s.name.to_lowercase())).cloned().collect()
    } else {
        (*stacks).clone()
    };

    let query = search_query.to_lowercase();
    if !query.is_empty() {
        filtered_stacks.retain(|s| {
            if s.name.to_lowercase().contains(&query) { return true; }
            containers.iter().any(|c| {
                let in_stack = c.labels.as_ref().map(|l|
                    l.get("com.docker.compose.project") == Some(&s.name)
                    || l.get("com.docker.stack.namespace") == Some(&s.name)
                ).unwrap_or(false);
                in_stack && c.names.get(0).cloned().unwrap_or_default().to_lowercase().contains(&query)
            })
        });
    }

    match (*sort_order).as_str() {
        "alpha" => filtered_stacks.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        "status" => filtered_stacks.sort_by(|a, b| b.status.cmp(&a.status).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))),
        "custom" => {
            let order = &*custom_order;
            if !order.is_empty() {
                filtered_stacks.sort_by(|a, b|
                    order.iter().position(|x| *x == a.name).unwrap_or(usize::MAX)
                    .cmp(&order.iter().position(|x| *x == b.name).unwrap_or(usize::MAX))
                );
            }
        }
        _ => {}
    }

    // Pre-compute star popup — Yew html! doesn't allow `let` bindings inside if-let branches
    let star_popup_frag: Html = if let Some(popup_stack) = (*star_popup).clone() {
        let close1 = { let sp = star_popup.clone(); Callback::from(move |_: MouseEvent| sp.set(None)) };
        let close2 = { let sp = star_popup.clone(); Callback::from(move |_: MouseEvent| sp.set(None)) };
        let add_group_cb = {
            let scg = show_create_group.clone();
            let sp  = star_popup.clone();
            Callback::from(move |_: MouseEvent| { sp.set(None); scg.set(true); })
        };
        let groups_snap = (*favorite_groups).clone();
        let empty_msg: Html = if groups_snap.is_empty() {
            html! { <p class="mini-popup-empty">{ "Aucun groupe. Créez-en un avec le bouton ＋." }</p> }
        } else { html! {} };
        let group_rows: Html = groups_snap.iter().map(|g| {
            let in_group = g.stacks.contains(&popup_stack.to_lowercase());
            let gid = g.id.clone();
            let sn  = popup_stack.clone();
            let tsg = toggle_stack_in_group.clone();
            html! {
                <label class="mini-popup-option">
                    <input type="checkbox" checked={in_group}
                        onchange={Callback::from(move |_| tsg.emit((sn.clone(), gid.clone())))} />
                    <span>{ &g.name }</span>
                    <span class="mini-popup-count">{ g.stacks.len() }</span>
                </label>
            }
        }).collect();
        html! {
            <div class="mini-overlay" onclick={close1}>
                <div class="mini-popup" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                    <div class="mini-popup-header">
                        <span>{ format!("Favoris — {}", popup_stack) }</span>
                        <button class="mini-popup-close" onclick={close2}>{ "×" }</button>
                    </div>
                    { empty_msg }
                    { group_rows }
                    <button class="mini-popup-add-group" onclick={add_group_cb}>{ "＋ Nouveau groupe" }</button>
                </div>
            </div>
        }
    } else { html! {} };

    // Pre-compute rename group modal
    let rename_group_frag: Html = if let Some(g) = (*rename_group).clone() {
        let rg_close = rename_group.clone();
        let rg_save  = rename_group.clone();
        let fgs = favorite_groups.clone();
        let co  = custom_order.clone();
        let so  = sort_order.clone();
        let gid = g.id.clone();
        html! {
            <GroupNameModal
                title={ "Renommer le groupe".to_string() }
                initial_name={ g.name.clone() }
                on_close={ Callback::from(move |_: ()| rg_close.set(None)) }
                on_save={
                    Callback::from(move |new_name: String| {
                        if !new_name.trim().is_empty() {
                            let mut groups = (*fgs).clone();
                            if let Some(gr) = groups.iter_mut().find(|x| x.id == gid) {
                                gr.name = new_name;
                            }
                            fgs.set(groups.clone());
                            save_to_backend(groups, (*co).clone(), (*so).clone());
                        }
                        rg_save.set(None);
                    })
                }
            />
        }
    } else { html! {} };

    html! {
        <>
            // ── Header ────────────────────────────────────────────────────────
            <header>
                <div class="logo">{ "⚡ Plustainer" }</div>
                <div style="display:flex;gap:0.75rem;align-items:center;">
                    <input type="text" placeholder="Rechercher..." value={(*search_query).clone()}
                        oninput={
                            let sq = search_query.clone();
                            Callback::from(move |e: InputEvent| {
                                let i: web_sys::HtmlInputElement = e.target_unchecked_into();
                                sq.set(i.value());
                            })
                        }
                        style="padding:0.45rem 1rem;border-radius:20px;background:rgba(255,255,255,0.08);border:1px solid rgba(255,255,255,0.12);color:white;outline:none;font-size:0.9rem;"
                    />
                    <button class="btn" onclick={
                        let so = settings_open.clone();
                        Callback::from(move |_: MouseEvent| so.set(!*so))
                    }>{ "⚙️ Paramètres" }</button>
                    <button class="btn" onclick={move |_| fetch_data.emit(())}>{ "🔄" }</button>
                </div>
            </header>

            // ── Filter bar ────────────────────────────────────────────────────
            <div class="filter-bar">
                // "Toutes"
                <button
                    class={if active_filter.is_none() { "filter-pill filter-pill--active" } else { "filter-pill" }}
                    onclick={ let af = active_filter.clone(); Callback::from(move |_: MouseEvent| af.set(None)) }
                >{ "Toutes" }</button>

                // One pill per favorites group
                {
                    (*favorite_groups).iter().map(|g| {
                        let gid = g.id.clone();
                        let gid_del = g.id.clone();
                        let gid_active = g.id.clone();
                        let is_active = active_filter.as_deref() == Some(&gid_active);
                        let count = g.stacks.len();
                        let g_for_rename = g.clone();

                        let fgs_del = favorite_groups.clone();
                        let co_del  = custom_order.clone();
                        let so_del  = sort_order.clone();
                        let af_del  = active_filter.clone();

                        html! {
                            <div class={if is_active { "group-pill group-pill--active" } else { "group-pill" }}>
                                <span class="group-pill-label" onclick={
                                    let af = active_filter.clone();
                                    Callback::from(move |_: MouseEvent| af.set(Some(gid.clone())))
                                }>
                                    { &g.name }
                                    if count > 0 {
                                        <span class="group-pill-count">{ count }</span>
                                    }
                                </span>
                                <div class="group-pill-actions">
                                    <button class="group-pill-btn group-pill-edit" title="Renommer"
                                        onclick={
                                            let rg = rename_group.clone();
                                            Callback::from(move |e: MouseEvent| {
                                                e.stop_propagation();
                                                rg.set(Some(g_for_rename.clone()));
                                            })
                                        }
                                    >{ "✏" }</button>
                                    <button class="group-pill-btn group-pill-delete" title="Supprimer"
                                        onclick={
                                            Callback::from(move |e: MouseEvent| {
                                                e.stop_propagation();
                                                let mut gs = (*fgs_del).clone();
                                                gs.retain(|x| x.id != gid_del);
                                                fgs_del.set(gs.clone());
                                                if af_del.as_deref() == Some(&gid_del) { af_del.set(None); }
                                                save_to_backend(gs, (*co_del).clone(), (*so_del).clone());
                                            })
                                        }
                                    >{ "×" }</button>
                                </div>
                            </div>
                        }
                    }).collect::<Html>()
                }

                // "+" create new group
                <button class="filter-pill-add" title="Nouveau groupe de favoris"
                    onclick={
                        let scg = show_create_group.clone();
                        Callback::from(move |_: MouseEvent| scg.set(true))
                    }
                >{ "＋" }</button>
            </div>

            // ── Main grid ─────────────────────────────────────────────────────
            <main>
                if let Some(err) = &*error {
                    <div style="color:red;padding:2rem;">{ format!("Erreur: {}", err) }</div>
                }
                if let Some(ref g) = active_group {
                    <div class="page-banner">
                        <div class="page-banner-title">{ format!("⭐ {}", g.name) }</div>
                        <button class="btn" style="font-size:0.8rem;" onclick={
                            let af = active_filter.clone();
                            Callback::from(move |_: MouseEvent| af.set(None))
                        }>{ "Voir toutes les stacks" }</button>
                    </div>
                }
                <div class="stacks-grid" style="animation:fadeIn 0.3s ease-out;">
                    {
                        filtered_stacks.iter().map(|stack| {
                            let is_active = stack.status == 1;
                            let sname = stack.name.clone();
                            let sname_drop = stack.name.clone();
                            let sname_star = stack.name.clone();
                            let sid = stack.id;

                            let in_any_group = favorite_groups.iter()
                                .any(|g| g.stacks.contains(&stack.name.to_lowercase()));

                            let stack_containers: Vec<&Container> = containers.iter().filter(|c| {
                                c.labels.as_ref().map(|l|
                                    l.get("com.docker.compose.project") == Some(&stack.name)
                                    || l.get("com.docker.stack.namespace") == Some(&stack.name)
                                ).unwrap_or(false)
                            }).collect();
                            let running_count = stack_containers.iter().filter(|c| c.state == "running").count();

                            let on_toggle = {
                                let ts = toggle_stack.clone(); let name = sname.clone();
                                Callback::from(move |_| ts.emit((sid, name.clone(), is_active)))
                            };
                            let on_star = {
                                let sp = star_popup.clone(); let name = sname_star.clone();
                                Callback::from(move |e: MouseEvent| {
                                    e.stop_propagation();
                                    sp.set(Some(name.clone()));
                                })
                            };
                            let on_drag_start = {
                                let ds = dragged_stack.clone(); let name = sname.clone();
                                Callback::from(move |e: DragEvent| {
                                    if let Some(dt) = e.data_transfer() { let _ = dt.set_data("text/plain", &name); dt.set_effect_allowed("move"); }
                                    ds.set(Some(name.clone()));
                                })
                            };
                            let on_drag_over = {
                                let dos = drag_over_stack.clone(); let name = sname.clone();
                                Callback::from(move |e: DragEvent| {
                                    e.prevent_default();
                                    if let Some(dt) = e.data_transfer() { dt.set_drop_effect("move"); }
                                    if *dos != Some(name.clone()) { dos.set(Some(name.clone())); }
                                })
                            };
                            let on_drag_leave = { let dos = drag_over_stack.clone(); Callback::from(move |_: DragEvent| dos.set(None)) };
                            let on_drop = {
                                let hd = handle_drop.clone();
                                Callback::from(move |e: DragEvent| { e.prevent_default(); hd.emit(sname_drop.clone()); })
                            };

                            let mut card_class = "stack-card".to_string();
                            if *drag_over_stack == Some(stack.name.clone()) { card_class.push_str(" drag-over"); }

                            html! {
                                <div class={card_class} draggable="true"
                                     ondragstart={on_drag_start} ondragover={on_drag_over}
                                     ondragleave={on_drag_leave} ondrop={on_drop}>
                                    <div class="stack-header">
                                        <div style="display:flex;align-items:flex-start;gap:0.5rem;min-width:0;">
                                            <button
                                                class={if in_any_group { "star-btn star-btn--on" } else { "star-btn" }}
                                                onclick={on_star}
                                                title="Gérer les favoris"
                                            >{ if in_any_group { "★" } else { "☆" } }</button>
                                            <div style="min-width:0;">
                                                <h3 style="white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">{ &stack.name }</h3>
                                                <span class={if is_active { "status active" } else { "status stopped" }}>
                                                    { if is_active { "Active" } else { "Stoppée" } }
                                                </span>
                                                <span style="font-size:0.8rem;margin-left:0.5rem;">
                                                    { format!("{}/{} conteneurs", running_count, stack_containers.len()) }
                                                </span>
                                            </div>
                                        </div>
                                        <button class={if is_active { "btn btn-danger" } else { "btn" }} onclick={on_toggle} style="flex-shrink:0;margin-left:0.5rem;">
                                            { if is_active { "■ Stopper" } else { "▶ Démarrer" } }
                                        </button>
                                    </div>

                                    <div class="containers-list">
                                        {
                                            stack_containers.iter().map(|c| {
                                                let cid   = c.id.clone();
                                                let cname = c.names.get(0).cloned().unwrap_or_default().replace("/", "");
                                                let running = c.state == "running";
                                                let mk = |act: &'static str| {
                                                    let tc = toggle_container.clone();
                                                    let id = cid.clone(); let n = cname.clone();
                                                    Callback::from(move |_| tc.emit((id.clone(), n.clone(), act.into())))
                                                };
                                                let on_start   = mk("start");
                                                let on_stop    = mk("stop");
                                                let on_restart = mk("restart");

                                                let mut urls = Vec::new();
                                                if let Some(labels) = &c.labels {
                                                    for (k, v) in labels {
                                                        if k.starts_with("traefik.http.routers.") && k.ends_with(".rule") && v.contains("Host(") {
                                                            let sc = if v.contains("Host(`") { '`' } else { '"' };
                                                            let parts: Vec<&str> = v.split(sc).collect();
                                                            if parts.len() >= 3 { urls.push(format!("http://{}", parts[1])); }
                                                        }
                                                    }
                                                }
                                                urls.sort(); urls.dedup();

                                                let mut pub_ports: Vec<u16> = Vec::new();
                                                if let Some(ports) = &c.ports {
                                                    for p in ports { if let Some(pp) = p.public_port { if pp > 0 { pub_ports.push(pp); } } }
                                                }
                                                pub_ports.sort(); pub_ports.dedup();
                                                let sip = (*server_ip).clone();

                                                html! {
                                                    <div class="container-item" style="flex-direction:column;align-items:stretch;gap:0.5rem;padding:0.5rem;">
                                                        <div style="display:flex;justify-content:space-between;align-items:center;width:100%;">
                                                            <span style={if running { "color:#22c55e" } else { "color:#ef4444" }}>
                                                                { if running { "🟢" } else { "🔴" } }{ " " }{ cname }
                                                            </span>
                                                            <div>
                                                                if !running {
                                                                    <button class="btn" style="padding:0.2rem 0.5rem;margin-left:0.5rem;" onclick={on_start}>{ "▶" }</button>
                                                                } else {
                                                                    <button class="btn btn-danger" style="padding:0.2rem 0.5rem;margin-left:0.5rem;" onclick={on_stop}>{ "■" }</button>
                                                                    <button class="btn" style="padding:0.2rem 0.5rem;margin-left:0.5rem;" onclick={on_restart}>{ "↻" }</button>
                                                                }
                                                            </div>
                                                        </div>
                                                        if !urls.is_empty() || !pub_ports.is_empty() {
                                                            <div style="display:flex;gap:0.5rem;flex-wrap:wrap;font-size:0.8rem;margin-left:1.5rem;">
                                                                { for urls.iter().map(|url| html! {
                                                                    <a href={url.clone()} target="_blank" style="color:#60a5fa;text-decoration:none;background:rgba(96,165,250,0.1);padding:0.1rem 0.4rem;border-radius:4px;display:inline-flex;align-items:center;gap:0.2rem;">{"🔗"}{url.replace("http://","").replace("https://","")}</a>
                                                                }) }
                                                                { for pub_ports.iter().map(|port| {
                                                                    let host = if !sip.is_empty() { sip.clone() } else {
                                                                        web_sys::window().and_then(|w| w.location().hostname().ok()).unwrap_or_else(|| "localhost".to_string())
                                                                    };
                                                                    html! { <a href={format!("http://{}:{}", host, port)} target="_blank" style="color:#34d399;text-decoration:none;background:rgba(52,211,153,0.1);padding:0.1rem 0.4rem;border-radius:4px;display:inline-flex;align-items:center;gap:0.2rem;">{"🔌"}{port}</a> }
                                                                }) }
                                                            </div>
                                                        }
                                                    </div>
                                                }
                                            }).collect::<Html>()
                                        }
                                    </div>
                                </div>
                            }
                        }).collect::<Html>()
                    }
                </div>
            </main>

            // ── Star popup (group selector) ───────────────────────────────────
            { star_popup_frag }

            // ── Toasts ────────────────────────────────────────────────────────
            <div class="toast-container">
                {
                    (*notifications).iter().map(|n| {
                        let id = n.id; let d = dismiss_notification.clone();
                        html! {
                            <div class={if n.success { "toast toast-success" } else { "toast toast-error" }}>
                                <span class="toast-icon">{ if n.success { "✓" } else { "✕" } }</span>
                                <span class="toast-message">{ &n.message }</span>
                                <button class="toast-close" onclick={Callback::from(move |_| d.emit(id))}>{ "×" }</button>
                            </div>
                        }
                    }).collect::<Html>()
                }
            </div>

            // ── Settings modal (sort order only) ─────────────────────────────
            if *settings_open {
                <SettingsModal
                    sort_order={(*sort_order).clone()}
                    on_close={
                        let so = settings_open.clone();
                        Callback::from(move |_: ()| so.set(false))
                    }
                    on_save={
                        let sort_order = sort_order.clone();
                        let favorite_groups = favorite_groups.clone();
                        let custom_order = custom_order.clone();
                        let settings_open = settings_open.clone();
                        Callback::from(move |new_sort: String| {
                            sort_order.set(new_sort.clone());
                            settings_open.set(false);
                            save_to_backend((*favorite_groups).clone(), (*custom_order).clone(), new_sort);
                        })
                    }
                />
            }

            // ── Create group modal ────────────────────────────────────────────
            if *show_create_group {
                <GroupNameModal
                    title={ "Nouveau groupe de favoris".to_string() }
                    initial_name={ "".to_string() }
                    on_close={
                        let scg = show_create_group.clone();
                        Callback::from(move |_: ()| scg.set(false))
                    }
                    on_save={
                        let fgs = favorite_groups.clone();
                        let co = custom_order.clone(); let so = sort_order.clone();
                        let scg = show_create_group.clone();
                        Callback::from(move |name: String| {
                            if !name.trim().is_empty() {
                                let mut groups = (*fgs).clone();
                                let id = make_group_id(&name, &groups);
                                groups.push(FavoriteGroup { id, name, stacks: vec![] });
                                fgs.set(groups.clone());
                                save_to_backend(groups, (*co).clone(), (*so).clone());
                            }
                            scg.set(false);
                        })
                    }
                />
            }

            // ── Rename group modal (pre-computed above) ───────────────────────
            { rename_group_frag }
        </>
    }
}

// ── Settings modal ────────────────────────────────────────────────────────────
#[derive(Properties, PartialEq)]
pub struct SettingsProps {
    pub sort_order: String,
    pub on_close: Callback<()>,
    pub on_save: Callback<String>,
}

#[function_component(SettingsModal)]
fn settings_modal(props: &SettingsProps) -> Html {
    let sort = use_state(|| props.sort_order.clone());
    let on_close = props.on_close.clone();
    let cb = props.on_save.clone();
    html! {
        <div class="modal-overlay">
            <div class="modal">
                <h3>{ "Paramètres" }</h3>
                <div style="margin:1rem 0;">
                    <label style="font-size:0.8rem;color:#aaa;display:block;margin-bottom:0.5rem;">{ "Ordre d'affichage :" }</label>
                    <select value={(*sort).clone()} onchange={
                        let sort = sort.clone();
                        Callback::from(move |e: Event| { let t: web_sys::HtmlSelectElement = e.target_unchecked_into(); sort.set(t.value()); })
                    } style="width:100%;padding:0.5rem;background:#1a1a2e;color:white;border:1px solid rgba(255,255,255,0.1);border-radius:8px;">
                        <option value="custom">{ "Personnalisé (Glisser-Déposer)" }</option>
                        <option value="default">{ "Par défaut" }</option>
                        <option value="alpha">{ "Alphabétique (A-Z)" }</option>
                        <option value="status">{ "Statut (Actives en premier)" }</option>
                    </select>
                </div>
                <div style="display:flex;gap:1rem;margin-top:1.5rem;">
                    <button class="btn" style="background:#333;" onclick={move |_| on_close.emit(())}>{ "Annuler" }</button>
                    <button class="btn" onclick={
                        let sort = sort.clone();
                        Callback::from(move |_| cb.emit((*sort).clone()))
                    }>{ "Enregistrer" }</button>
                </div>
            </div>
        </div>
    }
}

// ── Group name modal (used for create & rename) ────────────────────────────────
#[derive(Properties, PartialEq)]
pub struct GroupNameProps {
    pub title: String,
    pub initial_name: String,
    pub on_close: Callback<()>,
    pub on_save: Callback<String>,
}

#[function_component(GroupNameModal)]
fn group_name_modal(props: &GroupNameProps) -> Html {
    let name = use_state(|| props.initial_name.clone());
    let on_close = props.on_close.clone();
    let cb = props.on_save.clone();
    html! {
        <div class="modal-overlay">
            <div class="modal" style="width:360px;">
                <h3>{ &props.title }</h3>
                <input
                    type="text"
                    placeholder="Nom du groupe..."
                    value={(*name).clone()}
                    oninput={
                        let name = name.clone();
                        Callback::from(move |e: InputEvent| {
                            let i: web_sys::HtmlInputElement = e.target_unchecked_into();
                            name.set(i.value());
                        })
                    }
                    style="width:100%;padding:0.6rem;background:#1a1a2e;color:white;border:1px solid rgba(255,255,255,0.1);border-radius:6px;font-size:0.95rem;margin-top:1rem;"
                />
                <div style="display:flex;gap:1rem;margin-top:1.5rem;">
                    <button class="btn" style="background:#333;" onclick={move |_| on_close.emit(())}>{ "Annuler" }</button>
                    <button class="btn" onclick={
                        let name = name.clone();
                        Callback::from(move |_| cb.emit((*name).clone()))
                    }>{ "Enregistrer" }</button>
                </div>
            </div>
        </div>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
