use crate::catalogue::Game;
use leptos::prelude::*;

#[component]
pub fn GameInfoContent(game: &'static Game, tab: &'static str) -> impl IntoView {
    let links = game.community;
    match tab {
        "about" => view! {
            <article class="game-wiki">
                <div class="game-wiki__actions">
                    <a href=links.edit target="_blank" rel="noopener noreferrer">"Edit this page / propose a PR"</a>
                    <a href=links.source target="_blank" rel="noopener noreferrer">"View Markdown source"</a>
                </div>
                <p>{game.description}</p>
                <div class="catalogue-notes" inner_html=game.content_html></div>
                <p class="game-wiki__hint">"This community page comes from the catalogue entry. Propose an edit on GitHub to improve its notes, history, or instructions."</p>
            </article>
        }.into_any(),
        "license" => view! {
            <article class="game-wiki">
                <div class="catalogue-notes" inner_html=game.license_html></div>
                <div class="game-wiki__actions">
                    <a href=links.edit target="_blank" rel="noopener noreferrer">"Suggest a licensing correction"</a>
                    <a href=links.source target="_blank" rel="noopener noreferrer">"View recorded evidence"</a>
                </div>
            </article>
        }.into_any(),
        "issues" => view! {
            <article class="game-wiki">
                <h2>"Open an issue"</h2>
                <p>"Choose the relevant GitHub action. Each form includes this game’s catalogue identifier."</p>
                <h3>"Compatibility"</h3>
                <p>"Compatibility reports live in the Systemless issue tracker, where fixes and discussion stay with the runtime."</p>
                <p>{if game.approved { "Browser launching is enabled." }
                    else { "Browser launching is currently disabled." }}</p>
                <p>{format!("Supported catalogue architectures: {}", game.architectures.iter().map(|a| a.label()).collect::<Vec<_>>().join(", "))}</p>
                <div class="game-wiki__actions">
                    <a href=links.open_reports target="_blank" rel="noopener noreferrer">"View open compatibility issues"</a>
                    <a href=links.compatibility_issue target="_blank" rel="noopener noreferrer">"Report a compatibility issue"</a>
                </div>
                <h3>"Catalogue and controls"</h3>
                <div class="game-wiki__actions">
                    <a href=links.metadata_issue target="_blank" rel="noopener noreferrer">"Suggest a metadata correction"</a>
                    <a href=links.suggest_controls_config target="_blank" rel="noopener noreferrer">"Suggest controls or settings"</a>
                </div>
                <h3>"Takedown request"</h3>
                <p>"Rights holders can request removal or correction of material associated with this game."</p>
                <p>"The request opens a public GitHub issue. Include the affected material and your request; avoid posting private personal information."</p>
                <a href=links.takedown_request target="_blank" rel="noopener noreferrer">"Open takedown request"</a>
            </article>
        }.into_any(),
        "keys" => view! { <MappedKeys game=game/> }.into_any(),
        _ => view! {}.into_any(),
    }
}

#[component]
fn MappedKeys(game: &'static Game) -> impl IntoView {
    let controls = game.settings.mobile_controls;
    view! {
        <article class="game-wiki">
            <h2>"Mapped keys"</h2>
            <p>"Click or tap the game to focus keyboard input. Unmapped keys use their normal Macintosh equivalents; in-game preferences may change what they do."</p>
            <Show when=move || !game.settings.key_mappings.is_empty() fallback=|| view! { <p>"No custom keyboard remappings are configured for this entry."</p> }>
                <table><thead><tr><th>"Browser key"</th><th>"Sent to game"</th></tr></thead>
                    <tbody>{game.settings.key_mappings.iter().map(|(from, to)| view! {
                        <tr><td><kbd>{*from}</kbd></td><td><kbd>{*to}</kbd></td></tr>
                    }).collect_view()}</tbody>
                </table>
            </Show>
            <Show when=move || game.settings.arrows_as_numpad>
                <p>"Arrow keys are sent as numeric keypad keys: ↑ = 8, ↓ = 2, ← = 4, → = 6."</p>
            </Show>
            <Show when=move || controls.enabled>
                <h3>"Touch controls"</h3>
                <table><thead><tr><th>"Control"</th><th>"Mapped key"</th></tr></thead><tbody>
                    {[("Joystick up", controls.joystick.up), ("Joystick down", controls.joystick.down),
                        ("Joystick left", controls.joystick.left), ("Joystick right", controls.joystick.right)]
                        .into_iter().map(|(label, key)| view! { <tr><td>{label}</td><td><kbd>{key}</kbd></td></tr> }).collect_view()}
                    {controls.buttons.iter().filter(|_| controls.button_groups.is_empty()).map(|button| view! { <tr><td>{button.label}</td><td><kbd>{button.key}</kbd></td></tr> }).collect_view()}
                    {controls.button_groups.iter().flat_map(|group| group.buttons.iter().map(move |button| view! {
                        <tr><td>{format!("{} / {}", group.label, button.label)}</td><td><kbd>{button.key}</kbd></td></tr>
                    })).collect_view()}
                </tbody></table>
            </Show>
            <p><a href=game.community.suggest_controls_config target="_blank" rel="noopener noreferrer">"Suggest a controls correction"</a></p>
        </article>
    }
}
