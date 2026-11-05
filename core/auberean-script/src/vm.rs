//! The machine: the linked program, the reference table, processes and their contexts, and the
//! ways in from outside (calling a function, running or resuming a context).
//!
//! A **process** owns an instance (the script object the engine object it serves is) and the
//! contexts it runs; the client runs every event an engine object receives in a new context of
//! that object's process. A context left suspended by a native stays with its process until it
//! is resumed.
//!
//! Every run draws on [`Vm::budget`], one step per instruction: a run that uses it up stops with
//! [`Stop::Budget`] instead of hanging.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::context::Context;
use crate::graph::{field, SavedState};
use crate::heap::{Heap, Object, Target};
use crate::interp;
use crate::library::STATE_BASE;
use crate::link::Program;
use crate::native::{Host, Status, Stop, Stopped};
use crate::native_object::NativeObject;
use crate::value::{tag, FunctionId, NULL};

/// A process: an instance and the contexts it has left suspended.
#[derive(Clone, Debug, Default)]
pub struct Process {
    /// The instance's reference id.
    pub instance: u32,
    pub suspended: Vec<Context>,
    next_context: u32,
}

/// The process id of a context that belongs to none (an initializer's).
pub const NO_PROCESS: usize = usize::MAX;

/// The machine.
#[derive(Debug)]
// D2-16478
pub struct Vm {
    program: Arc<Program>,
    pub heap: Heap,
    processes: Vec<Option<Process>>,
    literals: Vec<u32>,
    /// Steps left for runs; each instruction takes one.
    pub budget: u64,
    /// Steps taken since the machine was made.
    pub steps: u64,
    /// References the host holds, kept alive by every collection.
    pub held: Vec<u32>,
    /// The process of the context running now (for the native that asks for it).
    pub running_process: usize,
    /// Where the innermost run that stopped last was (a word index into the code); the caller
    /// clears it.
    pub stopped_at: Option<usize>,
    /// Every run that stopped short since the machine was made, counted by what stopped it and
    /// the code word it stopped at (the innermost run's): what the scripts still lack, for a host
    /// to report.
    pub stops: BTreeMap<(Stopped, usize), u64>,
}

impl Vm {
    /// A machine for a linked program.
    // D2-16489, D2-16426
    #[must_use]
    pub fn new(program: Arc<Program>) -> Self {
        let n = program.library.literals.len();
        Self {
            program,
            heap: Heap::new(),
            processes: Vec::new(),
            literals: vec![NULL; n],
            budget: u64::MAX,
            steps: 0,
            held: Vec::new(),
            running_process: NO_PROCESS,
            stopped_at: None,
            stops: BTreeMap::new(),
        }
    }

    /// The program.
    #[must_use]
    pub fn program(&self) -> &Arc<Program> {
        &self.program
    }

    // -----------------------------------------------------------------------------------------
    // Objects
    // -----------------------------------------------------------------------------------------

    /// A new instance of a package, as `new` makes one: a copy of the package's default instance
    /// when the data has one, else fresh slots and the package's initializer run at once.
    // D2-16365
    pub fn new_object(
        &mut self,
        host: &mut dyn Host,
        package: u32,
        prog: &Program,
    ) -> Result<u32, Stop> {
        let state = STATE_BASE | (package & 0xFFFF);
        let (id, initialize) = if let Some(s) = host.saved_state(state) {
            let id = self.heap.import(&s.graph, None)?;
            (id, false)
        } else {
            let size = prog
                .library
                .package_size(package)
                .ok_or_else(|| Stop::BadDispatch(format!("package {package:#x} has no size")))?;
            let id = self.heap.add(
                field::OBJECT,
                Target::Object(Box::new(Object::script(package, size))),
            );
            (id, true)
        };
        if id != NULL {
            let f = self.heap.field(id)?;
            self.heap
                .set_field(id, (f & !field::PROCESS) | field::LOADED)?;
            self.heap.object_mut(id)?.owner = 0;
        }
        if initialize {
            self.initialize(host, id, prog)?;
        }
        Ok(id)
    }

    /// A process instance from a saved state: its graph copied in, loaded, owned by the process.
    // D2-16419, D2-16420
    pub fn instance_from_state(
        &mut self,
        host: &mut dyn Host,
        state: u32,
        owner: u32,
    ) -> Result<u32, Stop> {
        let s = host.saved_state(state).ok_or(Stop::BadState {
            state,
            what: "not in the data".into(),
        })?;
        self.instance_from_saved(&s, state, owner)
    }

    /// [`Vm::instance_from_state`] for a state already read: state `state`, as `s`.
    pub fn instance_from_saved(
        &mut self,
        s: &SavedState,
        state: u32,
        owner: u32,
    ) -> Result<u32, Stop> {
        let id = self.heap.import(&s.graph, None)?;
        if id == NULL {
            return Err(Stop::BadState {
                state,
                what: "its root is not packed with it".into(),
            });
        }
        let f = self.heap.field(id)?;
        self.heap
            .set_field(id, f | field::LOADED | field::PROCESS)?;
        let o = self.heap.object_mut(id)?;
        o.state = state;
        if owner != 0 {
            o.owner = owner;
        }
        Ok(id)
    }

    /// A new native object of a type, as the native factory makes one.
    pub fn new_native(&mut self, ty: u32, prog: &Program) -> u32 {
        let value = NativeObject::new(&prog.library, ty);
        self.heap.add_native(ty, value)
    }

    /// The reference of string literal `n`, made on first use.
    // D2-16687
    pub fn literal(&mut self, n: u32, prog: &Program) -> Result<u32, Stop> {
        let slot = self
            .literals
            .get_mut(n as usize)
            .ok_or_else(|| Stop::Codec(format!("no string literal {n}")))?;
        if *slot == NULL {
            let units = prog.library.literals[n as usize].clone();
            *slot = self.heap.add_native(
                crate::native_object::native_type::WP_STRING,
                NativeObject::Text(units),
            );
        }
        Ok(*slot)
    }

    /// Fill a singleton from its saved state (or, when the data has none, fresh slots of the
    /// state's package and its initializer), in place, and mark it loaded.
    // D2-00977, D2-00957
    pub fn fill(&mut self, host: &mut dyn Host, id: u32, prog: &Program) -> Result<(), Stop> {
        let state = self.heap.object(id)?.state;
        let saved = host.saved_state(state);
        if let Some(s) = &saved {
            self.heap.import(&s.graph, Some(id))?;
        } else {
            let package = state & 0xFFFF;
            let size = prog
                .library
                .package_size(package)
                .ok_or_else(|| Stop::BadState {
                    state,
                    what: "no saved state and no package".into(),
                })?;
            let o = self.heap.object_mut(id)?;
            *o = Object::script(package, size);
        }
        let f = self.heap.field(id)?;
        self.heap
            .set_field(id, f | field::SINGLETON | field::LOADED)?;
        self.heap.object_mut(id)?.state = state;
        if saved.is_none() {
            self.initialize(host, id, prog)?;
        }
        Ok(())
    }

    /// Fill an object when it is a singleton not loaded yet.
    fn ensure_loaded(&mut self, host: &mut dyn Host, id: u32, prog: &Program) -> Result<(), Stop> {
        let f = self.heap.field(id)?;
        if f & field::KIND == field::OBJECT && f & field::LOADED == 0 {
            self.fill(host, id, prog)?;
        }
        Ok(())
    }

    /// The package of the object a reference names, filling it first when it is a singleton not
    /// loaded yet.
    // D2-00973
    pub fn package_of(
        &mut self,
        host: &mut dyn Host,
        id: u32,
        prog: &Program,
    ) -> Result<u32, Stop> {
        if id == NULL {
            return Err(Stop::NullReference);
        }
        let f = self.heap.field(id)?;
        if f & field::LOADED == 0 {
            if f & field::KIND == field::OBJECT {
                let state = self.heap.object(id)?.state;
                let s = self.heap.singleton(state);
                self.ensure_loaded(host, s, prog)?;
            } else {
                return Err(Stop::Heap(crate::heap::HeapError::WrongKind(id)));
            }
        }
        Ok(self.heap.object(id)?.package)
    }

    /// Run a new object's package initializer (function 0 of its package, through the object) in
    /// a context of its own, the object held as reserved until it finishes.
    // D2-00974
    pub fn initialize(
        &mut self,
        host: &mut dyn Host,
        id: u32,
        prog: &Program,
    ) -> Result<Status, Stop> {
        self.heap.begin_build();
        let mut cx = Context::new(NO_PROCESS, 0);
        cx.push(0, tag::INT)?;
        cx.push(id, tag::REFERENCE)?;
        let package = self.package_of(host, id, prog)?;
        let status = self.call_in(host, &mut cx, package, FunctionId::new(package, 0), prog)?;
        if matches!(status, Status::Terminated | Status::NoFunction) {
            self.heap.end_build();
        }
        Ok(status)
    }

    // -----------------------------------------------------------------------------------------
    // Slots reached through references
    // -----------------------------------------------------------------------------------------

    /// Where a reference plus a slot offset ends: a slot of the running context's stack, or a
    /// slot of an object (filled first when it is a singleton not loaded yet).
    // D2-16366
    pub fn place(
        &mut self,
        host: &mut dyn Host,
        r: u32,
        slot: i64,
        prog: &Program,
    ) -> Result<crate::heap::Place, Stop> {
        let (id, at) = self.heap.locate(r, slot)?;
        match id {
            None => Ok(crate::heap::Place::Stack(
                usize::try_from(at).map_err(|_| Stop::StackUnderflow)?,
            )),
            Some(id) => {
                self.ensure_loaded(host, id, prog)?;
                let n = self.heap.object(id)?.slots.len();
                let s = usize::try_from(at).ok().filter(|s| *s < n);
                let slot = s.ok_or(crate::heap::HeapError::SlotOutOfRange { id, slot: at })?;
                Ok(crate::heap::Place::Object { id, slot })
            }
        }
    }

    /// Read the slot a reference reaches.
    pub fn read_slot(
        &mut self,
        host: &mut dyn Host,
        cx: &Context,
        r: u32,
        slot: i64,
        prog: &Program,
    ) -> Result<(u32, u8), Stop> {
        match self.place(host, r, slot, prog)? {
            crate::heap::Place::Stack(i) => cx.slot(i),
            crate::heap::Place::Object { id, slot } => {
                let o = self.heap.object(id)?;
                Ok((o.slots[slot], o.tags[slot]))
            }
        }
    }

    /// Write the slot a reference reaches: its data when given, and its tag.
    #[allow(clippy::too_many_arguments)]
    pub fn write_slot(
        &mut self,
        host: &mut dyn Host,
        cx: &mut Context,
        r: u32,
        slot: i64,
        v: Option<u32>,
        t: u8,
        prog: &Program,
    ) -> Result<(), Stop> {
        match self.place(host, r, slot, prog)? {
            crate::heap::Place::Stack(i) => {
                if i >= cx.data.len() {
                    return Err(Stop::StackOverflow);
                }
                if let Some(v) = v {
                    cx.data[i] = v;
                }
                cx.tags[i] = t;
            }
            crate::heap::Place::Object { id, slot } => {
                let o = self.heap.object_mut(id)?;
                if let Some(v) = v {
                    o.slots[slot] = v;
                }
                o.tags[slot] = t;
            }
        }
        Ok(())
    }

    /// Write the data of the slot a reference reaches, leaving its tag.
    pub fn write_data(
        &mut self,
        host: &mut dyn Host,
        cx: &mut Context,
        r: u32,
        slot: i64,
        v: u32,
        prog: &Program,
    ) -> Result<(), Stop> {
        match self.place(host, r, slot, prog)? {
            crate::heap::Place::Stack(i) => {
                *cx.data.get_mut(i).ok_or(Stop::StackOverflow)? = v;
            }
            crate::heap::Place::Object { id, slot } => self.heap.object_mut(id)?.slots[slot] = v,
        }
        Ok(())
    }

    // -----------------------------------------------------------------------------------------
    // Processes and runs
    // -----------------------------------------------------------------------------------------

    /// A new process for an instance.
    // D2-16418, D2-16427
    pub fn create_process(&mut self, instance: u32) -> usize {
        self.processes.push(Some(Process {
            instance,
            suspended: Vec::new(),
            next_context: 0,
        }));
        self.processes.len() - 1
    }

    /// End a process: its suspended contexts go with it; its instance stays until nothing holds
    /// it.
    // D2-16456, D2-16412, D2-16409, D2-16424
    pub fn delete_process(&mut self, p: usize) {
        if let Some(slot) = self.processes.get_mut(p) {
            *slot = None;
        }
        // Ended processes at the end of the table leave it, so a process made and ended for
        // one call does not lengthen it.
        while matches!(self.processes.last(), Some(None)) {
            self.processes.pop();
        }
    }

    /// Every process's place in the table, in order; an empty place is a process that ended.
    #[must_use]
    pub fn process_slots(&self) -> &[Option<Process>] {
        &self.processes
    }

    /// A process by id.
    #[must_use]
    // D2-16423
    pub fn process(&self, p: usize) -> Option<&Process> {
        self.processes.get(p).and_then(Option::as_ref)
    }

    /// The process whose instance a reference is, when one is.
    #[must_use]
    pub fn process_with_instance(&self, instance: u32) -> Option<usize> {
        self.processes
            .iter()
            .position(|p| p.as_ref().is_some_and(|p| p.instance == instance))
    }

    /// A new context of a process.
    // D2-16415
    pub fn new_context(&mut self, p: usize) -> Context {
        let id = match self.processes.get_mut(p).and_then(Option::as_mut) {
            Some(proc) => {
                let id = proc.next_context;
                proc.next_context += 1;
                id
            }
            None => 0,
        };
        Context::new(p, id)
    }

    /// Keep a suspended context with its process.
    pub fn park(&mut self, cx: Context) {
        if let Some(Some(p)) = self.processes.get_mut(cx.process) {
            p.suspended.push(cx);
        }
    }

    /// Take a process's suspended context out by its id (to resume it, or to keep it again).
    // D2-08030, D2-16413
    pub fn take_suspended(&mut self, p: usize, id: u32) -> Option<Context> {
        let proc = self.processes.get_mut(p).and_then(Option::as_mut)?;
        let at = proc.suspended.iter().position(|cx| cx.id == id)?;
        Some(proc.suspended.remove(at))
    }

    /// Call a function in a context, as engine code calls into script: the caller has pushed the
    /// result slot, the arguments and the receiver; `package` is the receiver's package. A
    /// function the tables leave empty is not called, and the context then ends at once.
    // D2-00968
    pub fn call_in(
        &mut self,
        host: &mut dyn Host,
        cx: &mut Context,
        package: u32,
        function: FunctionId,
        prog: &Program,
    ) -> Result<Status, Stop> {
        let code = prog
            .library
            .function_offset(package, function)
            .map_err(Stop::BadDispatch)?;
        if let Some(offset) = code {
            cx.enter(cx.ip, (offset / 4) as usize)?;
        }
        self.run_with(host, cx, prog)
    }

    /// Call a function in a context, with the machine's own program.
    pub fn call(
        &mut self,
        host: &mut dyn Host,
        cx: &mut Context,
        package: u32,
        function: FunctionId,
    ) -> Result<Status, Stop> {
        let prog = Arc::clone(&self.program);
        self.call_in(host, cx, package, function, &prog)
    }

    /// Run a context from where it stands.
    // D2-00966
    pub fn run(&mut self, host: &mut dyn Host, cx: &mut Context) -> Result<Status, Stop> {
        let prog = Arc::clone(&self.program);
        self.run_with(host, cx, &prog)
    }

    fn run_with(
        &mut self,
        host: &mut dyn Host,
        cx: &mut Context,
        prog: &Program,
    ) -> Result<Status, Stop> {
        let outer = self.running_process;
        self.running_process = cx.process;
        let r = interp::run(self, cx, host, prog);
        self.running_process = outer;
        r
    }

    /// Resume a context a native left suspended: close the native's frame (its result already
    /// written by the engine) and run on from the instruction after the call.
    // D2-08036
    pub fn resume(&mut self, host: &mut dyn Host, cx: &mut Context) -> Result<Status, Stop> {
        cx.waiting_in = 0;
        cx.leave()?;
        self.run(host, cx)
    }

    /// The instance of the process a context belongs to.
    #[must_use]
    pub fn instance_of(&self, process: usize) -> u32 {
        self.process(process).map_or(NULL, |p| p.instance)
    }

    /// Collect garbage: everything not reachable from the processes (their instances and
    /// suspended contexts), the singletons, the literals and what the host holds is freed. Only
    /// call it when no context is running.
    // D2-16477, D2-00896, D2-00892, D2-00894
    pub fn collect(&mut self) -> usize {
        let mut roots: Vec<u32> = self.held.clone();
        roots.extend(self.literals.iter().copied().filter(|l| *l != NULL));
        for p in self.processes.iter().flatten() {
            roots.push(p.instance);
            for cx in &p.suspended {
                for i in 0..cx.sp {
                    if tag::is_reference(cx.tags[i]) {
                        roots.push(cx.data[i]);
                    }
                }
            }
        }
        self.heap.collect(roots)
    }
}
