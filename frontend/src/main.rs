use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    #[serde(rename = "Labels")] pub labels: Option<HashMap<String, String>>,
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
pub enum NotificationKind {
    Loading,
    Success,
    Error,
}

#[derive(Clone, PartialEq)]
pub struct Notification {
    pub id: u32,
    pub message: String,
    pub kind: NotificationKind,
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
    let stacks             = use_state(Vec::<Stack>::new);
    let containers         = use_state(Vec::<Container>::new);
    let favorite_groups    = use_state(Vec::<FavoriteGroup>::new);
    let custom_order       = use_state(Vec::<String>::new);
    let sort_order         = use_state(|| "custom".to_string());
    let settings_open      = use_state(|| false);
    let error              = use_state(|| None::<String>);
    let initial_fetch_done = use_state(|| false);
    let search_query       = use_state(String::new);
    let server_ip          = use_state(String::new);

    // Notifications backed by RefCell so concurrent async tasks never overwrite each other
    let notifications      = use_state(Vec::<Notification>::new);
    let notif_list_ref     = use_mut_ref(Vec::<Notification>::new);
    let notif_counter      = use_mut_ref(|| 0u32);

    // Pending actions state (stack_id -> "start"|"stop", container_id -> "start"|"stop"|"restart")
    let pending_stacks         = use_state(HashMap::<u32, String>::new);
    let pending_stacks_ref     = use_mut_ref(HashMap::<u32, String>::new);
    let pending_containers     = use_state(HashMap::<String, String>::new);
    let pending_containers_ref = use_mut_ref(HashMap::<String, String>::new);

    // None = all stacks; Some(group_id) = filter to that group
    let active_filter      = use_state(|| None::<String>);
    // Some(stack_name) = star popup open for that stack
    let star_popup         = use_state(|| None::<String>);
    // Create / rename / delete group modals
    let show_create_group    = use_state(|| false);
    let rename_group         = use_state(|| None::<FavoriteGroup>);
    let confirm_delete_group = use_state(|| None::<FavoriteGroup>);
    let dragged_stack        = use_state(|| None::<String>);
    let drag_over_stack      = use_state(|| None::<String>);

    // ── Notifications ─────────────────────────────────────────────────────────
    let dismiss_notification = {
        let notifications = notifications.clone();
        let notif_list_ref = notif_list_ref.clone();
        Callback::from(move |id: u32| {
            let mut list = notif_list_ref.borrow_mut();
            list.retain(|x| x.id != id);
            notifications.set(list.clone());
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
                    Ok(res) => {
                        if let Ok(d) = res.json::<Vec<Stack>>().await {
                            stacks.set(d);
                            error.set(None);
                        }
                    }
                    Err(e) => error.set(Some(e.to_string())),
                }
                match Request::get("/api/containers").send().await {
                    Ok(res) => { if let Ok(d) = res.json::<Vec<Container>>().await { containers.set(d); } }
                    Err(e) => error.set(Some(e.to_string())),
                }
            });
        })
    };
    {
        let fd = fetch_data.clone();
        use_effect_with((), move |_| {
            fd.emit(());
            let iv = gloo_timers::callback::Interval::new(5000, move || fd.emit(()));
            || drop(iv)
        });
    }

    // ── Stack / container actions with loading state & completion toast ───────
    let toggle_stack = {
        let stacks = stacks.clone();
        let containers = containers.clone();
        let pending_stacks = pending_stacks.clone();
        let pending_stacks_ref = pending_stacks_ref.clone();
        let notifications = notifications.clone();
        let notif_list_ref = notif_list_ref.clone();
        let notif_counter = notif_counter.clone();
        let dismiss = dismiss_notification.clone();

        Callback::from(move |(id, name, running): (u32, String, bool)| {
            let action = if running { "stop" } else { "start" };

            // Mark stack as pending immediately
            {
                let mut map = pending_stacks_ref.borrow_mut();
                if map.contains_key(&id) {
                    return;
                }
                map.insert(id, action.to_string());
                pending_stacks.set(map.clone());
            }

            // Show immediate loading toast
            let loading_notif_id = {
                let mut c = notif_counter.borrow_mut();
                *c += 1;
                *c
            };
            let loading_msg = if running {
                format!("Arrêt de la stack « {} » en cours...", name)
            } else {
                format!("Démarrage de la stack « {} » en cours...", name)
            };
            {
                let mut list = notif_list_ref.borrow_mut();
                list.push(Notification {
                    id: loading_notif_id,
                    message: loading_msg,
                    kind: NotificationKind::Loading,
                });
                notifications.set(list.clone());
            }

            let stacks = stacks.clone();
            let containers = containers.clone();
            let pending_stacks = pending_stacks.clone();
            let pending_stacks_ref = pending_stacks_ref.clone();
            let notifications = notifications.clone();
            let notif_list_ref = notif_list_ref.clone();
            let notif_counter = notif_counter.clone();
            let dismiss = dismiss.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let ok = match Request::post(&format!("/api/stacks/{}/{}", id, action)).send().await {
                    Ok(res) => res.ok(),
                    Err(_) => false,
                };

                // Refresh stacks and containers before clearing the loading state
                if let Ok(res) = Request::get("/api/stacks").send().await {
                    if let Ok(d) = res.json::<Vec<Stack>>().await {
                        stacks.set(d);
                    }
                }
                if let Ok(res) = Request::get("/api/containers").send().await {
                    if let Ok(d) = res.json::<Vec<Container>>().await {
                        containers.set(d);
                    }
                }

                // Clear pending state on stack
                {
                    let mut map = pending_stacks_ref.borrow_mut();
                    map.remove(&id);
                    pending_stacks.set(map.clone());
                }

                // Replace loading toast with completion toast
                let done_notif_id = {
                    let mut c = notif_counter.borrow_mut();
                    *c += 1;
                    *c
                };
                let done_msg = if running {
                    if ok {
                        format!("Stack « {} » arrêtée avec succès", name)
                    } else {
                        format!("Erreur : impossible d'arrêter la stack « {} »", name)
                    }
                } else if ok {
                    format!("Stack « {} » démarrée avec succès", name)
                } else {
                    format!("Erreur : impossible de démarrer la stack « {} »", name)
                };

                {
                    let mut list = notif_list_ref.borrow_mut();
                    list.retain(|x| x.id != loading_notif_id);
                    list.push(Notification {
                        id: done_notif_id,
                        message: done_msg,
                        kind: if ok { NotificationKind::Success } else { NotificationKind::Error },
                    });
                    notifications.set(list.clone());
                }

                let d = dismiss.clone();
                gloo_timers::callback::Timeout::new(4500, move || d.emit(done_notif_id)).forget();
            });
        })
    };

    let toggle_container = {
        let stacks = stacks.clone();
        let containers = containers.clone();
        let pending_containers = pending_containers.clone();
        let pending_containers_ref = pending_containers_ref.clone();
        let notifications = notifications.clone();
        let notif_list_ref = notif_list_ref.clone();
        let notif_counter = notif_counter.clone();
        let dismiss = dismiss_notification.clone();

        Callback::from(move |(id, name, action): (String, String, String)| {
            // Mark container as pending immediately
            {
                let mut map = pending_containers_ref.borrow_mut();
                if map.contains_key(&id) {
                    return;
                }
                map.insert(id.clone(), action.clone());
                pending_containers.set(map.clone());
            }

            // Show immediate loading toast
            let loading_notif_id = {
                let mut c = notif_counter.borrow_mut();
                *c += 1;
                *c
            };
            let loading_msg = match action.as_str() {
                "start" => format!("Démarrage du conteneur « {} » en cours...", name),
                "stop" => format!("Arrêt du conteneur « {} » en cours...", name),
                _ => format!("Relance du conteneur « {} » en cours...", name),
            };
            {
                let mut list = notif_list_ref.borrow_mut();
                list.push(Notification {
                    id: loading_notif_id,
                    message: loading_msg,
                    kind: NotificationKind::Loading,
                });
                notifications.set(list.clone());
            }

            let stacks = stacks.clone();
            let containers = containers.clone();
            let pending_containers = pending_containers.clone();
            let pending_containers_ref = pending_containers_ref.clone();
            let notifications = notifications.clone();
            let notif_list_ref = notif_list_ref.clone();
            let notif_counter = notif_counter.clone();
            let dismiss = dismiss.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let ok = match Request::post(&format!("/api/containers/{}/{}", id, action)).send().await {
                    Ok(res) => res.ok(),
                    Err(_) => false,
                };

                // Refresh containers and stacks before clearing the loading state
                if let Ok(res) = Request::get("/api/containers").send().await {
                    if let Ok(d) = res.json::<Vec<Container>>().await {
                        containers.set(d);
                    }
                }
                if let Ok(res) = Request::get("/api/stacks").send().await {
                    if let Ok(d) = res.json::<Vec<Stack>>().await {
                        stacks.set(d);
                    }
                }

                // Clear pending state on container
                {
                    let mut map = pending_containers_ref.borrow_mut();
                    map.remove(&id);
                    pending_containers.set(map.clone());
                }

                // Replace loading toast with completion toast
                let done_notif_id = {
                    let mut c = notif_counter.borrow_mut();
                    *c += 1;
                    *c
                };
                let word = match action.as_str() {
                    "start" => "démarré",
                    "stop" => "arrêté",
                    _ => "relancé",
                };
                let done_msg = if ok {
                    format!("Conteneur « {} » {} avec succès", name, word)
                } else {
                    format!("Erreur lors de l'action sur le conteneur « {} »", name)
                };

                {
                    let mut list = notif_list_ref.borrow_mut();
                    list.retain(|x| x.id != loading_notif_id);
                    list.push(Notification {
                        id: done_notif_id,
                        message: done_msg,
                        kind: if ok { NotificationKind::Success } else { NotificationKind::Error },
                    });
                    notifications.set(list.clone());
                }

                let d = dismiss.clone();
                gloo_timers::callback::Timeout::new(4500, move || d.emit(done_notif_id)).forget();
            });
        })
    };

    // ── Drag & drop + mobile touch reorder ────────────────────────────────────
    let handle_drop = {
        let custom_order = custom_order.clone(); let sort_order = sort_order.clone();
        let dragged_stack = dragged_stack.clone(); let drag_over_stack = drag_over_stack.clone();
        let all_stacks = stacks.clone(); let favorite_groups = favorite_groups.clone();
        Callback::from(move |target: String| {
            if let Some(dragged) = (*dragged_stack).clone() {
                if dragged != target {
                    let mut order = (*custom_order).clone();
                    if order.is_empty() { order = all_stacks.iter().map(|s| s.name.clone()).collect(); }
                    for s in all_stacks.iter() {
                        if !order.contains(&s.name) { order.push(s.name.clone()); }
                    }
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

    let move_stack_step = {
        let custom_order = custom_order.clone();
        let sort_order = sort_order.clone();
        let all_stacks = stacks.clone();
        let favorite_groups = favorite_groups.clone();
        Callback::from(move |(stack_name, delta): (String, isize)| {
            let mut order = (*custom_order).clone();
            if order.is_empty() {
                order = all_stacks.iter().map(|s| s.name.clone()).collect();
            } else {
                for s in all_stacks.iter() {
                    if !order.contains(&s.name) {
                        order.push(s.name.clone());
                    }
                }
            }
            if let Some(pos) = order.iter().position(|x| *x == stack_name) {
                let new_pos = pos as isize + delta;
                if new_pos >= 0 && (new_pos as usize) < order.len() {
                    order.swap(pos, new_pos as usize);
                    let new_sort = "custom".to_string();
                    custom_order.set(order.clone());
                    sort_order.set(new_sort.clone());
                    save_to_backend((*favorite_groups).clone(), order, new_sort);
                }
            }
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

    let total_stacks = stacks.len();
    let active_stacks_count = stacks.iter().filter(|s| s.status == 1).count();
    let can_reorder_touch = *sort_order == "custom" && query.is_empty() && active_filter.is_none();
    let filtered_len = filtered_stacks.len();

    // Pre-compute star popup
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
            html! { <p class="mini-popup-empty">{ "Aucun groupe. Créez-en un avec le bouton ci-dessous." }</p> }
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
                        <button class="mini-popup-close" onclick={close2} aria-label="Fermer">{ "×" }</button>
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

    // Pre-compute delete group confirmation modal
    let confirm_delete_group_frag: Html = if let Some(g) = (*confirm_delete_group).clone() {
        let cdg_close = confirm_delete_group.clone();
        let fgs = favorite_groups.clone();
        let co  = custom_order.clone();
        let so  = sort_order.clone();
        let af  = active_filter.clone();
        let gid = g.id.clone();
        html! {
            <ConfirmModal
                title={ "Supprimer le groupe".to_string() }
                message={ format!("Voulez-vous vraiment supprimer le groupe « {} » ?", g.name) }
                on_close={ Callback::from(move |_: ()| cdg_close.set(None)) }
                on_confirm={
                    let cdg = confirm_delete_group.clone();
                    Callback::from(move |_: ()| {
                        let mut groups = (*fgs).clone();
                        groups.retain(|x| x.id != gid);
                        fgs.set(groups.clone());
                        if af.as_deref() == Some(&gid) { af.set(None); }
                        save_to_backend(groups, (*co).clone(), (*so).clone());
                        cdg.set(None);
                    })
                }
            />
        }
    } else { html! {} };

    html! {
        <div class="app-shell">
            // ── Sticky Top Area (Header + Filter Bar) ─────────────────────────
            <div class="top-sticky">
                <header class="app-header">
                    <div class="header-brand">
                        <div class="logo">{ "⚡ Plustainer" }</div>
                        if total_stacks > 0 {
                            <span class="header-stats" title="Stacks actives / total">
                                { format!("{}/{} actives", active_stacks_count, total_stacks) }
                            </span>
                        }
                    </div>
                    <div class="header-controls">
                        <div class="search-box">
                            <span class="search-icon">{ "🔍" }</span>
                            <input
                                class="search-input"
                                type="search"
                                placeholder="Rechercher stack ou conteneur..."
                                value={(*search_query).clone()}
                                oninput={
                                    let sq = search_query.clone();
                                    Callback::from(move |e: InputEvent| {
                                        let i: web_sys::HtmlInputElement = e.target_unchecked_into();
                                        sq.set(i.value());
                                    })
                                }
                            />
                            if !search_query.is_empty() {
                                <button
                                    class="search-clear"
                                    title="Effacer la recherche"
                                    onclick={
                                        let sq = search_query.clone();
                                        Callback::from(move |_: MouseEvent| sq.set(String::new()))
                                    }
                                >{ "×" }</button>
                            }
                        </div>
                        <div class="header-buttons">
                            <button class="btn btn-header" title="Paramètres d'affichage" onclick={
                                let so = settings_open.clone();
                                Callback::from(move |_: MouseEvent| so.set(!*so))
                            }>
                                <span>{ "⚙️" }</span>
                                <span class="btn-label">{ "Paramètres" }</span>
                            </button>
                            <button class="btn btn-header btn-refresh" title="Rafraîchir" onclick={move |_| fetch_data.emit(())}>
                                { "🔄" }
                            </button>
                        </div>
                    </div>
                </header>

                // ── Filter bar ────────────────────────────────────────────────
                <div class="filter-bar">
                    // "Toutes"
                    <button
                        class={if active_filter.is_none() { "filter-pill filter-pill--active" } else { "filter-pill" }}
                        onclick={ let af = active_filter.clone(); Callback::from(move |_: MouseEvent| af.set(None)) }
                    >
                        <span>{ "Toutes" }</span>
                        if total_stacks > 0 {
                            <span class="filter-pill-count">{ total_stacks }</span>
                        }
                    </button>

                    // One pill per favorites group — single click immediately selects group
                    {
                        (*favorite_groups).iter().map(|g| {
                            let gid = g.id.clone();
                            let gid_active = g.id.clone();
                            let is_active = active_filter.as_deref() == Some(&gid_active);
                            let count = g.stacks.len();
                            let g_for_rename = g.clone();
                            let g_for_delete = g.clone();
                            let cdg = confirm_delete_group.clone();

                            html! {
                                <div
                                    class={if is_active { "group-pill group-pill--active" } else { "group-pill" }}
                                    onclick={
                                        let af = active_filter.clone();
                                        Callback::from(move |_: MouseEvent| af.set(Some(gid.clone())))
                                    }
                                >
                                    <span class="group-pill-label">
                                        { &g.name }
                                        if count > 0 {
                                            <span class="group-pill-count">{ count }</span>
                                        }
                                    </span>
                                    if is_active {
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
                                                        cdg.set(Some(g_for_delete.clone()));
                                                    })
                                                }
                                            >{ "×" }</button>
                                        </div>
                                    }
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
            </div>

            // ── Main grid ─────────────────────────────────────────────────────
            <main class="main-content">
                if let Some(err) = &*error {
                    <div class="error-banner">{ format!("⚠️ Erreur de connexion API : {}", err) }</div>
                }
                if let Some(ref g) = active_group {
                    <div class="page-banner">
                        <div class="page-banner-title">{ format!("⭐ {}", g.name) }</div>
                        <div class="page-banner-actions">
                            <button class="btn btn-sm btn-ghost" onclick={
                                let rg = rename_group.clone();
                                let g_clone = g.clone();
                                Callback::from(move |_: MouseEvent| rg.set(Some(g_clone.clone())))
                            }>{ "✏ Renommer" }</button>
                            <button class="btn btn-sm btn-ghost btn-ghost-danger" onclick={
                                let cdg = confirm_delete_group.clone();
                                let g_clone = g.clone();
                                Callback::from(move |_: MouseEvent| cdg.set(Some(g_clone.clone())))
                            }>{ "🗑" }</button>
                            <button class="btn btn-sm" onclick={
                                let af = active_filter.clone();
                                Callback::from(move |_: MouseEvent| af.set(None))
                            }>{ "Voir toutes" }</button>
                        </div>
                    </div>
                }
                if *initial_fetch_done && filtered_stacks.is_empty() {
                    <div class="empty-state">
                        <div class="empty-state-icon">{ "📦" }</div>
                        <p class="empty-state-text">
                            {
                                if !query.is_empty() {
                                    format!("Aucune stack ou conteneur ne correspond à « {} ».", *search_query)
                                } else if active_group.is_some() {
                                    "Aucune stack dans ce groupe de favoris. Touchez ☆ sur une stack pour l'ajouter.".to_string()
                                } else {
                                    "Aucune stack trouvée.".to_string()
                                }
                            }
                        </p>
                        if !query.is_empty() || active_group.is_some() {
                            <button class="btn" onclick={
                                let sq = search_query.clone();
                                let af = active_filter.clone();
                                Callback::from(move |_: MouseEvent| {
                                    sq.set(String::new());
                                    af.set(None);
                                })
                            }>{ "Réinitialiser les filtres" }</button>
                        }
                    </div>
                }
                <div class="stacks-grid">
                    {
                        filtered_stacks.iter().enumerate().map(|(idx, stack)| {
                            let is_active = stack.status == 1;
                            let sname = stack.name.clone();
                            let sname_drop = stack.name.clone();
                            let sname_star = stack.name.clone();
                            let sname_up = stack.name.clone();
                            let sname_down = stack.name.clone();
                            let sid = stack.id;

                            let stack_pending_action = pending_stacks.get(&sid).cloned();
                            let is_stack_loading = stack_pending_action.is_some();

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
                            let on_move_up = {
                                let ms = move_stack_step.clone();
                                Callback::from(move |e: MouseEvent| {
                                    e.stop_propagation();
                                    ms.emit((sname_up.clone(), -1));
                                })
                            };
                            let on_move_down = {
                                let ms = move_stack_step.clone();
                                Callback::from(move |e: MouseEvent| {
                                    e.stop_propagation();
                                    ms.emit((sname_down.clone(), 1));
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
                            if is_stack_loading { card_class.push_str(" is-loading"); }

                            let stack_status_badge: Html = if let Some(ref act) = stack_pending_action {
                                let txt = if act == "stop" { "Arrêt..." } else { "Démarrage..." };
                                html! {
                                    <span class="status loading">
                                        <span class="spinner spinner-xs"></span>
                                        { txt }
                                    </span>
                                }
                            } else if is_active {
                                html! { <span class="status active">{ "Active" }</span> }
                            } else {
                                html! { <span class="status stopped">{ "Stoppée" }</span> }
                            };

                            let stack_btn_content: Html = if let Some(ref act) = stack_pending_action {
                                let txt = if act == "stop" { "Arrêt..." } else { "Démarrage..." };
                                html! {
                                    <>
                                        <span class="spinner"></span>
                                        <span>{ txt }</span>
                                    </>
                                }
                            } else if is_active {
                                html! { <>{ "■ Stopper" }</> }
                            } else {
                                html! { <>{ "▶ Démarrer" }</> }
                            };

                            html! {
                                <div class={card_class} draggable="true"
                                     ondragstart={on_drag_start} ondragover={on_drag_over}
                                     ondragleave={on_drag_leave} ondrop={on_drop}>
                                    <div class="stack-header">
                                        <div class="stack-title-group">
                                            <button
                                                class={if in_any_group { "star-btn star-btn--on" } else { "star-btn" }}
                                                onclick={on_star}
                                                title="Gérer les favoris"
                                            >{ if in_any_group { "★" } else { "☆" } }</button>
                                            <div class="stack-meta">
                                                <h3 class="stack-name" title={stack.name.clone()}>{ &stack.name }</h3>
                                                <div class="stack-submeta">
                                                    { stack_status_badge }
                                                    <span class="stack-count">
                                                        { format!("{}/{} conteneurs", running_count, stack_containers.len()) }
                                                    </span>
                                                </div>
                                            </div>
                                        </div>
                                        <div class="stack-header-actions">
                                            if can_reorder_touch && filtered_len > 1 {
                                                <div class="reorder-btns">
                                                    <button
                                                        class="reorder-btn"
                                                        disabled={idx == 0}
                                                        onclick={on_move_up}
                                                        title="Monter"
                                                    >{ "↑" }</button>
                                                    <button
                                                        class="reorder-btn"
                                                        disabled={idx + 1 >= filtered_len}
                                                        onclick={on_move_down}
                                                        title="Descendre"
                                                    >{ "↓" }</button>
                                                </div>
                                            }
                                            <button
                                                class={if is_stack_loading {
                                                    "btn btn-stack-toggle is-loading"
                                                } else if is_active {
                                                    "btn btn-danger btn-stack-toggle"
                                                } else {
                                                    "btn btn-stack-toggle"
                                                }}
                                                disabled={is_stack_loading}
                                                onclick={on_toggle}
                                            >
                                                { stack_btn_content }
                                            </button>
                                        </div>
                                    </div>

                                    <div class="containers-list">
                                        {
                                            stack_containers.iter().map(|c| {
                                                let cid   = c.id.clone();
                                                let cname = c.names.get(0).cloned().unwrap_or_default().replace('/', "");
                                                let running = c.state == "running";

                                                let container_pending = pending_containers.get(&cid).cloned();
                                                let is_container_loading = container_pending.is_some();

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

                                                let pending_label: Html = if let Some(ref act) = container_pending {
                                                    let word = match act.as_str() {
                                                        "start" => "Démarrage...",
                                                        "stop" => "Arrêt...",
                                                        _ => "Relance...",
                                                    };
                                                    html! {
                                                        <span class="container-pending-badge">
                                                            <span class="spinner spinner-xs"></span>
                                                            { word }
                                                        </span>
                                                    }
                                                } else {
                                                    html! {}
                                                };

                                                html! {
                                                    <div class={if is_container_loading { "container-item is-loading" } else { "container-item" }}>
                                                        <div class="container-row">
                                                            <div class="container-info" title={cname.clone()}>
                                                                <span class="container-dot">
                                                                    { if is_container_loading { "⏳" } else if running { "🟢" } else { "🔴" } }
                                                                </span>
                                                                <span class={if running { "container-name running" } else { "container-name stopped" }}>
                                                                    { &cname }
                                                                </span>
                                                                { pending_label }
                                                            </div>
                                                            <div class="container-actions">
                                                                if is_container_loading {
                                                                    <button class="btn btn-icon-action is-loading" disabled=true>
                                                                        <span class="spinner"></span>
                                                                    </button>
                                                                } else if !running {
                                                                    <button class="btn btn-icon-action" title="Démarrer" disabled={is_stack_loading} onclick={on_start}>{ "▶" }</button>
                                                                } else {
                                                                    <button class="btn btn-danger btn-icon-action" title="Arrêter" disabled={is_stack_loading} onclick={on_stop}>{ "■" }</button>
                                                                    <button class="btn btn-icon-action" title="Redémarrer" disabled={is_stack_loading} onclick={on_restart}>{ "↻" }</button>
                                                                }
                                                            </div>
                                                        </div>
                                                        if !urls.is_empty() || !pub_ports.is_empty() {
                                                            <div class="container-links">
                                                                { for urls.iter().map(|url| html! {
                                                                    <a href={url.clone()} target="_blank" rel="noopener noreferrer" class="link-chip link-chip--url">
                                                                        <span>{"🔗"}</span>
                                                                        <span>{url.replace("http://","").replace("https://","")}</span>
                                                                    </a>
                                                                }) }
                                                                { for pub_ports.iter().map(|port| {
                                                                    let host = if !sip.is_empty() { sip.clone() } else {
                                                                        web_sys::window().and_then(|w| w.location().hostname().ok()).unwrap_or_else(|| "localhost".to_string())
                                                                    };
                                                                    html! {
                                                                        <a href={format!("http://{}:{}", host, port)} target="_blank" rel="noopener noreferrer" class="link-chip link-chip--port">
                                                                            <span>{"🔌"}</span>
                                                                            <span>{port}</span>
                                                                        </a>
                                                                    }
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
                        let id = n.id;
                        let d = dismiss_notification.clone();
                        let toast_class = match n.kind {
                            NotificationKind::Loading => "toast toast-loading",
                            NotificationKind::Success => "toast toast-success",
                            NotificationKind::Error   => "toast toast-error",
                        };
                        let icon_html: Html = match n.kind {
                            NotificationKind::Loading => html! { <span class="spinner toast-spinner"></span> },
                            NotificationKind::Success => html! { <span class="toast-icon">{ "✓" }</span> },
                            NotificationKind::Error   => html! { <span class="toast-icon">{ "✕" }</span> },
                        };
                        html! {
                            <div class={toast_class}>
                                { icon_html }
                                <span class="toast-message">{ &n.message }</span>
                                <button class="toast-close" onclick={Callback::from(move |_| d.emit(id))}>{ "×" }</button>
                            </div>
                        }
                    }).collect::<Html>()
                }
            </div>

            // ── Settings modal (sort order) ───────────────────────────────────
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

            // ── Rename group modal ────────────────────────────────────────────
            { rename_group_frag }

            // ── Delete group confirmation modal ───────────────────────────────
            { confirm_delete_group_frag }
        </div>
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
    let on_close_bg = props.on_close.clone();
    let on_close_btn = props.on_close.clone();
    let cb = props.on_save.clone();
    html! {
        <div class="modal-overlay" onclick={move |_| on_close_bg.emit(())}>
            <div class="modal" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                <h3>{ "Paramètres" }</h3>
                <div class="modal-field">
                    <label class="modal-label">{ "Ordre d'affichage :" }</label>
                    <select class="modal-select" value={(*sort).clone()} onchange={
                        let sort = sort.clone();
                        Callback::from(move |e: Event| {
                            let t: web_sys::HtmlSelectElement = e.target_unchecked_into();
                            sort.set(t.value());
                        })
                    }>
                        <option value="custom">{ "Personnalisé (Glisser / ↑↓)" }</option>
                        <option value="default">{ "Par défaut" }</option>
                        <option value="alpha">{ "Alphabétique (A-Z)" }</option>
                        <option value="status">{ "Statut (Actives en premier)" }</option>
                    </select>
                </div>
                <div class="modal-actions">
                    <button class="btn btn-secondary" onclick={move |_| on_close_btn.emit(())}>{ "Annuler" }</button>
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
    let on_close_bg = props.on_close.clone();
    let on_close_btn = props.on_close.clone();
    let cb = props.on_save.clone();
    html! {
        <div class="modal-overlay" onclick={move |_| on_close_bg.emit(())}>
            <div class="modal" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                <h3>{ &props.title }</h3>
                <input
                    class="modal-input"
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
                />
                <div class="modal-actions">
                    <button class="btn btn-secondary" onclick={move |_| on_close_btn.emit(())}>{ "Annuler" }</button>
                    <button class="btn" onclick={
                        let name = name.clone();
                        Callback::from(move |_| cb.emit((*name).clone()))
                    }>{ "Enregistrer" }</button>
                </div>
            </div>
        </div>
    }
}

// ── Confirmation modal (used for destructive actions like delete) ──────────────
#[derive(Properties, PartialEq)]
pub struct ConfirmProps {
    pub title: String,
    pub message: String,
    pub on_close: Callback<()>,
    pub on_confirm: Callback<()>,
}

#[function_component(ConfirmModal)]
fn confirm_modal(props: &ConfirmProps) -> Html {
    let on_close_bg = props.on_close.clone();
    let on_close_btn = props.on_close.clone();
    let on_confirm = props.on_confirm.clone();
    html! {
        <div class="modal-overlay" onclick={move |_| on_close_bg.emit(())}>
            <div class="modal" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                <h3>{ &props.title }</h3>
                <p class="modal-message">{ &props.message }</p>
                <div class="modal-actions">
                    <button class="btn btn-secondary" onclick={move |_| on_close_btn.emit(())}>{ "Annuler" }</button>
                    <button class="btn btn-danger" onclick={move |_| on_confirm.emit(())}>{ "Supprimer" }</button>
                </div>
            </div>
        </div>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
