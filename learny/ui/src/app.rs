use dioxus::prelude::*;
use dioxus::document::{Script, Stylesheet};
use crate::components::{ DeckList, CardView, CardListPage, CardEditorEdit, CardEditorNew, KnowledgeMapPage, TabStrip, TabInstance, FindBar };
use crate::api::{logout, UserInfo};
// Login flow only exists in the server build; the Tauri app has no auth.
#[cfg(feature = "server")]
use crate::components::LoginPage;
#[cfg(feature = "server")]
use crate::api::get_current_user;


/// Cross-tab handoff: when a new tab is opened programmatically (e.g. clicking a
/// tag in the card view), its router starts at this route instead of the default
/// home page. Consumed once by the freshly-mounted `TabInstance`.
pub static PENDING_TAB_ROUTE: GlobalSignal<Option<Route>> = Signal::global(|| None);

/// Tag names to pre-fill on the next "new card" editor (set when creating a
/// card from a tag panel on the knowledge map). Consumed once on mount.
pub static PENDING_NEW_CARD_TAGS: GlobalSignal<Vec<String>> = Signal::global(Vec::new);

/// Navigation requested from the global navbar, which lives outside every tab's
/// router and so can't use `Link`/`navigator()` itself. Holds the target tab id
/// and route; the matching tab's `AppLayout` consumes it and navigates.
pub static PENDING_NAV: GlobalSignal<Option<(usize, Route)>> = Signal::global(|| None);


/// UI theme. Global because every tab renders its own navbar toggle and they
/// must stay in sync. Mirrored onto `<html data-theme>` (see `AppShell`) and
/// persisted to localStorage on toggle.
pub static THEME: GlobalSignal<Theme> = Signal::global(|| Theme::Dark);

#[derive(Clone, Copy, PartialEq)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    fn name(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }
}

/// Auto-derived tab titles keyed by tab id — reflects the content each tab shows.
/// Pages with data (card view, card list) refine these; everything else uses the
/// default from `route_title`.
pub static TAB_TITLES: GlobalSignal<std::collections::HashMap<usize, String>> =
    Signal::global(std::collections::HashMap::new);

/// The tab id a routed page belongs to, provided by its `TabInstance` so pages
/// can report their own title.
#[derive(Clone, Copy)]
pub struct TabId(pub usize);

/// Default tab label for a route. Data-rich pages overwrite this once loaded
/// (card view → card title, card list → deck name).
fn route_title(route: &Route) -> String {
    match route {
        Route::DeckList => "Home",
        Route::CardListPage { .. } => "Deck",
        Route::CardView { .. } => "Card",
        Route::CardEditorEdit { .. } => "Edit card",
        Route::CardEditorNew { .. } => "New card",
        Route::KnowledgeMapPage => "Map",
    }
    .to_string()
}

/// Tab list state shared down the tree so any page can open a new tab.
#[derive(Clone, Copy)]
pub struct TabsCtx {
    pub tabs: Signal<Vec<usize>>,
    pub active: Signal<usize>,
    pub next_id: Signal<usize>,
}

impl TabsCtx {
    /// Open a brand-new tab whose router starts at `route`, and focus it.
    pub fn open(mut self, route: Route) {
        *PENDING_TAB_ROUTE.write() = Some(route);
        let id = *self.next_id.read();
        self.next_id.set(id + 1);
        self.tabs.write().push(id);
        self.active.set(id);
    }
}

#[derive(Clone, Debug, PartialEq, Routable)]
pub enum Route {
    #[layout(AppLayout)]
    #[route("/")]
    DeckList,

    #[route("/deck/:id")]
    CardListPage { id: i64 },

    #[route("/card/:id")]
    CardView { id: i64 },

    #[route("/card/:id/edit")]
    CardEditorEdit { id: i64 },

    #[route("/card/new/:deck_id")]
    CardEditorNew { deck_id: i64 },

    #[route("/knowledge-map")]
    KnowledgeMapPage,

}

static CSS: Asset = asset!("/assets/global.css");

// ---------------------------------------------------------------------------
// Root app — auth-gated on server, direct on Tauri
// ---------------------------------------------------------------------------

#[component]
pub fn App() -> Element {
    // On Tauri, skip auth entirely.
    #[cfg(feature = "tauri")]
    return rsx! {
        AppShell {
            user: UserInfo {
                id: 1.to_string(),
                username: "local".into(),
            },
        }
    };

    // On web, check the session before rendering anything.
    #[cfg(feature = "server")]
    rsx! {
        AuthGate {}
    }
}

// ---------------------------------------------------------------------------
// Auth gate (server feature only)
// ---------------------------------------------------------------------------

#[cfg(feature = "server")]
#[component]
fn AuthGate() -> Element {
    let mut user: Signal<Option<UserInfo>> = use_signal(|| None);
    let mut checking = use_signal(|| true);

    use_future(move || async move {
        user.set(get_current_user().await);
        checking.set(false);
    });

    if *checking.read() {
        return rsx! {
            Stylesheet { href: CSS }
            div { class: "loading", "Loading…" }
        };
    }

    match user.read().clone() {
        None => rsx! {
            Stylesheet { href: CSS }
            LoginPage { on_login: move |u: UserInfo| user.set(Some(u)) }
        },
        Some(u) => rsx! {
            AppShell { user: u, on_logout: move |_| user.set(None) }
        },
    }
}

// ---------------------------------------------------------------------------
// Main shell — shown once authenticated
// ---------------------------------------------------------------------------

/// Shared shell state made available to the routed layout (which lives inside
/// the Router and so can't receive these as props).
#[derive(Clone)]
struct ShellCtx {
    user: UserInfo,
    on_logout: EventHandler<()>,
}

#[component]
fn AppShell(
    user: UserInfo,
    #[props(default)]
    on_logout: EventHandler<()>,
) -> Element {
    use_context_provider(|| ShellCtx { user: user.clone(), on_logout });

    // Browser-style tabs: each is a full, independent instance of the app with
    // its own in-memory router. `tabs` holds stable ids (for keying), `active`
    // is the visible one, `next_id` hands out fresh ids so keys never collide.
    let tabs = use_signal(|| vec![0usize]);
    let active = use_signal(|| 0usize);
    let next_id = use_signal(|| 1usize);
    // User-given tab names, keyed by tab id. Missing → falls back to "Tab N".
    let tab_names = use_signal(std::collections::HashMap::<usize, String>::new);

    // Make tab state reachable from any page (e.g. to open related cards in a new tab).
    use_context_provider(|| TabsCtx { tabs, active, next_id });

    // Restore the saved theme once. Only the toggle writes localStorage, so
    // this read can't race with the effect below.
    use_future(|| async {
        if let Ok(v) = dioxus::document::eval("return localStorage.getItem('theme');").await {
            if v.as_str() == Some("light") {
                *THEME.write() = Theme::Light;
            }
        }
    });

    // Mirror the theme onto <html> whenever it changes.
    use_effect(|| {
        let name = THEME.read().name();
        let _ = dioxus::document::eval(&format!(
            "document.documentElement.dataset.theme = '{name}';"
        ));
    });

    rsx! {
        Script { src: asset!("/assets/mathjax-config.js") }
        Script { src: asset!("/assets/mathjax/es5/tex-svg-full.js") }
        Script { src: asset!("/assets/highlight.min.js") }
        Stylesheet { href: asset!("/assets/highlight-ocean.min.css") }
        Stylesheet { href: CSS }

        Navbar { active }

        TabStrip { tabs, active, next_id, tab_names }

        FindBar {}

        div { class: "tab-instances",
            for id in tabs.read().iter().copied() {
                div {
                    key: "{id}",
                    class: "tab-instance",
                    hidden: id != *active.read(),
                    TabInstance { id }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Global navbar — rendered once, above the tab strip. It sits outside every
// tab's router, so its links can't be `Link`s: they hand a route to the active
// tab through `PENDING_NAV` instead.
// ---------------------------------------------------------------------------

#[component]
fn Navbar(active: Signal<usize>) -> Element {
    let ctx = use_context::<ShellCtx>();
    let on_logout = ctx.on_logout;
    let go = move |route: Route| {
        *PENDING_NAV.write() = Some((*active.read(), route));
    };

    rsx! {
        nav { class: "navbar",
            button {
                class: "navbar-brand",
                onclick: move |_| go(Route::DeckList),
                span { class: "navbar-brand-icon", "◌" }
                "Learny"
            }
            div { class: "navbar-end",
                button {
                    class: "icon-btn navbar-icon-btn",
                    title: "Toggle light/dark mode",
                    onclick: move |_| {
                        let next = match *THEME.read() {
                            Theme::Dark => Theme::Light,
                            Theme::Light => Theme::Dark,
                        };
                        *THEME.write() = next;
                        let _ = dioxus::document::eval(&format!(
                            "localStorage.setItem('theme', '{}');",
                            next.name()
                        ));
                    },
                    if *THEME.read() == Theme::Dark { "◐" } else { "◑" }
                }
                if cfg!(feature = "server") {
                    span { class: "navbar-user", "{ctx.user.username}" }
                    button {
                        class: "button button-secondary button-xs",
                        onclick: move |_| {
                            spawn(async move {
                                logout().await;
                                on_logout.call(());
                            });
                        },
                        "Sign out"
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Routed layout — lives inside the Router, so `navigator()`/`use_route()` work
// here. The navbar and tab strip are rendered once by the shell, above this.
// ---------------------------------------------------------------------------

#[component]
fn AppLayout() -> Element {
    // Keep this tab's title in sync with the route it's showing. Data-rich pages
    // (card view, card list) refine it further once their content loads.
    let tab_id = use_context::<TabId>().0;
    let route = use_route::<Route>();
    use_effect(use_reactive((&route,), move |(route,)| {
        TAB_TITLES.write().insert(tab_id, route_title(&route));
    }));

    // Apply a navigation requested by the global navbar, which can't reach this
    // tab's router itself. Only the addressed tab consumes the request.
    use_effect(move || {
        let requested = PENDING_NAV.read().clone();
        if let Some((target, route)) = requested {
            if target == tab_id {
                *PENDING_NAV.write() = None;
                navigator().push(route);
            }
        }
    });

    rsx! {
        Outlet::<Route> {}
    }
}
