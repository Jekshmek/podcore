use middleware;
use model;

use actix;
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse};
use diesel::pg::PgConnection;
use errors::*;
use r2d2::Pool;
use r2d2_diesel::ConnectionManager;
use slog::Logger;

//
// Macros
//

/// Macro that easily creates the scaffolding necessary for a
/// `server::SyncExecutor` message handler from within an endpoint. It puts the
/// necessary type definitions in place and creates a wrapper function with
/// access to a connection and log.
#[macro_export]
macro_rules! message_handler {
    () => {
        impl ::actix::prelude::Handler<server::Message<Params>> for server::SyncExecutor {
            type Result = Result<ViewModel>;

            fn handle(
                &mut self,
                message: server::Message<Params>,
                _: &mut Self::Context,
            ) -> Self::Result {
                let conn = self.pool.get()?;
                let log = message.log.clone();
                time_helpers::log_timed(&log.new(o!("step" => "handle_message")), |log| {
                    handle_inner(log, &*conn, message.params)
                })
            }
        }

        impl ::actix::prelude::Message for server::Message<Params> {
            type Result = Result<ViewModel>;
        }
    };
}

//
// Traits
//

/// A trait to be implemented for parameters that are decoded from an incoming
/// HTTP request. It's also reused as a message to be received by
/// `SyncExecutor` containing enough information to run its synchronous
/// database operations.
pub trait Params: Sized {
    /// Builds a `Params` implementation by decoding an HTTP request. This may
    /// result in an error if appropriate parameters were not found or not
    /// valid.
    ///
    /// `HttpRequest` is mutable because we're allowed to reach into a session
    /// to build parameters.
    fn build<S: State>(log: &Logger, req: &mut HttpRequest<S>) -> Result<Self>;
}

pub trait State {
    fn log(&self) -> &Logger;

    fn sync_addr(&self) -> &actix::prelude::Addr<actix::prelude::Syn, SyncExecutor>;
}

//
// Trait implementations
//

impl From<Error> for ::actix_web::error::Error {
    fn from(error: Error) -> Self {
        ::actix_web::error::ErrorInternalServerError(error.to_string())
    }
}

//
// Structs
//

pub struct Message<P: Params> {
    pub log:    Logger,
    pub params: P,
}

impl<P: Params> Message<P> {
    pub fn new(log: &Logger, params: P) -> Message<P> {
        Message {
            log: log.clone(),
            params,
        }
    }
}

pub struct StateImpl {
    // Assets are versioned so that they can be expired immediately without worrying about any kind
    // of client-side caching. This is a version represented as a string.
    //
    // Note that this is only used by `web::Server`.
    pub assets_version: String,

    pub log:       Logger,
    pub sync_addr: actix::prelude::Addr<actix::prelude::Syn, SyncExecutor>,
}

impl State for StateImpl {
    fn log(&self) -> &Logger {
        &self.log
    }

    fn sync_addr(&self) -> &actix::prelude::Addr<actix::prelude::Syn, SyncExecutor> {
        &self.sync_addr
    }
}

pub struct SyncExecutor {
    pub pool: Pool<ConnectionManager<PgConnection>>,
}

impl actix::Actor for SyncExecutor {
    type Context = actix::SyncContext<Self>;
}

//
// Functions
//

/// Gets the authenticated account through either the API or web authenticator
/// middleware (the former not being implemented yet). The account is cloned so
/// that it can be moved into a `Param` and sent to a `SyncExecutor`.
///
/// It'd be nice to know in advance which is in use in this context, but I'm
/// not totally sure how to do that in a way that doesn't suck.
pub fn account<S: State>(req: &mut HttpRequest<S>) -> Option<model::Account> {
    {
        if let Some(account) = middleware::api::authenticator::account(req) {
            return Some(account.clone());
        }
    }

    {
        if let Some(account) = middleware::web::authenticator::account(req) {
            return Some(account.clone());
        }
    }

    // This is a path that's used only by the test suite which allows us to set an
    // authenticated account much more easily. The `cfg!` macro allows it to be
    // optimized out for release builds so that it doesn't slow things down.
    if cfg!(test) {
        if let Some(account) = middleware::test::authenticator::account(req) {
            return Some(account.clone());
        }
    }

    None
}

/// Handles a `Result` and renders an error that was intended for the user by
/// invoking the given `render_user` function.
///
/// If `render_user` fails or the user wasn't intended to be user-facing,
/// `render_internal` is invoked instead.
pub fn render_error<FInternal, FUser>(
    log: &Logger,
    e: Error,
    render_internal: FInternal,
    render_user: FUser,
) -> HttpResponse
where
    FInternal: FnOnce(&Logger, StatusCode, String) -> HttpResponse,
    FUser: FnOnce(&Logger, StatusCode, String) -> Result<HttpResponse>,
{
    // Note that `format!` activates the `Display` trait and shows our errors'
    // `display` definition
    let res = match e {
        e @ Error(ErrorKind::BadParameter(_, _), _) => {
            render_user(log, StatusCode::BAD_REQUEST, format!("{}", e))
        }
        e @ Error(ErrorKind::BadRequest(_), _) => {
            render_user(log, StatusCode::BAD_REQUEST, format!("{}", e))
        }
        e @ Error(ErrorKind::NotFound(_, _), _) => {
            render_user(log, StatusCode::NOT_FOUND, format!("{}", e))
        }
        e @ Error(ErrorKind::NotFoundGeneral(_), _) => {
            render_user(log, StatusCode::NOT_FOUND, format!("{}", e))
        }
        e @ Error(ErrorKind::Unauthorized, _) => {
            render_user(log, StatusCode::UNAUTHORIZED, format!("{}", e))
        }
        e => Err(e),
    };

    // Non-user errors will fall through to `render_internal`, but also errors that
    // occurred while trying to render one of the user errors above.
    match res {
        Ok(response) => response,
        Err(e) => {
            // This is an internal error, so print it out
            error!(log, "Encountered internal error: {}", e);

            render_internal(log, StatusCode::UNAUTHORIZED, format!("{}", e))
        }
    }
}
