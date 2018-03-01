use errors::*;
use web::common;

use actix;
use actix_web::{HttpRequest, HttpResponse, StatusCode};
use diesel::pg::PgConnection;
use r2d2::Pool;
use r2d2_diesel::ConnectionManager;
use slog::Logger;

//
// Macros
//

/// Creates an asynchronous HTTP handler function suitable for use with Actix for the current
/// endpoint module.
///
/// The key point to understand here is that because we have a convention so that all `Params` and
/// `ViewModel`s are given the same name in every module, this can be pulled in and expanded while
/// still properly resolving symbols.
///
/// Honestly, I would've preferred not to have to sink into a macro to get this working, but I
/// started running into some serious typing problems when trying to make this a generic function.
/// Be it with generics or associated types I'd always get a complaint from the compiler that there
/// was no implementation for the generic version when sending a message to Actix (and in a few
/// other places). After trying many different approaches and failing on all of them, I eventually
/// just resorted to this. To keep things clean, offload as much work as possible to functions
/// outside of the macro. Try to change this as little as possible.
macro_rules! handler {
    () => (
        pub fn handler(
            mut req: HttpRequest<endpoints::StateImpl>,
        ) -> Box<Future<Item = HttpResponse, Error = Error>> {
            use web::endpoints;
            // Imported so that we can use the traits, but assigned a different name to avoid
            // clashing with the module's implementations.
            use web::endpoints::Params as P;
            use web::endpoints::ViewModel as VM;
            use web::middleware;

            use actix_web::AsyncResponder;
            use futures::future;

            let log = middleware::log_initializer::log(&mut req);

            let params = match Params::build(&log, &req) {
                Ok(params) => params,
                Err(e) => return Box::new(future::err(e)),
            };

            let message = endpoints::Message::new(&log, params);

            req.state()
                .sync_addr
                .call_fut(message)
                .chain_err(|| "Error from SyncExecutor")
                .from_err()
                .and_then(move |res| {
                    let response = res?;
                    let view_model = ViewModel::build(&log, &req, response);
                    view_model.render(&log, &req)
                })
                .responder()
        }
    )
}

//
// Traits
//

/// A trait to be implemented for the typed responses that come back from `SyncExecutor`. This
/// usually contains information loaded from a database.
pub trait ExecutorResponse {}

/// A trait to be implemented for parameters that are decoded from an incoming HTTP request. It's
/// also reused as a message to be received by `SyncExecutor` containing enough information to run
/// its synchronous database operations.
pub trait Params: Sized {
    /// Builds a `Params` implementation by decoding an HTTP request. This may result in an error
    /// if appropriate parameters were not found or not valid.
    fn build(log: &Logger, req: &HttpRequest<StateImpl>) -> Result<Self>;
}

/// A trait to be implemented by the view models that render views. A view model is a model
/// containing all the information needed to build a view. In our case it wraps a response that
/// comes from from `SyncExecutor`.
pub trait ViewModel {
    type ExecutorResponse: ExecutorResponse;

    /// Builds a `ViewModel` implementation from an HTTP request and a response from
    /// `SyncExecutor`.
    fn build(log: &Logger, req: &HttpRequest<StateImpl>, response: Self::ExecutorResponse) -> Self;

    /// Renders a `ViewModel` implementation to an HTTP response. This could be a standard HTML
    /// page, but could also be any arbitrary response like a redirect.
    fn render(&self, log: &Logger, req: &HttpRequest<StateImpl>) -> Result<HttpResponse>;
}

//
// Structs
//

pub struct CommonViewModel {
    pub assets_version: String,
    pub title:          String,
}

pub struct Message<P: Params> {
    pub log:    Logger,
    pub params: P,
}

impl<P: Params> Message<P> {
    fn new(log: &Logger, params: P) -> Message<P> {
        Message {
            log: log.clone(),
            params,
        }
    }
}

pub struct StateImpl {
    pub assets_version: String,
    pub log:            Logger,
    pub pool:           Pool<ConnectionManager<PgConnection>>,
    pub sync_addr:      actix::prelude::SyncAddress<SyncExecutor>,
}

impl common::State for StateImpl {
    fn log(&self) -> &Logger {
        &self.log
    }
}

pub struct SyncExecutor {
    pub pool: Pool<ConnectionManager<PgConnection>>,
}

impl actix::Actor for SyncExecutor {
    type Context = actix::SyncContext<Self>;
}

//
// Error handlers
//

pub fn handle_404() -> Result<HttpResponse> {
    Ok(HttpResponse::build(StatusCode::NOT_FOUND)
        .content_type("text/html; charset=utf-8")
        .body("404!")?)
}

//
// Endpoints
//

pub mod directory_podcast_show {
    use errors::*;
    use http_requester::HTTPRequesterLive;
    use mediators::directory_podcast_updater::DirectoryPodcastUpdater;
    use model;
    use schema;
    use web::endpoints;

    use actix;
    use actix_web::{HttpRequest, HttpResponse, StatusCode};
    use diesel::prelude::*;
    use futures::future::Future;
    use hyper::Client;
    use hyper_tls::HttpsConnector;
    use slog::Logger;
    use tokio_core::reactor::Core;

    handler!();

    pub enum ExecutorResponse {
        Exception(model::DirectoryPodcastException),
        NotFound,
        Podcast(model::Podcast),
    }

    impl endpoints::ExecutorResponse for ExecutorResponse {}

    struct Params {
        id: i64,
    }

    impl endpoints::Params for Params {
        fn build(_log: &Logger, req: &HttpRequest<endpoints::StateImpl>) -> Result<Self> {
            Ok(Self {
                id: req.match_info()
                    .get("id")
                    .unwrap()
                    .parse::<i64>()
                    .chain_err(|| "Error parsing ID")?,
            })
        }
    }

    // TODO: `ResponseType` will change to `Message`
    impl actix::prelude::ResponseType for endpoints::Message<Params> {
        type Item = ExecutorResponse;
        type Error = Error;
    }

    struct ViewModel {
        _common:  endpoints::CommonViewModel,
        response: ExecutorResponse,
    }

    impl endpoints::ViewModel for ViewModel {
        type ExecutorResponse = ExecutorResponse;

        fn build(
            _log: &Logger,
            req: &HttpRequest<endpoints::StateImpl>,
            response: Self::ExecutorResponse,
        ) -> Self {
            ViewModel {
                _common: endpoints::CommonViewModel {
                    assets_version: req.state().assets_version.clone(),
                    title:          "".to_owned(),
                },
                response,
            }
        }

        fn render(
            &self,
            _log: &Logger,
            _req: &HttpRequest<endpoints::StateImpl>,
        ) -> Result<HttpResponse> {
            match self.response {
                ExecutorResponse::Exception(ref _dir_podcast_ex) => {
                    Err(Error::from("Couldn't expand directory podcast"))
                }
                ExecutorResponse::NotFound => Ok(endpoints::handle_404()?),
                ExecutorResponse::Podcast(ref podcast) => {
                    Ok(HttpResponse::build(StatusCode::PERMANENT_REDIRECT)
                        .header("Location", format!("/podcasts/{}", podcast.id).as_str())
                        .finish()?)
                }
            }
        }
    }

    impl actix::prelude::Handler<endpoints::Message<Params>> for endpoints::SyncExecutor {
        type Result = actix::prelude::MessageResult<endpoints::Message<Params>>;

        fn handle(
            &mut self,
            message: endpoints::Message<Params>,
            _: &mut Self::Context,
        ) -> Self::Result {
            let conn = self.pool.get()?;
            let log = message.log;

            info!(log, "Expanding directory podcast"; "id" => message.params.id);

            let core = Core::new().unwrap();
            let client = Client::configure()
                .connector(HttpsConnector::new(4, &core.handle()).map_err(Error::from)?)
                .build(&core.handle());
            let mut http_requester = HTTPRequesterLive { client, core };

            let dir_podcast: Option<model::DirectoryPodcast> = schema::directory_podcast::table
                .filter(schema::directory_podcast::id.eq(message.params.id))
                .first(&*conn)
                .optional()?;
            match dir_podcast {
                Some(mut dir_podcast) => {
                    let mut mediator = DirectoryPodcastUpdater {
                        conn:           &*conn,
                        dir_podcast:    &mut dir_podcast,
                        http_requester: &mut http_requester,
                    };
                    let res = mediator.run(&log)?;

                    if let Some(dir_podcast_ex) = res.dir_podcast_ex {
                        return Ok(ExecutorResponse::Exception(dir_podcast_ex));
                    }

                    Ok(ExecutorResponse::Podcast(res.podcast.unwrap()))
                }
                None => Ok(ExecutorResponse::NotFound),
            }
        }
    }
}
