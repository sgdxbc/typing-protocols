use std::collections::BTreeMap;

use crate::{Addr, App, Id, Op};

pub mod messages {
    use std::collections::BTreeMap;

    use crate::{App, Id};

    pub use crate::pb::messages::*;

    // these messages are not directly used in server implementation (which is actually located in
    // the `typed` module's state methods)
    // instead the methods of `Context` are used, and these messages are only prepared for
    // deployment
    #[derive(Debug, Clone)]
    pub struct SyncRequest {
        pub seq: u32,
        pub request: Request,
    }

    #[derive(Debug, Clone)]
    pub struct SyncOkUpTo {
        pub seq: u32,
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
    // backup state machine: start Replied(0)
    // Replied(seq)->Replying(seq')->Replied(seq')
    //
    // the two machines disagree on permitted transitions, so two sets of states are instantiated
    // to be specific, PReplied->Replying is not permitted while BReplied->Replying is permitted
    // consequentially, PReplying and BReplying must also be distinguished because
    // PReplying->PReplied while BReplying->BReplied

    // primary
    #[derive(Debug, Clone)]
    pub struct PReplied(u32);

    #[derive(Debug, Clone)]
    pub struct BackingUp(u32, pub Op, pub Addr);

    pub struct PReplying(Replying);

    // backup
    #[derive(Debug, Clone)]
    pub struct BReplied(u32);

    #[derive(Debug, Clone)]
    pub struct BReplying(Replying);

    // shared
    #[derive(Debug, Clone)]
    pub struct Replying(u32, pub Res);

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
        ) -> BackingUp {
            context.sync_op(backup_id, sync_seq, request.id, request.op.clone());
            BackingUp(request.seq, request.op, request.client_addr)
        }

        pub fn request_no_sync(self, request: messages::Request) -> BackingUp {
            BackingUp(request.seq, request.op, request.client_addr)
        }
    }

    impl BackingUp {
        pub fn execute(self, app: &mut App, context: &mut impl super::Context) -> PReplying {
            let Self(seq, op, addr) = self;
            let result = app.execute(op);
            let reply = messages::Reply {
                seq,
                result: result.clone(),
            };
            context.send_reply(addr, reply);
            PReplying(Replying(seq, result))
        }
    }

    impl BReplied {
        pub fn new() -> Self {
            Self(0)
        }

        pub fn execute(self, seq: u32, op: Op, app: &mut App) -> BReplying {
            BReplying(Replying(seq, app.execute(op)))
        }
    }

    impl Replying {
        pub fn reply(self, addr: Addr, context: &mut impl super::Context) -> Self {
            let Self(seq, result) = self;
            let reply = messages::Reply {
                seq,
                result: result.clone(),
            };
            context.send_reply(addr, reply);
            Replying(seq, result)
        }

        // utilities
        pub fn is_received(&self, seq: u32) -> bool {
            seq > self.0
        }

        pub fn is_replying(&self, seq: u32) -> bool {
            seq == self.0
        }
    }

    impl PReplying {
        pub fn received(self) -> PReplied {
            PReplied(self.0 .0)
        }

        pub fn reply(self, addr: Addr, context: &mut impl super::Context) -> Self {
            Self(self.0.reply(addr, context))
        }
    }

    impl Deref for PReplying {
        type Target = Replying;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl BReplying {
        pub fn received(self) -> BReplied {
            BReplied(self.0 .0)
        }

        pub fn reply(self, addr: Addr, context: &mut impl super::Context) -> Self {
            Self(self.0.reply(addr, context))
        }

        pub fn promote(self) -> PReplying {
            PReplying(self.0)
        }
    }

    impl Deref for BReplying {
        type Target = Replying;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    #[derive(Debug)]
    pub struct Primary;
    // pub struct Primary {
    //     pub prepared_seq: u32,
    //     committed_seq: u32,
    // }

    #[derive(Debug)]
    pub struct Backup;
    // pub struct Backup(u32);

    #[derive(Debug)]
    pub struct Promoting(Id); // id of the new backup; save for resending

    #[derive(Debug)]
    // no restriction on constructor; Idle happens to be a (and the only) valid initial state
    pub struct Idle;

    impl Primary {
        // pub fn prepare(self) -> Self {
        //     Self {
        //         prepared_seq: self.prepared_seq + 1,
        //         committed_seq: self.committed_seq,
        //     }
        // }

        // pub fn commit(self) -> Self {
        //     Self {
        //         prepared_seq: self.prepared_seq,
        //         committed_seq: self.upcoming_commit(),
        //     }
        // }

        // // util
        // pub fn upcoming_commit(&self) -> u32 {
        //     self.committed_seq + 1
        // }
    }

    impl Backup {
        // pub fn commit(self) -> Self {
        //     Self(self.upcoming_commit())
        // }

        pub fn promote(
            self,
            backup_id: Id,
            app: App,
            replying: BTreeMap<Id, BReplying>,
            context: &mut impl super::Context,
        ) -> Promoting {
            context.sync_app(backup_id, app, replying);
            Promoting(backup_id)
        }

        pub fn promote_no_sync(self) -> Primary {
            Primary
            // Primary {
            //     prepared_seq: 0,
            //     committed_seq: 0,
            // }
        }

        // util
        // pub fn upcoming_commit(&self) -> u32 {
        //     self.0 + 1
        // }
    }

    // design note: sync seq is not "inherited" between consecutive primaries
    // it always resets to 0 after backup promotions
    impl Promoting {
        pub fn promoted(self) -> Primary {
            Primary
            // Primary {
            //     prepared_seq: 0,
            //     committed_seq: 0,
            // }
        }
    }

    impl Idle {
        pub fn start_primary(self) -> Primary {
            Primary
            // Primary {
            //     prepared_seq: 0,
            //     committed_seq: 0,
            // }
        }

        pub fn load(self, primary_id: Id, context: &mut impl super::Context) -> Backup {
            context.sync_app_ok(primary_id);
            Backup
            // Backup(0)
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
    role: RoleState,
}
