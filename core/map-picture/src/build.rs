//! A map built off the frame: a build made of steps ([`Steps`]), run on a thread of its own, or
//! where there are no threads a share of each frame ([`OffFrame`]).

/// A build, a step at a time.
pub trait Steps: 'static {
    /// What the build makes.
    type Done: 'static;

    /// One step: `Some` once the build is done (or could not be).
    ///
    /// # Errors
    /// The build could not be done, and why.
    fn step(&mut self) -> Option<Result<Self::Done, String>>;

    /// Every step, on the thread the build has to itself: a build may share a step's work among
    /// the machine's threads here.
    ///
    /// # Errors
    /// As [`Self::step`].
    fn run(mut self) -> Result<Self::Done, String>
    where
        Self: Sized,
    {
        loop {
            if let Some(done) = self.step() {
                return done;
            }
        }
    }
}

/// The share of a frame a build in the frame's own thread takes.
#[cfg(target_arch = "wasm32")]
const FRAME_SHARE: std::time::Duration = std::time::Duration::from_millis(6);

/// A build under way off the frame.
pub struct OffFrame<S: Steps> {
    /// What the build is called, in what it says.
    name: &'static str,
    #[cfg(not(target_arch = "wasm32"))]
    done: std::sync::mpsc::Receiver<Result<S::Done, String>>,
    #[cfg(target_arch = "wasm32")]
    steps: S,
}

impl<S: Steps> std::fmt::Debug for OffFrame<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OffFrame({})", self.name)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<S: Steps + Send> OffFrame<S>
where
    S::Done: Send,
{
    /// Start `steps` on a thread of its own, named `name`.
    #[must_use]
    pub fn start(name: &'static str, steps: S) -> Self {
        let (tx, done) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                let _ = tx.send(steps.run());
            });
        if let Err(e) = spawned {
            tracing::warn!("the {name}'s thread did not start: {e}");
        }
        Self { name, done }
    }
}

#[cfg(target_arch = "wasm32")]
impl<S: Steps> OffFrame<S> {
    /// Start `steps`, named `name`: a share of each frame that polls it builds it.
    #[must_use]
    pub fn start(name: &'static str, steps: S) -> Self {
        Self { name, steps }
    }
}

impl<S: Steps> OffFrame<S> {
    /// What the build made, once it is done. Where there are no threads this is where a share of
    /// the frame's time builds it.
    pub fn poll(&mut self) -> Option<Result<S::Done, String>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match self.done.try_recv() {
                Ok(done) => Some(done),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err(format!("the {}'s thread stopped", self.name)))
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let until = web_time::Instant::now() + FRAME_SHARE;
            loop {
                if let Some(done) = self.steps.step() {
                    return Some(done);
                }
                if web_time::Instant::now() >= until {
                    return None;
                }
            }
        }
    }

    /// What the build made, waited for.
    ///
    /// # Errors
    /// As [`Steps::step`], or the build's thread stopped.
    pub fn wait(self) -> Result<S::Done, String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.done
                .recv()
                .unwrap_or_else(|_| Err(format!("the {}'s thread stopped", self.name)))
        }
        #[cfg(target_arch = "wasm32")]
        {
            let mut steps = self.steps;
            loop {
                if let Some(done) = steps.step() {
                    return done;
                }
            }
        }
    }
}
