use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};

use crate::catalogue::{
    canonical_path_for_game, categories, default_game, game_from_path, is_next_library_path,
    library_games, library_path_for_game, Game, GameArchitecture, GAME_ARCHITECTURES,
};
use crate::components::{
    game_screen::{prefetch_game_archive, GameScreen, UnavailableGameScreen},
    game_thumb::GameThumb,
};
use crate::paths::{asset_path, browser_path_for_route, normalized_path};

const SYSTEMLESS_REPOSITORY: &str = env!("SYSTEMLESS_REPOSITORY");
const SYSTEMLESS_VERSION: &str = env!("SYSTEMLESS_VERSION");
const SYSTEMLESS_GIT_SHA: &str = env!("SYSTEMLESS_GIT_SHA");
const UNKNOWN: &str = "unknown";
const COMPACT_DOCUMENT_TITLE: &str = "Systemless";
const COMPACT_DOCUMENT_TITLE_MEDIA: &str =
    "(max-width: 760px), (display-mode: standalone), (hover: none) and (pointer: coarse)";
const THEME_STORAGE_KEY: &str = "systemless.theme";
const HOMEBREW_INSTALL_COMMAND: &str = "brew install benletchford/tap/systemless";
const CARGO_INSTALL_COMMAND: &str = "cargo install systemless";
const CLI_LAUNCH_COMMAND: &str = "systemless path/to/application.sit";

thread_local! {
    static RUNTIME_NOTICE: std::cell::Cell<Option<RwSignal<String>>> = const { std::cell::Cell::new(None) };
}

pub(crate) fn report_runtime_notice(message: String) {
    RUNTIME_NOTICE.with(|notice| {
        if let Some(notice) = notice.get() {
            let _ = notice.try_set(message);
        }
    });
}

#[component]
fn PixelMacMark() -> impl IntoView {
    view! {
        <svg
            class="pixel-mac"
            viewBox="0 0 16 16"
            shape-rendering="crispEdges"
            role="img"
            aria-label="A smiling classic Macintosh"
        >
            <rect x="4" y="2" width="8" height="1" fill="var(--ink)"/>
            <rect x="3" y="3" width="1" height="8" fill="var(--ink)"/>
            <rect x="12" y="3" width="1" height="8" fill="var(--ink)"/>
            <rect x="4" y="3" width="8" height="8" fill="#c9b183"/>
            <rect x="5" y="4" width="6" height="6" fill="#2794c4"/>
            <rect x="6" y="5" width="1" height="1" fill="#dff6ff"/>
            <rect x="6" y="6" width="1" height="1" fill="var(--pixel-detail)"/>
            <rect x="9" y="6" width="1" height="1" fill="var(--pixel-detail)"/>
            <rect x="6" y="8" width="1" height="1" fill="var(--pixel-detail)"/>
            <rect x="9" y="8" width="1" height="1" fill="var(--pixel-detail)"/>
            <rect x="7" y="9" width="2" height="1" fill="var(--pixel-detail)"/>
            <rect x="4" y="11" width="8" height="1" fill="var(--ink)"/>
            <rect x="6" y="12" width="4" height="1" fill="var(--ink)"/>
            <rect x="7" y="12" width="2" height="1" fill="#c9b183"/>
            <rect x="5" y="13" width="6" height="1" fill="var(--ink)"/>
        </svg>
    }
}

#[component]
fn SiteHeader(
    set_view: WriteSignal<&'static str>,
    dark_theme: ReadSignal<bool>,
    set_dark_theme: WriteSignal<bool>,
) -> impl IntoView {
    let star_count = RwSignal::new(None::<String>);
    Effect::new(move |_| {
        spawn_local(async move {
            if let Some(count) = fetch_github_star_count().await {
                star_count.set(Some(count));
            }
        });
    });

    view! {
        <header class="site-header">
            <div class="site-header__inner">
                <a
                    class="wordmark"
                    href="/"
                    on:click=move |ev| {
                        ev.prevent_default();
                        navigate_to_library(set_view);
                    }
                >
                    "systemless.org"
                </a>
                <nav class="site-header__nav" aria-label="Site navigation">
                    <button
                        class="theme-toggle"
                        type="button"
                        aria-label=move || if dark_theme.get() {
                            "Switch to light theme"
                        } else {
                            "Switch to dark theme"
                        }
                        on:click=move |_| set_dark_theme.update(|dark| *dark = !*dark)
                    >
                        {move || if dark_theme.get() { "Light" } else { "Dark" }}
                    </button>
                    <a
                        class="github-link"
                        href=SYSTEMLESS_REPOSITORY
                        target="_blank"
                        rel="noopener noreferrer"
                        aria-label=move || star_count
                            .get()
                            .map(|count| format!("Systemless on GitHub, {count} stars"))
                            .unwrap_or_else(|| "Star Systemless on GitHub".to_string())
                    >
                        <span class="github-link__action">
                            <svg
                                class="github-link__star"
                                viewBox="0 0 16 16"
                                width="16"
                                height="16"
                                aria-hidden="true"
                                focusable="false"
                            >
                                <path d="M8 .25a.75.75 0 0 1 .673.418l1.882 3.815 4.21.612a.75.75 0 0 1 .416 1.279l-3.046 2.97.719 4.192a.751.751 0 0 1-1.088.791L8 12.347l-3.766 1.98a.75.75 0 0 1-1.088-.79l.72-4.194L.818 6.374a.75.75 0 0 1 .416-1.28l4.21-.611L7.327.668A.75.75 0 0 1 8 .25Zm0 2.445L6.615 5.5a.75.75 0 0 1-.564.41l-3.097.45 2.24 2.184a.75.75 0 0 1 .216.664l-.528 3.084 2.769-1.456a.75.75 0 0 1 .698 0l2.77 1.456-.53-3.084a.75.75 0 0 1 .216-.664l2.24-2.183-3.096-.45a.75.75 0 0 1-.564-.41L8 2.694Z" />
                            </svg>
                            <span>
                                "Star"
                                <span class="github-link__on-github">" on GitHub"</span>
                            </span>
                        </span>
                        <span class="github-link__count">
                            {move || star_count.get().unwrap_or_else(|| "—".to_string())}
                        </span>
                    </a>
                </nav>
            </div>
        </header>
    }
}

#[derive(serde::Deserialize)]
struct GithubStarBadge {
    message: String,
}

async fn fetch_github_star_count() -> Option<String> {
    let repository = SYSTEMLESS_REPOSITORY
        .strip_prefix("https://github.com/")?
        .trim_end_matches('/');
    let url = format!("https://img.shields.io/github/stars/{repository}.json");
    let response = gloo_net::http::Request::get(&url).send().await.ok()?;
    if !response.ok() {
        return None;
    }
    let body = response.text().await.ok()?;
    let badge = serde_json::from_str::<GithubStarBadge>(&body).ok()?;
    (badge.message != "unknown").then_some(badge.message)
}

#[component]
fn LibraryPage(
    show_unapproved: bool,
    set_active_game: WriteSignal<&'static Game>,
    set_view: WriteSignal<&'static str>,
) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let architecture = RwSignal::new(None::<GameArchitecture>);
    let category = RwSignal::new(None::<&'static str>);
    let install_on_macos = RwSignal::new(true);
    let total = library_games(show_unapproved).count();

    view! {
        <div class="library">
            <section class="library-hero">
                <div class="library-hero__content">
                    <PixelMacMark/>
                    <div class="library-hero__copy">
                        {show_unapproved.then(|| view! {
                            <span class="eyebrow">"Preview library"</span>
                        })}
                        <h1>
                            "Classic 68K and PowerPC Macintosh games, running everywhere "
                            <em>"with no ROM and no System"</em>
                            "."
                        </h1>
                        <p>"Play in your browser now. Nothing to install."</p>
                    </div>
                </div>
            </section>

            <section class="cli-hint" aria-labelledby="cli-hint-title">
                <div class="cli-hint__intro">
                    <span class="eyebrow">"Command line"</span>
                    <h2 id="cli-hint-title">"Run your own application"</h2>
                    <p>"Install the Systemless CLI, then launch a classic 68K or PowerPC Macintosh application from its archive."</p>
                </div>
                <div class="cli-hint__setup">
                    <div class="cli-platform" role="group" aria-label="Choose an installation method">
                        <button
                            type="button"
                            class=move || if install_on_macos.get() {
                                "cli-platform__option cli-platform__option--active"
                            } else {
                                "cli-platform__option"
                            }
                            aria-pressed=move || bool_attr(install_on_macos.get())
                            on:click=move |_| install_on_macos.set(true)
                        >
                            "macOS"
                        </button>
                        <button
                            type="button"
                            class=move || if install_on_macos.get() {
                                "cli-platform__option"
                            } else {
                                "cli-platform__option cli-platform__option--active"
                            }
                            aria-pressed=move || bool_attr(!install_on_macos.get())
                            on:click=move |_| install_on_macos.set(false)
                        >
                            "Other"
                        </button>
                    </div>
                    <div class="cli-command">
                        <span>"Install"</span>
                        <code>{move || if install_on_macos.get() {
                            HOMEBREW_INSTALL_COMMAND
                        } else {
                            CARGO_INSTALL_COMMAND
                        }}</code>
                    </div>
                    <div class="cli-command">
                        <span>"Launch"</span>
                        <code>{CLI_LAUNCH_COMMAND}</code>
                    </div>
                </div>
            </section>

            <section class="library-catalogue" aria-label="Game library">
                <div class="library-tools">
                    <label class="library-search">
                        <span class="library-search__label">"Find"</span>
                        <input
                            type="search"
                            placeholder=format!("Search {total} titles")
                            aria-label="Search games"
                            on:input=move |ev| query.set(event_target_value(&ev))
                        />
                        <span class="library-search__count">
                            {move || {
                                let count = filtered_library_games(
                                    show_unapproved,
                                    &query.get(),
                                    architecture.get(),
                                    category.get(),
                                ).len();
                                format!("{count} {}", if count == 1 { "title" } else { "titles" })
                            }}
                        </span>
                    </label>
                    <div class="library-filters" aria-label="Filter games by tags">
                        <div class="library-filter-group" role="group" aria-label="System">
                            <span class="library-filter-group__label">"System"</span>
                            <div class="library-filter-group__tags">
                                {GAME_ARCHITECTURES.iter().copied().map(|option| view! {
                                    <button
                                        type="button"
                                        class=move || filter_tag_class(architecture.get() == Some(option))
                                        aria-pressed=move || bool_attr(architecture.get() == Some(option))
                                        on:click=move |_| architecture.update(|selected| {
                                            *selected = if *selected == Some(option) { None } else { Some(option) };
                                        })
                                    >
                                        {option.label()}
                                    </button>
                                }).collect_view()}
                            </div>
                        </div>
                        <div class="library-filter-group" role="group" aria-label="Genre">
                            <span class="library-filter-group__label">"Genre"</span>
                            <div class="library-filter-group__tags">
                                {categories(show_unapproved).iter().copied().map(|option| view! {
                                    <button
                                        type="button"
                                        class=move || filter_tag_class(category.get() == Some(option))
                                        aria-pressed=move || bool_attr(category.get() == Some(option))
                                        on:click=move |_| category.update(|selected| {
                                            *selected = if *selected == Some(option) { None } else { Some(option) };
                                        })
                                    >
                                        {option}
                                    </button>
                                }).collect_view()}
                            </div>
                        </div>
                        <button
                            type="button"
                            class=move || if architecture.get().is_some() || category.get().is_some() {
                                "library-filter-clear library-filter-clear--visible"
                            } else {
                                "library-filter-clear"
                            }
                            disabled=move || architecture.get().is_none() && category.get().is_none()
                            on:click=move |_| {
                                architecture.set(None);
                                category.set(None);
                            }
                        >
                            "Clear"
                        </button>
                    </div>
                </div>
                <div class="library-list">
                    {move || {
                        let games = filtered_library_games(
                            show_unapproved,
                            &query.get(),
                            architecture.get(),
                            category.get(),
                        );
                        if games.is_empty() {
                            let message = if query.get().trim().is_empty() {
                                "No titles match those tags."
                            } else if architecture.get().is_none() && category.get().is_none() {
                                "No titles match that search."
                            } else {
                                "No titles match that search and those tags."
                            };
                            view! {
                                <p class="library-empty">{message}</p>
                            }.into_any()
                        } else {
                            games.into_iter().map(|game| {
                                view! {
                                    <LibraryGameCard
                                        game=game
                                        show_unapproved=show_unapproved
                                        set_active_game=set_active_game
                                        set_view=set_view
                                    />
                                }
                            }).collect_view().into_any()
                        }
                    }}
                </div>
            </section>
        </div>
    }
}

#[component]
fn LibraryGameCard(
    game: &'static Game,
    show_unapproved: bool,
    set_active_game: WriteSignal<&'static Game>,
    set_view: WriteSignal<&'static str>,
) -> impl IntoView {
    let game_path = library_path_for_game(game, show_unapproved);
    let href = browser_path_for_route(&game_path, "/");

    view! {
        <a
            class="library-card"
            href=href
            aria-label=format!("{} {}", if game.approved { "Play" } else { "View" }, game.title)
            on:pointerenter=move |_| prefetch_game_archive(game)
            on:focus=move |_| prefetch_game_archive(game)
            on:touchstart=move |_| prefetch_game_archive(game)
            on:click=move |ev| {
                ev.prevent_default();
                if game.approved {
                    crate::emulator::begin_audio_from_user_gesture();
                }
                set_active_game.set(game);
                set_view.set("playing");
                push_route(&game_path);
                set_document_head_for_game(game);
            }
        >
            <span class="library-card__thumb"><GameThumb game=game/></span>
            <span class="library-card__title">{game.title}</span>
            <span class="library-card__tags" aria-label="Game tags">
                {game.architectures.iter().copied().map(|architecture| view! {
                    <span class="game-tag game-tag--architecture">{architecture.label()}</span>
                }).collect_view()}
                <span class="game-tag">{game.category}</span>
            </span>
            <span class="library-card__description">{game.description}</span>
            <span class="library-card__arrow" aria-hidden="true">"→"</span>
        </a>
    }
}

fn filtered_library_games(
    show_unapproved: bool,
    query: &str,
    architecture: Option<GameArchitecture>,
    category: Option<&str>,
) -> Vec<&'static Game> {
    let query = query.trim().to_lowercase();
    library_games(show_unapproved)
        .filter(|game| {
            (query.is_empty()
                || game.title.to_lowercase().contains(&query)
                || game.developer.to_lowercase().contains(&query)
                || game.year.contains(&query)
                || game.description.to_lowercase().contains(&query)
                || game.architectures.iter().any(|architecture| {
                    architecture.label().to_lowercase().contains(&query)
                        || architecture.key().contains(&query)
                })
                || game.category.to_lowercase().contains(&query))
                && architecture.is_none_or(|architecture| game.supports_architecture(architecture))
                && category.is_none_or(|category| game.category == category)
        })
        .collect()
}

fn filter_tag_class(active: bool) -> &'static str {
    if active {
        "library-filter-tag library-filter-tag--active"
    } else {
        "library-filter-tag"
    }
}

fn bool_attr(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

#[component]
fn GamePage(
    active_game: ReadSignal<&'static Game>,
    set_active_game: WriteSignal<&'static Game>,
    set_view: WriteSignal<&'static str>,
) -> impl IntoView {
    let related_games = Memo::new(move |_| related_games(active_game.get().id, RELATED_GAME_COUNT));

    let focus_game = RwSignal::new(active_game.get_untracked().approved);
    Effect::new(move |_| {
        focus_game.set(active_game.get().approved);
    });
    view! {
        <div class="game-page" class:game-page--focused=move || focus_game.get()>
            <Show when=move || active_game.get().approved>
                <button class="game-focus-toggle" type="button" aria-pressed=move || focus_game.get().to_string()
                    on:click=move |_| focus_game.update(|focus| *focus = !*focus)>
                    {move || if focus_game.get() { "Show game information" } else { "Focus game" }}
                </button>
            </Show>
            <a
                class="back"
                href="/"
                on:click=move |ev| {
                    ev.prevent_default();
                    navigate_to_library(set_view);
                }
            >
                "\u{2190} Library"
            </a>
            <header class="game-heading">
                <h1 class="game-title">{move || active_game.get().title}</h1>
                <div class="game-heading__meta">
                    <p class="game-byline">
                        {move || {
                            let game = active_game.get();
                            format!("{} · {}", game.developer, game.year)
                        }}
                    </p>
                    <a
                        class="game-download"
                        href=move || asset_path(active_game.get().assets.archive_path)
                        target="_blank"
                        rel="noopener noreferrer"
                        download=move || active_game.get().assets.archive_download_name
                        aria-label=move || format!("Download {}", active_game.get().title)
                    >
                        "Download game"
                    </a>
                </div>
            </header>
            <div class="game-screen-wrap">
                <For
                    each=move || vec![active_game.get()]
                    key=game_screen_instance_key
                    let:game
                >
                    <Show when=move || game.approved fallback=move || view! { <UnavailableGameScreen game=game/> }>
                        <GameScreen game=game/>
                    </Show>
                </For>
            </div>
            <section class="related-games">
                <h2>"Also in the library"</h2>
                <div class="related-games__grid">
                    {move || {
                        related_games
                            .get()
                            .into_iter()
                            .map(|game| view! {
                                <RelatedGameCard
                                    game=game
                                    set_active_game=set_active_game
                                    set_view=set_view
                                />
                            })
                            .collect_view()
                    }}
                </div>
            </section>
        </div>
    }
}

#[component]
fn RelatedGameCard(
    game: &'static Game,
    set_active_game: WriteSignal<&'static Game>,
    set_view: WriteSignal<&'static str>,
) -> impl IntoView {
    let game_path = library_path_for_game(game, false);
    let href = browser_path_for_route(&game_path, "/");

    view! {
        <a
            class="related-card"
            href=href
            on:pointerenter=move |_| prefetch_game_archive(game)
            on:focus=move |_| prefetch_game_archive(game)
            on:click=move |ev| {
                ev.prevent_default();
                if game.approved {
                    crate::emulator::begin_audio_from_user_gesture();
                }
                set_active_game.set(game);
                set_view.set("playing");
                push_route(&game_path);
                set_document_head_for_game(game);
            }
        >
            <span class="related-card__thumb"><GameThumb game=game/></span>
            <span>{game.title}</span>
        </a>
    }
}

const RELATED_GAME_COUNT: usize = 4;

fn related_games(active_id: &str, limit: usize) -> Vec<&'static Game> {
    library_games(false)
        .filter(|game| game.id != active_id)
        .take(limit)
        .collect()
}

fn game_screen_instance_key(game: &&'static Game) -> &'static str {
    game.id
}

#[component]
fn SiteFooter(set_view: WriteSignal<&'static str>) -> impl IntoView {
    view! {
        <footer class="site-footer">
            <div class="site-footer__main">
                <div class="site-footer__brand">
                    <span>"Systemless"</span>
                    <p>"ROM-free classic Mac runtime with 68K and PowerPC support."</p>
                </div>
                <nav class="site-footer__nav" aria-label="Footer navigation">
                    <a
                        href="/"
                        on:click=move |ev| {
                            ev.prevent_default();
                            navigate_to_library(set_view);
                        }
                    >
                        "Library"
                    </a>
                    <a href=SYSTEMLESS_REPOSITORY target="_blank" rel="noopener noreferrer">
                        "GitHub"
                    </a>
                </nav>
            </div>
            <div class="site-footer__legal">
                <p class="site-footer__disclaimer">
                    "Game titles, artwork, trademarks, and game content remain the property of their respective rights holders. This project is unaffiliated with those rights holders. Rights holders who want material removed should contact the project maintainers. This site is provided as a preservation effort for historical reference, with no infringement intended."
                </p>
                <div class="powered-by" aria-label="Build information">
                    <span class="powered-by__label">"Powered by"</span>
                    <BuildCrate
                        name="systemless"
                        version=SYSTEMLESS_VERSION
                        sha=SYSTEMLESS_GIT_SHA
                        repository=SYSTEMLESS_REPOSITORY
                    />
                </div>
            </div>
        </footer>
    }
}

#[component]
fn BuildCrate(
    name: &'static str,
    version: &'static str,
    sha: &'static str,
    repository: &'static str,
) -> impl IntoView {
    let sha_view = if let Some(href) = commit_url(repository, sha) {
        view! {
            <a class="build-crate__sha" href=href target="_blank" rel="noreferrer">
                {short_sha(sha)}
            </a>
        }
        .into_any()
    } else {
        view! {
            <span class="build-crate__sha build-crate__sha--unknown">
                {short_sha(sha)}
            </span>
        }
        .into_any()
    };

    view! {
        <span class="build-crate">
            <a class="build-crate__name" href=repository target="_blank" rel="noreferrer">
                {name}
            </a>
            <span class="build-crate__version">{format!("v{version}")}</span>
            {sha_view}
        </span>
    }
}

#[component]
pub fn App() -> impl IntoView {
    let runtime_notice = RwSignal::new(String::new());
    RUNTIME_NOTICE.with(|notice| notice.set(Some(runtime_notice)));
    on_cleanup(|| RUNTIME_NOTICE.with(|notice| notice.set(None)));
    let initial_path = current_path();
    let initial_game = game_from_path(&initial_path);
    let (active_game, set_active_game) = signal(initial_game.unwrap_or_else(default_game));
    let (dark_theme, set_dark_theme) = signal(initial_dark_theme());
    let (active_view, set_active_view) = signal(if initial_game.is_some() {
        "playing"
    } else if is_next_library_path(&initial_path) {
        "next"
    } else {
        "library"
    });

    if let Some(game) = initial_game {
        set_document_head_for_game(game);
    } else if is_next_library_path(&initial_path) {
        set_document_head_for_next();
    } else {
        set_document_head_for_home();
    }
    install_popstate_handler(set_active_game, set_active_view);
    Effect::new(move |_| apply_theme(dark_theme.get()));

    view! {
        <div class="app" data-view=move || active_view.get()>
            <SiteHeader
                set_view=set_active_view
                dark_theme=dark_theme
                set_dark_theme=set_dark_theme
            />
            <Show when=move || !runtime_notice.get().is_empty()>
                <div class="runtime-notice" role="alert">
                    <span>{move || runtime_notice.get()}</span>
                    <button type="button" on:click=move |_| runtime_notice.set(String::new())>"Dismiss"</button>
                </div>
            </Show>
            <main class="app__main">
                {move || match active_view.get() {
                    "playing" => view! {
                        <GamePage
                            active_game=active_game
                            set_active_game=set_active_game
                            set_view=set_active_view
                        />
                    }.into_any(),
                    "next" => view! {
                        <LibraryPage show_unapproved=true set_active_game=set_active_game set_view=set_active_view/>
                    }.into_any(),
                    _ => view! {
                        <LibraryPage show_unapproved=false set_active_game=set_active_game set_view=set_active_view/>
                    }.into_any(),
                }}
            </main>
            <SiteFooter set_view=set_active_view/>
        </div>
    }
}

fn navigate_to_library(set_view: WriteSignal<&'static str>) {
    set_view.set("library");
    push_route("/");
    set_document_head_for_home();
}

fn initial_dark_theme() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    if let Ok(Some(storage)) = window.local_storage() {
        if let Ok(Some(theme)) = storage.get_item(THEME_STORAGE_KEY) {
            return theme == "dark";
        }
    }
    window
        .match_media("(prefers-color-scheme: dark)")
        .ok()
        .flatten()
        .map(|query| query.matches())
        .unwrap_or(false)
}

fn apply_theme(dark: bool) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    if let Some(root) = document.document_element() {
        let _ = root.set_attribute("data-theme", if dark { "dark" } else { "light" });
    }
    if let Ok(Some(storage)) = window.local_storage() {
        let _ = storage.set_item(THEME_STORAGE_KEY, if dark { "dark" } else { "light" });
    }
    if let Ok(Some(theme_color)) = document.query_selector("meta[name='theme-color']") {
        let _ = theme_color.set_attribute("content", if dark { "#131211" } else { "#f2efe9" });
    }
}

fn short_sha(sha: &str) -> String {
    if sha == UNKNOWN {
        UNKNOWN.to_string()
    } else {
        sha.chars().take(12).collect()
    }
}

fn commit_url(repository: &str, sha: &str) -> Option<String> {
    if sha == UNKNOWN || !is_git_sha(sha) {
        return None;
    }
    Some(format!("{}/commit/{sha}", repository.trim_end_matches('/')))
}

fn is_git_sha(value: &str) -> bool {
    (7..=40).contains(&value.len()) && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn install_popstate_handler(
    set_active_game: WriteSignal<&'static Game>,
    set_view: WriteSignal<&'static str>,
) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let on_pop = Closure::wrap(Box::new(move |_ev: web_sys::Event| {
        let path = current_path();
        if let Some(game_id) = game_from_path(&path) {
            set_active_game.set(game_id);
            set_view.set("playing");
            set_document_head_for_game(game_id);
        } else if is_next_library_path(&path) {
            set_view.set("next");
            set_document_head_for_next();
        } else {
            set_view.set("library");
            set_document_head_for_home();
        }
    }) as Box<dyn FnMut(web_sys::Event)>);

    let _ = window.add_event_listener_with_callback("popstate", on_pop.as_ref().unchecked_ref());
    on_pop.forget();
}

fn current_path() -> String {
    web_sys::window()
        .and_then(|window| window.location().pathname().ok())
        .unwrap_or_else(|| "/".to_string())
}

fn push_route(path: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let current_path = window
        .location()
        .pathname()
        .unwrap_or_else(|_| "/".to_string());
    let current = normalized_path(&current_path);
    let next = normalized_path(path);
    if current == next {
        return;
    }
    if let Ok(history) = window.history() {
        let next_url = browser_path_for_route(path, &current_path);
        let _ = history.push_state_with_url(&JsValue::NULL, "", Some(&next_url));
    }
}

fn set_document_head_for_home() {
    let description = "Play classic 68K and PowerPC Macintosh games in the browser with Systemless, a ROM-free classic Mac runtime for playable preservation.";
    let canonical = absolute_url("/");
    let image = absolute_url("/assets/icons/favicon.svg");
    set_document_head(HeadMeta {
        title: "Systemless | Play classic 68K and PowerPC Mac games in your browser".to_string(),
        description: description.to_string(),
        robots: "index,follow",
        canonical_url: canonical.clone(),
        image_url: image,
        schema_json: Some(format!(
            "{{\"@context\":\"https://schema.org\",\"@type\":\"WebSite\",\"name\":\"Systemless\",\"alternateName\":\"systemless.org\",\"url\":{}}}",
            json_string(&canonical)
        )),
    });
}

fn set_document_head_for_next() {
    let description = "Play classic 68K and PowerPC Macintosh games in the browser with Systemless, a ROM-free classic Mac runtime for playable preservation.";
    let canonical = absolute_url(crate::catalogue::NEXT_LIBRARY_ROUTE);
    let image = absolute_url("/assets/icons/favicon.svg");
    set_document_head(HeadMeta {
        title: "Systemless | Play classic 68K and PowerPC Mac games in your browser".to_string(),
        description: description.to_string(),
        robots: "noindex,nofollow",
        canonical_url: canonical.clone(),
        image_url: image,
        schema_json: None,
    });
}

fn set_document_head_for_game(game: &Game) {
    let canonical = absolute_url(&browser_path_for_route(&canonical_path_for_game(game), "/"));
    let image = crate::paths::optional_asset_path(game.assets.screenshot_path)
        .map(|path| absolute_url(&path))
        .unwrap_or_else(|| absolute_url("/assets/icons/favicon.svg"));
    set_document_head(HeadMeta {
        title: format!("{}{} ({}) | Systemless", if game.approved { "Play " } else { "" }, game.title, game.year),
        description: game.description.to_string(),
        robots: if game.approved {
            "index,follow"
        } else {
            "noindex,nofollow"
        },
        canonical_url: canonical.clone(),
        image_url: image.clone(),
        schema_json: game.approved.then(|| format!(
            "{{\"@context\":\"https://schema.org\",\"@type\":\"VideoGame\",\"name\":{},\"description\":{},\"url\":{},\"image\":{},\"datePublished\":{},\"gamePlatform\":\"Classic Macintosh\",\"operatingSystem\":\"Classic Mac OS\",\"creator\":{{\"@type\":\"Organization\",\"name\":{}}}}}",
            json_string(game.title),
            json_string(game.description),
            json_string(&canonical),
            json_string(&image),
            json_string(game.year),
            json_string(game.developer)
        )),
    });
}

struct HeadMeta {
    title: String,
    description: String,
    robots: &'static str,
    canonical_url: String,
    image_url: String,
    schema_json: Option<String>,
}

fn set_document_head(meta: HeadMeta) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };

    document.set_title(&display_document_title(&meta.title));
    set_meta_name(&document, "description", &meta.description);
    set_meta_name(&document, "robots", meta.robots);
    set_meta_name(&document, "twitter:title", &meta.title);
    set_meta_name(&document, "twitter:description", &meta.description);
    set_meta_name(&document, "twitter:image", &meta.image_url);
    set_meta_property(&document, "og:title", &meta.title);
    set_meta_property(&document, "og:description", &meta.description);
    set_meta_property(&document, "og:url", &meta.canonical_url);
    set_meta_property(&document, "og:image", &meta.image_url);
    set_canonical(&document, &meta.canonical_url);
    set_schema(&document, meta.schema_json.as_deref());
}

fn display_document_title(seo_title: &str) -> String {
    if should_use_compact_document_title() {
        COMPACT_DOCUMENT_TITLE.to_string()
    } else {
        seo_title.to_string()
    }
}

#[cfg(target_arch = "wasm32")]
fn should_use_compact_document_title() -> bool {
    web_sys::window()
        .and_then(|window| {
            window
                .match_media(COMPACT_DOCUMENT_TITLE_MEDIA)
                .ok()
                .flatten()
        })
        .map(|query| query.matches())
        .unwrap_or(false)
}

#[cfg(not(target_arch = "wasm32"))]
fn should_use_compact_document_title() -> bool {
    let _ = COMPACT_DOCUMENT_TITLE_MEDIA;
    false
}

fn set_meta_name(document: &web_sys::Document, name: &str, content: &str) {
    set_head_element_attr(
        document,
        &format!("meta[name='{name}']"),
        "meta",
        "name",
        name,
        content,
    );
}

fn set_meta_property(document: &web_sys::Document, property: &str, content: &str) {
    set_head_element_attr(
        document,
        &format!("meta[property='{property}']"),
        "meta",
        "property",
        property,
        content,
    );
}

fn set_head_element_attr(
    document: &web_sys::Document,
    selector: &str,
    tag: &str,
    key_attr: &str,
    key_value: &str,
    content: &str,
) {
    let Some(head) = document.head() else {
        return;
    };
    let element = document
        .query_selector(selector)
        .ok()
        .flatten()
        .or_else(|| {
            document.create_element(tag).ok().inspect(|el| {
                let _ = el.set_attribute(key_attr, key_value);
                let _ = head.append_child(el);
            })
        });
    if let Some(element) = element {
        let _ = element.set_attribute("content", content);
    }
}

fn set_canonical(document: &web_sys::Document, href: &str) {
    let Some(head) = document.head() else {
        return;
    };
    let element = document
        .query_selector("link[rel='canonical']")
        .ok()
        .flatten()
        .or_else(|| {
            document.create_element("link").ok().inspect(|el| {
                let _ = el.set_attribute("rel", "canonical");
                let _ = head.append_child(el);
            })
        });
    if let Some(element) = element {
        let _ = element.set_attribute("href", href);
    }
}

fn set_schema(document: &web_sys::Document, json: Option<&str>) {
    let Some(head) = document.head() else {
        return;
    };
    if json.is_none() {
        if let Ok(Some(element)) = document
            .query_selector("script[type='application/ld+json'][data-systemless-schema='true']")
        {
            element.remove();
        }
        return;
    }

    let element = document
        .query_selector("script[type='application/ld+json'][data-systemless-schema='true']")
        .ok()
        .flatten()
        .or_else(|| {
            document
                .query_selector("script[type='application/ld+json']")
                .ok()
                .flatten()
        })
        .or_else(|| {
            document.create_element("script").ok().inspect(|el| {
                let _ = el.set_attribute("type", "application/ld+json");
                let _ = head.append_child(el);
            })
        });
    if let Some(element) = element {
        let _ = element.set_attribute("type", "application/ld+json");
        let _ = element.set_attribute("data-systemless-schema", "true");
        element.set_text_content(json);
    }
}

fn absolute_url(path: &str) -> String {
    let origin = web_sys::window()
        .and_then(|window| window.location().origin().ok())
        .unwrap_or_else(|| "https://systemless.org".to_string());
    let normalized = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    format!("{}{}", origin.trim_end_matches('/'), normalized)
}

fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}
