use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Op;
#[derive(Debug, Clone)]
pub struct Res;

#[derive(Debug, Clone, Copy)]
pub struct Addr;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Id;

pub mod messages {
    use super::{Addr, Id, Op, Res};

    #[derive(Debug, Clone)]
    pub struct Request {
        pub client_id: Id,
        pub seq: u32,
        pub op: Op,
        pub client_addr: Addr,
    }

    #[derive(Debug, Clone)]
    pub struct Reply {
        pub client_id: Id,
        pub seq: u32,
        pub result: Res,
    }
}

pub mod typed {
    use super::{
        messages::{Reply, Request},
        Id, Res,
    };

    // states
    // ensure these types cannot be constructed (by mistakes) outside of this module

    // ClearedUpTo(seq): client will not send any request <= `seq` anymore
    #[derive(Debug, Default)]
    pub struct ClearedUpTo(u32);

    // Executed(seq, res): client may or may not have received `res` for `seq` yet
    #[derive(Debug)]
    pub struct Executed(u32, pub Res);

    // initial state: cleared up to `seq` = 0
    impl ClearedUpTo {
        pub fn new() -> Self {
            Self::default()
        }
    }

    // transitions
    // executed -> cleared: can only happen once per `seq`
    // this involves runtime check, which has been always necessary in protocol logic
    impl Executed {
        pub fn clear(&self, request: &Request) -> Option<ClearedUpTo> {
            let Self(seq, _) = self;
            if request.seq >= *seq {
                Some(ClearedUpTo(request.seq))
            } else {
                None
            }
        }

        pub fn reply(&self, id: Id) -> Reply {
            let Self(seq, result) = self;
            Reply {
                client_id: id,
                seq: *seq,
                result: result.clone(),
            }
        }
    }

    // cleared -> executed: can only happen once per `ClearedUpTo`, so can only happen (at most)
    // once per `seq`
    // it is possible to drop `ClearedUpTo` without calling `execute` i.e. liveness is not
    // guaranteed, which is expected
    impl ClearedUpTo {
        pub fn execute(self, res: Res) -> Executed {
            let Self(seq) = self;
            Executed(seq, res)
        }
    }
}

pub struct App;

impl App {
    fn execute(&mut self, op: Op, cleared: typed::ClearedUpTo) -> typed::Executed {
        let _ = op;
        cleared.execute(Res)
    }
}

// a newtype for map that does not allow evict values, so the values cannot be "reset" by a
// conflicted insertion
pub struct RestrictedMap<K, V>(BTreeMap<K, V>);

impl<K, V> RestrictedMap<K, V> {
    fn new() -> Self {
        Self(BTreeMap::new())
    }

    fn map(&mut self, k: K, f: impl FnOnce(Option<V>) -> V)
    where
        K: Ord,
    {
        let Self(m) = self;
        let v = f(m.remove(&k));
        m.insert(k, v);
    }

    fn get(&self, k: &K) -> Option<&V>
    where
        K: Ord,
    {
        let Self(m) = self;
        m.get(k)
    }
}

pub struct Server {
    executes: RestrictedMap<Id, typed::Executed>,
    app: App,
}

pub trait ServerContext {
    fn send(&mut self, addr: Addr, reply: messages::Reply);
}

impl Server {
    pub fn handle(&mut self, request: messages::Request, context: &mut impl ServerContext) {
        let client_id = request.client_id;
        let client_addr = request.client_addr;
        self.executes.map(client_id, |executed| {
            let cleared = if let Some(executed) = executed {
                if let Some(cleared) = executed.clear(&request) {
                    cleared
                } else {
                    return executed;
                }
            } else {
                typed::ClearedUpTo::new()
            };
            self.app.execute(request.op, cleared)
        });
        if let Some(executed) = self.executes.get(&client_id) {
            context.send(client_addr, executed.reply(client_id))
        }
    }
}
