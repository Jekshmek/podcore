use actix_web::http::StatusCode;
use errors::*;
use web::endpoints;

use horrorshow::helper::doctype;
use horrorshow::prelude::*;

//
// Layouts
//

pub fn render_layout(view_model: &endpoints::CommonViewModel, content: &str) -> Result<String> {
    (html! {
        : doctype::HTML;
        html {
            head {
                title: view_model.title.as_str();

                meta(content="text/html; charset=utf-8", http-equiv="Content-Type");

                link(href=format_args!("/assets/{}/app.css", view_model.assets_version), media="screen", rel="stylesheet", type="text/css");

                // curl -L -o assets/react.production.min.js https://unpkg.com/react@16/umd/react.production.min.js
                script(defer, src=format_args!("/assets/{}/react.production.min.js", view_model.assets_version)) {}

                // curl -L -o assets/react-dom.production.min.js https://unpkg.com/react-dom@16/umd/react-dom.production.min.js
                script(defer, src=format_args!("/assets/{}/react-dom.production.min.js", view_model.assets_version)) {}

                script(defer, src=format_args!("/assets/{}/app.js", view_model.assets_version)) {}
            }
            body {
                span {
                    @ if let Some(ref account) = view_model.account {
                        : format_args!("Account ID: {}", account.id)
                    } else {
                        : "Not account set"
                    }
                }
                container {
                    : Raw(content)
                }
            }
        }
    }).into_string()
        .map_err(Error::from)
}

//
// Errors
//

pub fn render_user_error(code: StatusCode, message: String) -> Result<String> {
    let title = format!("Error: {}", code);
    render_layout(
        // It's somewhat difficult to access `State` from here, so we just take advantage of the
        // fact that assets will respond for every version. Hopefully errors aren't shown so often
        // that some CSS staleness on error pages will be a problem.
        &endpoints::CommonViewModel {
            account:        None,
            assets_version: "1".to_owned(),
            title:          title.to_owned(),
        },
        (html! {
            h1: title;
            p: message;
        }).into_string()?
            .as_str(),
    )
}

//
// Views
//

pub mod episode_show {
    use errors::*;
    use web::endpoints::CommonViewModel;
    use web::endpoints::episode_show::view_model;
    use web::views;

    use horrorshow::Template;
    use horrorshow::prelude::*;

    pub fn render(common: &CommonViewModel, view_model: &view_model::Ok) -> Result<String> {
        views::render_layout(
            common,
            (html! {
                h1: view_model.episode.title.as_str();
                div(id="favorited-toggle") {}
                div(id="played-toggle") {}
                p: view_model.episode.media_url.as_str();
                @ if let Some(ref description) = view_model.episode.description {
                    p: description.as_str();
                }
                @ if view_model.account_podcast.is_some() {
                    script : Raw(views::react_element(
                        "EpisodeFavoritedToggler",
                        "favorited-toggle",
                        &json!({
                            "episodeId": view_model.episode.id.to_string(),
                            "favorited": view_model.is_episode_favorited(),
                        }).to_string(),
                    ));
                    script : Raw(views::react_element(
                        "EpisodePlayedToggler",
                        "played-toggle",
                        &json!({
                            "episodeId": view_model.episode.id.to_string(),
                            "played": view_model.is_episode_played(),
                        }).to_string(),
                    ));
                }
            }).into_string()?
                .as_str(),
        )
    }
}

pub mod podcast_show {
    use errors::*;
    use web::endpoints::CommonViewModel;
    use web::endpoints::podcast_show::view_model;
    use web::views;

    use horrorshow::Template;
    use horrorshow::prelude::*;

    pub fn render(common: &CommonViewModel, view_model: &view_model::Ok) -> Result<String> {
        views::render_layout(
            common,
            (html! {
                h1: view_model.podcast.title.as_str();
                div(id="subscribed-toggle") {}
                p {
                    : "Hello! This is <html />"
                }
                ul {
                    @ for episode in &view_model.episodes {
                        li {
                            a(href=format_args!("/podcasts/{}/episodes/{}", episode.podcast_id, episode.id)) {
                                : episode.title.as_str()
                            }
                        }
                    }
                }
                script : Raw(views::react_element(
                    "PodcastSubscribedToggler",
                    "subscribed-toggle",
                    &json!({
                        "podcastId": view_model.podcast.id.to_string(),
                        "subscribed": view_model.is_subscribed(),
                    }).to_string(),
                ));
            }).into_string()?
                .as_str(),
        )
    }
}

pub mod search_new_show {
    use errors::*;
    use web::endpoints::CommonViewModel;
    use web::endpoints::search_new_show::ViewModel;
    use web::views;

    use horrorshow::Template;

    pub fn render(common: &CommonViewModel, _view_model: &ViewModel) -> Result<String> {
        views::render_layout(
            common,
            (html! {
                h1: "Search";
                form(action="/search", method="get") {
                    input(type="text", name="q");
                    input(type="submit", value="Submit");
                }
            }).into_string()?
                .as_str(),
        )
    }
}

pub mod search_show {
    use errors::*;
    use web::endpoints::CommonViewModel;
    use web::endpoints::search_show::view_model;
    use web::views;

    use horrorshow::Template;

    pub fn render(
        common: &CommonViewModel,
        view_model: &view_model::SearchResults,
    ) -> Result<String> {
        views::render_layout(
            common,
            (html! {
                p {
                    : format_args!("Query: {}", view_model.query);
                }
                ul {
                    @ for dir_podcast in &view_model.directory_podcasts {
                        li {
                            @ if let Some(podcast_id) = dir_podcast.podcast_id {
                                a(href=format_args!("/podcasts/{}", podcast_id)) {
                                    : dir_podcast.title.as_str()
                                }
                            } else {
                                a(href=format_args!("/directory-podcasts/{}", dir_podcast.id)) {
                                    : dir_podcast.title.as_str()
                                }
                            }
                        }
                    }
                }
            }).into_string()?
                .as_str(),
        )
    }
}

//
// Other helpers
//

/// Generates a simple initializer for a React component targeting a specific
/// container in the DOM. Use of the `json!` macro is recommended to generate
/// properties.
///
/// This should probably be a macro, but I'm too lazy to write on right now.
#[inline]
pub fn react_element(component: &str, container: &str, properties: &str) -> String {
    // Our scripts use `defer` so make sure to only run this on the `load` event.
    format!(
        "window.addEventListener('load', function () {{ ReactDOM.render(React.createElement({}, {}), document.getElementById('{}')); }});",
        component, properties, container
    ).to_owned()
}
