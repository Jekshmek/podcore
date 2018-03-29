use errors::*;
use model;
use model::insertable;
use schema;
use time_helpers;

use diesel;
use diesel::pg::PgConnection;
use diesel::pg::upsert::excluded;
use diesel::prelude::*;
use slog::Logger;

pub struct Mediator<'a> {
    pub account_podcast:  &'a model::AccountPodcast,
    pub conn:             &'a PgConnection,
    pub episode:          &'a model::Episode,
    pub listened_seconds: Option<i64>,
    pub played:           bool,
}

impl<'a> Mediator<'a> {
    pub fn run(&mut self, log: &Logger) -> Result<RunResult> {
        self.conn.transaction::<_, Error, _>(|| self.run_inner(log))
    }

    fn run_inner(&mut self, log: &Logger) -> Result<RunResult> {
        let account_podcast_episode = self.upsert_account_podcast_episode(log)?;
        Ok(RunResult {
            account_podcast_episode,
        })
    }

    //
    // Steps
    //

    fn upsert_account_podcast_episode(
        &mut self,
        log: &Logger,
    ) -> Result<model::AccountPodcastEpisode> {
        let ins_episode = insertable::AccountPodcastEpisode {
            account_podcast_id: self.account_podcast.id,
            episode_id:         self.episode.id,
            listened_seconds:   self.listened_seconds,
            played:             self.played,
        };

        time_helpers::log_timed(
            &log.new(o!("step" => "upsert_account_podcast_episode")),
            |_log| {
                diesel::insert_into(schema::account_podcast_episode::table)
                    .values(&ins_episode)
                    .on_conflict((
                        schema::account_podcast_episode::account_podcast_id,
                        schema::account_podcast_episode::episode_id,
                    ))
                    .do_update()
                    .set((
                        schema::account_podcast_episode::listened_seconds
                            .eq(excluded(schema::account_podcast_episode::listened_seconds)),
                        schema::account_podcast_episode::played
                            .eq(excluded(schema::account_podcast_episode::played)),
                    ))
                    .get_result(self.conn)
                    .chain_err(|| "Error upserting account podcast episode")
            },
        )
    }
}

pub struct RunResult {
    pub account_podcast_episode: model::AccountPodcastEpisode,
}

//
// Tests
//

#[cfg(test)]
mod tests {
    use mediators::account_podcast_episode_upserter::*;
    use test_data;
    use test_helpers;

    use r2d2::PooledConnection;
    use r2d2_diesel::ConnectionManager;

    #[test]
    fn test_account_podcast_episode_upsert_partially_played() {
        let mut bootstrap = TestBootstrap::new(Args {
            account_podcast:  None,
            episode:          None,
            listened_seconds: Some(10),
            played:           false,
        });
        let (mut mediator, log) = bootstrap.mediator();
        let res = mediator.run(&log).unwrap();

        assert_ne!(0, res.account_podcast_episode.id);
    }

    #[test]
    fn test_account_podcast_episode_upsert_played() {
        let mut bootstrap = TestBootstrap::new(Args {
            account_podcast:  None,
            episode:          None,
            listened_seconds: None,
            played:           true,
        });
        let (mut mediator, log) = bootstrap.mediator();
        let res = mediator.run(&log).unwrap();

        assert_ne!(0, res.account_podcast_episode.id);
    }

    //
    // Private types/functions
    //

    struct Args {
        account_podcast:  Option<model::AccountPodcast>,
        episode:          Option<model::Episode>,
        listened_seconds: Option<i64>,
        played:           bool,
    }

    struct TestBootstrap {
        _common: test_helpers::CommonTestBootstrap,
        args:    Args,
        conn:    PooledConnection<ConnectionManager<PgConnection>>,
        log:     Logger,
    }

    impl TestBootstrap {
        fn new(mut args: Args) -> TestBootstrap {
            let conn = test_helpers::connection();
            let log = test_helpers::log();

            if args.account_podcast.is_none() {
                let account_podcast = test_data::account_podcast::insert(&log, &*conn);
                let episode: model::Episode = schema::episode::table
                    .filter(schema::episode::podcast_id.eq(account_podcast.podcast_id))
                    .limit(1)
                    .get_result(&*conn)
                    .unwrap();

                args.account_podcast = Some(account_podcast);
                args.episode = Some(episode);
            }

            TestBootstrap {
                _common: test_helpers::CommonTestBootstrap::new(),
                args: args,
                conn,
                log,
            }
        }

        fn mediator(&mut self) -> (Mediator, Logger) {
            (
                Mediator {
                    account_podcast:  self.args.account_podcast.as_ref().clone().unwrap(),
                    conn:             &*self.conn,
                    episode:          self.args.episode.as_ref().clone().unwrap(),
                    listened_seconds: self.args.listened_seconds.clone(),
                    played:           self.args.played,
                },
                self.log.clone(),
            )
        }
    }
}
