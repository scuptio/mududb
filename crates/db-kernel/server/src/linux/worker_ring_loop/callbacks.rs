use super::*;

impl WorkerRingLoop {
    pub(crate) fn register_async_callback(
        &mut self,
        trigger: CallbackTrigger,
        callback: AsyncCallback,
    ) -> RS<CallbackId> {
        if let CallbackTrigger::Sequence { domain, target } = trigger {
            if let Some(frontier) = self.callback_sequence_frontiers.get(&domain).copied() {
                if frontier >= target {
                    let id = self.callback_registry.register(trigger, callback);
                    let ready = self.callback_registry.advance_sequence(domain, frontier);
                    self.spawn_ready_callbacks(ready)?;
                    return Ok(id);
                }
            }
        }
        Ok(self.callback_registry.register(trigger, callback))
    }

    pub(crate) fn cancel_async_callback(&mut self, callback_id: CallbackId) -> bool {
        self.callback_registry.cancel(callback_id)
    }

    pub(crate) fn fire_callback_event(&mut self, key: CallbackEventKey) -> RS<()> {
        let ready = self.callback_registry.fire_event(key);
        self.spawn_ready_callbacks(ready)
    }

    pub(crate) fn advance_callback_sequence(
        &mut self,
        domain: CallbackDomain,
        value: u64,
    ) -> RS<()> {
        let frontier = self.callback_sequence_frontiers.entry(domain).or_insert(0);
        if value <= *frontier {
            return Ok(());
        }
        *frontier = value;
        let ready = self.callback_registry.advance_sequence(domain, value);
        self.spawn_ready_callbacks(ready)
    }

    fn spawn_ready_callbacks(&mut self, callbacks: Vec<PendingCallback>) -> RS<()> {
        for pending in callbacks {
            let future = (pending.callback)();
            self.spawn(None, future);
        }
        Ok(())
    }
}
