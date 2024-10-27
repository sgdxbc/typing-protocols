use std::collections::BTreeMap;

use crate::{Addr, App, Id, Op};

pub mod messages {
    use std::collections::BTreeMap;

    use crate::{App, Id, Op};

    pub use crate::pb::messages::*;

    // these messages are not directly used in server implementation (which is actually located in
    // the `typed` module's state methods)
    // instead the methods of `Context` are used, and these messages are only prepared for
    // deployment
    #[derive(Debug, Clone)]
    pub struct SyncOp {
        pub sync_seq: u32,
        pub client_id: Id,
        pub op: Op,
    }

    #[derive(Debug, Clone)]
    pub struct SyncOkUpTo {
        pub sync_seq: u32,
    }

    #[derive(Debug, Clone)]
    pub struct SyncApp {
        pub app: App,
        pub replying: BTreeMap<Id, super::typed::BReplying>,
    }
}

pub trait Context {
    fn send_reply(&mut self, addr: Addr, reply: messages::Reply);
    fn sync_op(&mut self, id: Id, seq: u32, client_id: Id, op: Op);
    fn sync_op_ok(&mut self, id: Id, up_to: u32);
    fn sync_app(&mut self, id: Id, app: App, replying: BTreeMap<Id, typed::BReplying>);
    fn sync_app_ok(&mut self, id: Id);
}

mod typed {
    use std::{collections::BTreeMap, ops::Deref};

    use crate::{Addr, App, Id, Op, Res};

    use super::messages;

    // per client state machine

    // primary state machine: start Replied(0)
    // Replied(seq)->BackingUp(seq', op, addr)->Replying(seq')->Replied(seq')
    // if there is no backup server presented in the view, Replied->BackingUp will produce no side
    // effect (i.e. no `sync_op` call), and BackingUp->Replying (i.e. execution) happens immediately
    // after it
    //
    // backup state machine: start BackingUp(seq) (constructible from SyncOp)
    // BackingUp(seq)->Replying(seq)
    //
    // although both machines have BackingUp and Replying states (and the transition in between),
    // PReplying and BReplying should be distinguished because PReplying further permits further
    // transition to Replied while BReplying does not, while BReplying is directly constructible but
    // PReplying is not. consequentially PBackingUp and BBackingUp
    // also need to be distinguished because they transition to different states

    // primary
    #[derive(Debug, Clone)]
    pub struct PReplied(u32);

    #[derive(Debug, Clone)]
    pub struct PBackingUp(u32, pub Op, pub Addr);

    #[derive(Debug, Clone)]
    pub struct PReplying(u32, pub Res);

    // backup
    #[derive(Debug, Clone)]
    pub struct BBackingUp(u32, pub Op);

    #[derive(Debug, Clone)]
    pub struct BReplying(u32, pub Res);

    impl PReplied {
        pub fn new() -> Self {
            Self(0)
        }

        pub fn request(
            self,
            request: messages::Request,
            sync_seq: u32,
            backup_id: Id,
            context: &mut impl super::Context,
        ) -> PBackingUp {
            context.sync_op(backup_id, sync_seq, request.id, request.op.clone());
            PBackingUp(request.seq, request.op, request.client_addr)
        }

        pub fn request_no_sync(self, request: messages::Request) -> PBackingUp {
            PBackingUp(request.seq, request.op, request.client_addr)
        }
    }

    impl PBackingUp {
        pub fn execute(self, app: &mut App, context: &mut impl super::Context) -> PReplying {
            let Self(seq, op, addr) = self;
            let result = app.execute(op);
            let reply = messages::Reply {
                seq,
                result: result.clone(),
            };
            context.send_reply(addr, reply);
            PReplying(seq, result)
        }
    }

    impl BBackingUp {
        pub fn new(seq: u32, op: Op) -> Self {
            Self(seq, op)
        }

        // unlike PBackingUp, no side effect is made to the context on backup server
        pub fn execute(self, app: &mut App) -> BReplying {
            let Self(seq, op) = self;
            BReplying(seq, app.execute(op))
        }
    }

    impl PReplying {
        pub fn reply(self, addr: Addr, context: &mut impl super::Context) -> Self {
            let Self(seq, result) = self;
            let reply = messages::Reply {
                seq,
                result: result.clone(),
            };
            context.send_reply(addr, reply);
            PReplying(seq, result)
        }

        // utilities
        pub fn is_received(&self, seq: u32) -> bool {
            seq > self.0
        }

        pub fn is_replying(&self, seq: u32) -> bool {
            seq == self.0
        }
    }

    impl BReplying {
        pub fn promote(self) -> PReplying {
            let Self(seq, result) = self;
            PReplying(seq, result)
        }
    }

    // per server state machine i.e. the "role"
    // TODO put `app` into relevant states?

    #[derive(Debug)]
    pub struct Primary {
        pub prepared_seq: u32,
        pub committed_seq: u32,
        pub backing_up: BTreeMap<u32, PBackingUp>,
        pub replying: BTreeMap<Id, PReplying>,
    }

    #[derive(Debug)]
    pub struct Backup {
        pub committed_seq: u32,
        pub backing_up: BTreeMap<u32, BBackingUp>,
        pub replying: BTreeMap<Id, BReplying>,
    }

    #[derive(Debug)]
    pub struct Promoting {
        pub backup_id: Id, // id of the new backup; save for resending
        pub replying: BTreeMap<Id, BReplying>,
    }

    #[derive(Debug)]
    // no restriction on constructor; Idle happens to be a (and the only) valid initial state
    pub struct Idle;

    impl Backup {
        pub fn promote(
            self,
            backup_id: Id,
            app: App,
            replying: BTreeMap<Id, BReplying>,
            context: &mut impl super::Context,
        ) -> Promoting {
            context.sync_app(backup_id, app, replying);
            Promoting {
                backup_id,
                replying: self.replying,
            }
        }

        pub fn promote_no_sync(self) -> Primary {
            Primary {
                prepared_seq: 0,
                committed_seq: 0,
                backing_up: Default::default(),
                replying: self
                    .replying
                    .into_iter()
                    .map(|(id, state)| (id, state.promote()))
                    .collect(),
            }
        }
    }

    // design note: sync seq is not "inherited" between consecutive primaries
    // it always resets to 0 after backup promotions
    impl Promoting {
        pub fn promoted(self) -> Primary {
            Primary {
                prepared_seq: 0,
                committed_seq: 0,
                backing_up: Default::default(),
                replying: self
                    .replying
                    .into_iter()
                    .map(|(id, state)| (id, state.promote()))
                    .collect(),
            }
        }
    }

    impl Idle {
        // only on the first view
        pub fn start_primary(self) -> Primary {
            Primary {
                prepared_seq: 0,
                committed_seq: 0,
                backing_up: Default::default(),
                replying: Default::default(),
            }
        }

        pub fn load(
            self,
            replying: BTreeMap<Id, BReplying>,
            primary_id: Id,
            context: &mut impl super::Context,
        ) -> Backup {
            context.sync_app_ok(primary_id);
            Backup {
                committed_seq: 0,
                backing_up: Default::default(),
                replying,
            }
        }
    }
}

#[derive(Debug)]
enum RoleState {
    Idle(typed::Idle),
    Primary(typed::Primary),
    Backup(typed::Backup),
    Promoting(typed::Promoting),
}

#[derive(Debug)]
pub struct State {
    id: Id,
    role: RoleState,
    app: App,
}

impl State {
    pub fn new(id: Id, app: App) -> Self {
        Self {
            id,
            app,
            role: RoleState::Idle(typed::Idle),
        }
    }
}
