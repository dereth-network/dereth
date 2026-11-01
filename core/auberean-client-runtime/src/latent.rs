//! Calls that wait: a script that sleeps, and the behaviors a script waits on.
//!
//! **Sleeping.** A script's sleep leaves its call suspended for so many seconds of the client's
//! clock; when the time comes (at the world's upkeep, earliest first), the call goes on from the
//! instruction after the sleep. A call waiting in anything else is not woken by it.
//!
//! **Behaviors waited on.** A call that waits for a behavior, a release, a slider or a mode to end
//! is told the end by the object's interface: the result goes through the call's out reference,
//! its wait is closed, and the call goes on at the next upkeep. A call waiting in another native
//! is left alone. (The client's own scripts never wait so; the completions reach a script only
//! through a call that did.)

use std::sync::Arc;

use auberean_script::native::{Call, NativeFn, Status, Stop};
use auberean_script::value::{join, NativeId, NULL};

use crate::host::{how, CallResult, ClientHost, Scripts};

/// The native type of an entity description.
pub const ENTITY_DESC: u32 = 0x22;

/// The interface's sleep.
pub const SLEEP: u32 = (0x7F << 16) | 0x4F;

/// The natives a call waits in for a behavior's end, by what ends.
pub mod waits {
    const PHYSICS_OBJECT: u32 = 0x80;
    pub const BEHAVIOR: u32 = (PHYSICS_OBJECT << 16) | 0x1B;
    pub const RELEASE: u32 = (PHYSICS_OBJECT << 16) | 0x1E;
    pub const SLIDER: u32 = (PHYSICS_OBJECT << 16) | 0x1F;
    pub const MODE: u32 = (PHYSICS_OBJECT << 16) | 0x15;
}

/// The interface's functions the client calls on an object's scripts to show text and to give
/// it properties.
pub mod function {
    const INTERFACE: u32 = 0x7F;
    pub const DISPLAY_TEXT: u32 = (INTERFACE << 16) | 0x2F;
    pub const DISPLAY_STRING_INFO: u32 = (INTERFACE << 16) | 0x30;
    pub const APPLY_PROPERTIES: u32 = (INTERFACE << 16) | 0x13;
}

/// A call to go on: a process's suspended context, by id.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Waiting {
    pub process: usize,
    pub context: u32,
}

/// The sleep native: the receiver's call is left suspended for `fp[-3..-2]` seconds (a 64-bit
/// float, low word first), marked as waiting in the sleep unless it waits in something already.
// D2-04660, D2-08094
fn sleep(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let seconds = f64::from_bits(join(c.cx.at(-3)?, c.cx.at(-2)?));
    if c.cx.waiting_in == 0 {
        c.cx.waiting_in = SLEEP;
    }
    let waiting = Waiting {
        process: c.cx.process,
        context: c.cx.id,
    };
    let h = c
        .host
        .as_any()
        .downcast_mut::<ClientHost>()
        .ok_or_else(|| Stop::Host("not the client's host".into()))?;
    h.sleeps.push((waiting, seconds));
    Ok(Status::Interrupted)
}

/// This module's natives, by id.
pub fn natives() -> impl Iterator<Item = (NativeId, NativeFn)> {
    [(NativeId(SLEEP), sleep as NativeFn)].into_iter()
}

impl Scripts {
    /// One of the interface's functions called on an object's scripts (see
    /// [`crate::host::call_on_process`]): how it went and the result slot; `None` when the object
    /// is not here.
    pub fn call_object(
        &mut self,
        object: u64,
        function: u32,
        args: &[crate::InterfaceArg],
    ) -> Option<(CallResult, u32)> {
        self.host.model.object(object)?;
        self.vm.budget = crate::host::BUDGET;
        let mut result = 0;
        let outcome = self.process_of(object).and_then(|p| {
            let (s, r) =
                crate::host::call_on_process(&mut self.vm, &mut self.host, p, function, args)?;
            result = r;
            Ok(s)
        });
        self.vm.heap.abandon_builds();
        self.vm.stopped_at = None;
        Some((how(outcome), result))
    }

    /// A sleeping call's time came: when it still waits in the sleep, its sleep is closed and it
    /// goes on. `None` when there is no such call.
    // D2-08084, D2-04657, D2-08036
    pub fn wake(&mut self, w: Waiting) -> Option<CallResult> {
        let mut cx = self.vm.take_suspended(w.process, w.context)?;
        if cx.waiting_in != SLEEP {
            self.vm.park(cx);
            return None;
        }
        self.vm.budget = crate::host::BUDGET;
        let outcome = self.vm.resume(&mut self.host, &mut cx);
        if outcome == Ok(Status::Interrupted) {
            self.vm.park(cx);
        }
        self.vm.heap.abandon_builds();
        self.vm.stopped_at = None;
        Some(how(outcome))
    }

    /// A behavior, release, slider or mode a call waits in (`native`) ended with `result`: the
    /// result goes through the call's out reference and its wait is closed; the call goes on at
    /// the next upkeep (the one to hand back). `None` when no call of the object waits in it.
    // D2-08066, D2-08067, D2-08068, D2-08069, D2-04765, D2-04775, D2-04768, D2-04767, D2-08093
    pub fn complete(
        &mut self,
        object: u64,
        context: u32,
        native: u32,
        result: u32,
    ) -> Option<Waiting> {
        let p = self.process_of(object).ok()?;
        let mut cx = self.vm.take_suspended(p, context)?;
        if cx.waiting_in != native {
            self.vm.park(cx);
            return None;
        }
        cx.waiting_in = 0;
        let prog = Arc::clone(&self.host.program);
        let written = cx.at(-3).and_then(|out| {
            self.vm
                .write_data(&mut self.host, &mut cx, out, 0, result, &prog)
        });
        let closed = written.and_then(|()| cx.leave());
        self.vm.park(cx);
        closed.ok().map(|()| Waiting {
            process: p,
            context,
        })
    }

    /// A call whose wait was closed goes on from where it stands.
    // D2-08080, D2-08036
    pub fn go_on(&mut self, w: Waiting) -> Option<CallResult> {
        let mut cx = self.vm.take_suspended(w.process, w.context)?;
        self.vm.budget = crate::host::BUDGET;
        let outcome = self.vm.run(&mut self.host, &mut cx);
        if outcome == Ok(Status::Interrupted) {
            self.vm.park(cx);
        }
        self.vm.heap.abandon_builds();
        self.vm.stopped_at = None;
        Some(how(outcome))
    }
}
