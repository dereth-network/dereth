//! A context: one running script call chain, with its slot stack and call stack.
//!
//! The stack holds 1,536 slots, each a 32-bit word and a tag byte. The call stack holds 192
//! frames of a return position and a frame pointer; the first is the base, and the machine ends
//! the context when a return brings it back there. A function's frame starts at `fp`: its locals
//! at `fp[0..]`, the receiver at `fp[-1]`, its parameters at `fp[-2]`, `fp[-3]`... and the slot its
//! caller reserved for the result below them.
//!
//! The client checks none of these bounds; this machine checks all of them and stops the run
//! ([`crate::native::Stop`]) instead of reading or writing past them.

use crate::native::Stop;
use crate::value::{join, split, tag};

/// Slots in a context's stack.
pub const STACK_SLOTS: usize = 1536;

/// Frames in a context's call stack, the base included.
pub const CALL_FRAMES: usize = 192;

/// One call frame: where to resume (a word index into the code) and the frame pointer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Frame {
    pub ip: usize,
    pub fp: usize,
}

/// One context.
#[derive(Clone, Debug)]
pub struct Context {
    pub data: Vec<u32>,
    pub tags: Vec<u8>,
    /// The next free slot.
    pub sp: usize,
    /// The current function's first local.
    pub fp: usize,
    /// The next instruction (a word index into the code).
    pub ip: usize,
    pub frames: Vec<Frame>,
    /// The current frame (0 is the base).
    pub sf: usize,
    /// The context's id within its process.
    pub id: u32,
    /// The process it belongs to.
    pub process: usize,
    /// The native a suspended context is waiting in (0 when it waits in none).
    pub waiting_in: u32,
}

impl Context {
    /// A fresh context: an empty stack, the base frame current.
    // D2-00958
    #[must_use]
    pub fn new(process: usize, id: u32) -> Self {
        Self {
            data: vec![0; STACK_SLOTS],
            tags: vec![tag::UNDEFINED; STACK_SLOTS],
            sp: 0,
            fp: 0,
            ip: 0,
            frames: vec![Frame::default(); CALL_FRAMES],
            sf: 0,
            id,
            process,
            waiting_in: 0,
        }
    }

    /// Push a value with its tag.
    #[inline]
    pub fn push(&mut self, v: u32, t: u8) -> Result<(), Stop> {
        if self.sp >= STACK_SLOTS {
            return Err(Stop::StackOverflow);
        }
        self.data[self.sp] = v;
        self.tags[self.sp] = t;
        self.sp += 1;
        Ok(())
    }

    /// Push a 64-bit value, low word first, both slots with one tag.
    // D2-01003, D2-16269
    #[inline]
    pub fn push_long(&mut self, v: u64, t: u8) -> Result<(), Stop> {
        let (lo, hi) = split(v);
        self.push(lo, t)?;
        self.push(hi, t)
    }

    /// Pop a value.
    #[inline]
    pub fn pop(&mut self) -> Result<u32, Stop> {
        if self.sp == 0 {
            return Err(Stop::StackUnderflow);
        }
        self.sp -= 1;
        Ok(self.data[self.sp])
    }

    /// Pop a 64-bit value (high word on top).
    #[inline]
    pub fn pop_long(&mut self) -> Result<u64, Stop> {
        let hi = self.pop()?;
        let lo = self.pop()?;
        Ok(join(lo, hi))
    }

    /// Drop `n` slots.
    #[inline]
    pub fn drop_slots(&mut self, n: usize) -> Result<(), Stop> {
        if n > self.sp {
            return Err(Stop::StackUnderflow);
        }
        self.sp -= n;
        Ok(())
    }

    /// The absolute index of slot `k` from the frame pointer.
    #[inline]
    pub fn frame_index(&self, k: i64) -> Result<usize, Stop> {
        let i = i64::try_from(self.fp).map_err(|_| Stop::StackOverflow)? + k;
        if i < 0 {
            return Err(Stop::StackUnderflow);
        }
        let i = usize::try_from(i).map_err(|_| Stop::StackOverflow)?;
        if i >= STACK_SLOTS {
            return Err(Stop::StackOverflow);
        }
        Ok(i)
    }

    /// The slot `k` from the frame pointer.
    #[inline]
    pub fn at(&self, k: i64) -> Result<u32, Stop> {
        Ok(self.data[self.frame_index(k)?])
    }

    /// Write slot `k` from the frame pointer, with its tag.
    #[inline]
    pub fn set(&mut self, k: i64, v: u32, t: u8) -> Result<(), Stop> {
        let i = self.frame_index(k)?;
        self.data[i] = v;
        self.tags[i] = t;
        Ok(())
    }

    /// The slot `k` below the top (0 is the top).
    #[inline]
    pub fn peek(&self, k: usize) -> Result<u32, Stop> {
        if k >= self.sp {
            return Err(Stop::StackUnderflow);
        }
        Ok(self.data[self.sp - 1 - k])
    }

    /// A slot of the stack by absolute index.
    pub fn slot(&self, i: usize) -> Result<(u32, u8), Stop> {
        if i >= STACK_SLOTS {
            return Err(Stop::StackOverflow);
        }
        Ok((self.data[i], self.tags[i]))
    }

    /// Enter a call: record where the caller resumes, open a new frame at `ip`.
    #[inline]
    pub fn enter(&mut self, return_ip: usize, ip: usize) -> Result<(), Stop> {
        if self.sf + 1 >= CALL_FRAMES {
            return Err(Stop::CallStackOverflow);
        }
        self.frames[self.sf].ip = return_ip;
        self.sf += 1;
        self.frames[self.sf] = Frame { ip, fp: 0 };
        self.ip = ip;
        self.fp = 0;
        Ok(())
    }

    /// Leave the current frame: back to the caller's frame pointer and resume position.
    #[inline]
    pub fn leave(&mut self) -> Result<(), Stop> {
        if self.sf == 0 {
            return Err(Stop::CallStackUnderflow);
        }
        self.sp = self.fp;
        self.frames[self.sf] = Frame::default();
        self.sf -= 1;
        self.fp = self.frames[self.sf].fp;
        self.ip = self.frames[self.sf].ip;
        Ok(())
    }

    /// Whether the context is back at its base frame (its call is over).
    #[must_use]
    pub fn at_base(&self) -> bool {
        self.sf == 0
    }
}
