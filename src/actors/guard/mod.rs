use std::sync::Arc;

use async_trait::async_trait;
use coerce::actor::{
    context::ActorContext,
    message::Handler,
    watch::{ActorTerminated, ActorWatch as _},
    Actor, LocalActorRef,
};
use tokio::sync::Notify;
use tracing::info;

use crate::actors::listener::actor::Listener;

pub struct GuardActor {
    target: LocalActorRef<Listener>,
    notification: Arc<Notify>,
}

impl GuardActor {
    pub fn new(target: LocalActorRef<Listener>, notification: Arc<Notify>) -> Self {
        Self {
            target,
            notification,
        }
    }
}

#[async_trait]
impl Actor for GuardActor {
    #[tracing::instrument(skip_all)]
    async fn started(&mut self, ctx: &mut ActorContext) {
        info!("GuardActor");
        self.watch(&self.target, ctx);
    }
}

#[async_trait]
impl Handler<ActorTerminated> for GuardActor {
    async fn handle(&mut self, _message: ActorTerminated, _ctx: &mut ActorContext) {
        self.notification.notify_one();
    }
}
