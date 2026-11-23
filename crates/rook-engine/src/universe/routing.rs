//! Which server a teleport goes to, as Roblox's matchmaking decides it
//! (creator-docs TeleportService, TeleportOptions, "Teleport between
//! places"): one function of the request, the universe and the servers
//! running, so every host routes alike. The words of each failure are
//! Rook's (Inference: Studio's aren't measured).

use rook_lua::teleport::{
    Destination, PlaceInstance, Routing, TeleportResult, access_code, parse_access_code,
};

use super::{ServerIdentity, Universe};

/// A running server, as routing sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningServer {
    pub identity: ServerIdentity,
    /// Its players, and those already on their way to it.
    pub players: usize,
    /// `Players.MaxPlayers`.
    pub max_players: usize,
    pub users: Vec<i64>,
}

impl RunningServer {
    fn has_room(&self, joining: usize) -> bool {
        self.players + joining <= self.max_players
    }
}

/// The server a teleport goes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A running server, by `JobId`.
    Running(String),
    /// A server to start.
    New(ServerIdentity),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub target: Target,
    pub destination: Destination,
}

/// Why a teleport can't go: its `TeleportInitFailed` result and message.
pub type Refusal = (TeleportResult, String);

/// Route `joining` players to `place_id` by `routing`.
pub fn route(
    universe: &Universe,
    running: &[RunningServer],
    place_id: i64,
    routing: &Routing,
    joining: usize,
) -> Result<Route, Refusal> {
    if !universe.has_place(place_id) {
        return Err((
            TeleportResult::GameNotFound,
            format!("Place {place_id} is not part of this game"),
        ));
    }
    let of_place = || running.iter().filter(|s| s.identity.place_id == place_id);
    let reserved = |id: &str, access_code: String| {
        let (target, job_id) = match of_place().find(|s| s.identity.private_server_id == id) {
            Some(server) => join(server, joining)?,
            None => start(ServerIdentity::new_server(
                universe,
                place_id,
                id.to_owned(),
            )),
        };
        Ok(Route {
            target,
            destination: Destination {
                job_id,
                private_server_id: id.to_owned(),
                access_code,
            },
        })
    };
    match routing {
        Routing::Public => {
            // Creator-docs: the first player's information is used for
            // matchmaking. Rook fills the oldest server with room.
            let server =
                of_place().find(|s| s.identity.private_server_id.is_empty() && s.has_room(joining));
            let (target, job_id) = match server {
                Some(server) => join(server, joining)?,
                None => start(ServerIdentity::new_server(
                    universe,
                    place_id,
                    String::new(),
                )),
            };
            Ok(Route {
                target,
                destination: Destination {
                    job_id,
                    ..Destination::default()
                },
            })
        }
        Routing::Instance(job) => {
            let Some(server) = of_place().find(|s| s.identity.job_id == *job) else {
                return Err((
                    TeleportResult::GameEnded,
                    format!("The server {job} of place {place_id} isn't running"),
                ));
            };
            let (target, job_id) = join(server, joining)?;
            Ok(Route {
                target,
                destination: Destination {
                    job_id,
                    private_server_id: server.identity.private_server_id.clone(),
                    access_code: String::new(),
                },
            })
        }
        Routing::Reserved(id) => reserved(id, String::new()),
        Routing::AccessCode(code) => match parse_access_code(code) {
            Some((place, id)) if place == place_id => reserved(id, code.clone()),
            _ => Err((
                TeleportResult::Unauthorized,
                format!("The access code isn't one for place {place_id}"),
            )),
        },
        Routing::NewReserved => {
            let id = universe.new_id();
            reserved(&id, access_code(place_id, &id))
        }
        Routing::Vip(_) => Err((
            TeleportResult::Unauthorized,
            "Rook has no private (VIP) servers".to_owned(),
        )),
    }
}

/// A running server, if it has room: the target and its `JobId`.
fn join(server: &RunningServer, joining: usize) -> Result<(Target, String), Refusal> {
    let job = server.identity.job_id.clone();
    if !server.has_room(joining) {
        return Err((
            TeleportResult::GameFull,
            format!("The server {job} is full"),
        ));
    }
    Ok((Target::Running(job.clone()), job))
}

/// A server to start: the target and its `JobId`.
fn start(identity: ServerIdentity) -> (Target, String) {
    let job = identity.job_id.clone();
    (Target::New(identity), job)
}

/// Where the user with `user_id` plays, for a server with `JobId` `asking`
/// (`GetPlayerPlaceInstanceAsync`); the error it raises when they're
/// nowhere in the game.
pub fn find_user(
    running: &[RunningServer],
    user_id: i64,
    asking: &str,
) -> Result<PlaceInstance, String> {
    running
        .iter()
        .find(|s| s.users.contains(&user_id))
        .map(|s| PlaceInstance {
            current: s.identity.job_id == asking,
            place_id: s.identity.place_id,
            job_id: s.identity.job_id.clone(),
        })
        .ok_or_else(|| format!("User {user_id} is not playing this game"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::universe::PlaceSource;

    fn universe() -> Universe {
        let mut u = Universe::new(7);
        for id in [1, 2] {
            u.add_place(id, PlaceSource::Bytes(Vec::new().into()));
        }
        u
    }

    fn server(place_id: i64, job: &str, private: &str, players: usize) -> RunningServer {
        RunningServer {
            identity: ServerIdentity {
                place_id,
                game_id: 7,
                job_id: job.into(),
                private_server_id: private.into(),
            },
            players,
            max_players: 2,
            users: Vec::new(),
        }
    }

    #[test]
    fn public_teleports_fill_a_running_server_with_room_or_start_one() {
        let u = universe();
        let running = [server(2, "full", "", 2), server(2, "open", "", 1)];
        let to = |n| route(&u, &running, 2, &Routing::Public, n).unwrap().target;
        assert_eq!(to(1), Target::Running("open".into()));
        assert!(
            matches!(to(2), Target::New(ref id) if id.place_id == 2 && id.private_server_id.is_empty())
        );
        let missing = route(&u, &running, 3, &Routing::Public, 1);
        assert_eq!(missing.unwrap_err().0, TeleportResult::GameNotFound);
    }

    #[test]
    fn instance_teleports_need_that_server_running_with_room() {
        let u = universe();
        let running = [server(2, "a", "", 2), server(2, "b", "", 0)];
        let to = |job: &str| route(&u, &running, 2, &Routing::Instance(job.into()), 1);
        assert_eq!(to("b").unwrap().target, Target::Running("b".into()));
        assert_eq!(to("a").unwrap_err().0, TeleportResult::GameFull);
        assert_eq!(to("gone").unwrap_err().0, TeleportResult::GameEnded);
    }

    #[test]
    fn access_codes_reach_their_reserved_server_in_their_place_only() {
        let u = universe();
        let running = [server(2, "r", "abc", 0)];
        let code = access_code(2, "abc");
        let found = route(&u, &running, 2, &Routing::AccessCode(code.clone()), 1).unwrap();
        assert_eq!(found.target, Target::Running("r".into()));
        assert_eq!(found.destination.access_code, code);
        assert_eq!(found.destination.private_server_id, "abc");
        let elsewhere = route(&u, &running, 1, &Routing::AccessCode(code), 1);
        assert_eq!(elsewhere.unwrap_err().0, TeleportResult::Unauthorized);
        let fresh = route(&u, &running, 2, &Routing::NewReserved, 1).unwrap();
        let Target::New(identity) = fresh.target else {
            panic!("a new server");
        };
        assert_eq!(
            identity.private_server_id,
            fresh.destination.private_server_id
        );
        assert_eq!(
            parse_access_code(&fresh.destination.access_code),
            Some((2, identity.private_server_id.as_str()))
        );
    }

    #[test]
    fn users_are_found_in_the_server_they_play_in() {
        let mut here = server(1, "here", "", 1);
        here.users.push(5);
        let mut there = server(2, "there", "", 1);
        there.users.push(6);
        let running = [here, there];
        let found = find_user(&running, 6, "here").unwrap();
        assert_eq!((found.current, found.place_id), (false, 2));
        assert!(find_user(&running, 5, "here").unwrap().current);
        assert!(find_user(&running, 9, "here").is_err());
    }
}
