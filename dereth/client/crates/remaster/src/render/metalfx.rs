//! MetalFX's temporal scaler on a device that runs over Metal. Metal's work cannot be recorded into
//! a Vulkan command buffer, so a frame it scales goes in two submissions: what comes before it in a
//! submission of its own, then the scaler in a Metal command buffer on the same queue, waiting for
//! that submission, and what follows in the frame's command buffer, which reaches the queue last.

#![allow(unsafe_code)]

use std::sync::atomic::{AtomicU64, Ordering};

use dereth_gfx::{Device, Texture};
use objc2::msg_send;
use objc2::rc::autoreleasepool;
use objc2::runtime::{AnyClass, AnyObject, Bool};

type Id = *mut AnyObject;

#[link(name = "MetalFX", kind = "framework")]
extern "C" {}

/// The last finished scaling's time on the device, in microseconds.
static LAST_MICROS: AtomicU64 = AtomicU64::new(0);

/// The last finished scaling's time on the device, in milliseconds.
#[must_use]
pub fn last_ms() -> f64 {
    #[allow(clippy::cast_precision_loss)] // a short time
    let us = LAST_MICROS.load(Ordering::Relaxed) as f64;
    us / 1000.0
}

/// What the scaler reads and writes.
#[derive(Debug, Clone, Copy)]
pub struct Images {
    pub color: Texture,
    /// One depth a pixel, as a color.
    pub depth: Texture,
    /// Where each pixel's surface was last frame less where it is, in pixels of the picture read.
    pub motion: Texture,
    pub output: Texture,
    /// The size of what it reads.
    pub input: (u32, u32),
    /// The size of what it writes.
    pub size: (u32, u32),
}

/// The Metal objects of the images, or why there are none.
fn metal_images(gfx: &Device, images: &Images) -> Result<[Id; 4], dereth_gfx::Error> {
    let mut out = [std::ptr::null_mut(); 4];
    for (o, t) in out
        .iter_mut()
        .zip([images.color, images.depth, images.motion, images.output])
    {
        *o = gfx.metal_texture(t)?.cast::<AnyObject>();
        if o.is_null() {
            return Err(dereth_gfx::Error::Unsupported("a Metal texture".to_owned()));
        }
    }
    Ok(out)
}

/// The temporal scaler.
#[derive(Debug)]
pub struct Scaler {
    raw: Id,
    queue: Id,
    event: Id,
    /// The sizes it was made for: its descriptor names them.
    input: (u32, u32),
    size: (u32, u32),
    /// The last command buffer, until it has been read.
    last: Id,
}

impl Scaler {
    /// A scaler for `images`, or why there is none.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(gfx: &Device, images: &Images) -> Result<Self, dereth_gfx::Error> {
        let no = |what: &str| dereth_gfx::Error::Unsupported(what.to_owned());
        let objects = gfx.metal_objects().ok_or_else(|| no("Metal objects"))?;
        let device: Id = objects.device.cast();
        let [color, depth, motion, output] = metal_images(gfx, images)?;
        let class = AnyClass::get(c"MTLFXTemporalScalerDescriptor").ok_or_else(|| no("MetalFX"))?;
        // SAFETY: every object is live; the messages are those of the classes' headers.
        let raw: Id = autoreleasepool(|_| unsafe {
            let format = |t: Id| -> usize { msg_send![t, pixelFormat] };
            let made: Id = msg_send![class, alloc];
            let d: Id = msg_send![made, init];
            let _: () = msg_send![d, setColorTextureFormat: format(color)];
            let _: () = msg_send![d, setDepthTextureFormat: format(depth)];
            let _: () = msg_send![d, setMotionTextureFormat: format(motion)];
            let _: () = msg_send![d, setOutputTextureFormat: format(output)];
            let _: () = msg_send![d, setInputWidth: images.input.0 as usize];
            let _: () = msg_send![d, setInputHeight: images.input.1 as usize];
            let _: () = msg_send![d, setOutputWidth: images.size.0 as usize];
            let _: () = msg_send![d, setOutputHeight: images.size.1 as usize];
            let raw: Id = msg_send![d, newTemporalScalerWithDevice: device];
            let _: () = msg_send![d, release];
            if !raw.is_null() {
                let _: () = msg_send![raw, setInputContentWidth: images.input.0 as usize];
                let _: () = msg_send![raw, setInputContentHeight: images.input.1 as usize];
                let _: () = msg_send![raw, setMotionVectorScaleX: 1.0f32];
                let _: () = msg_send![raw, setMotionVectorScaleY: 1.0f32];
                let _: () = msg_send![raw, setDepthReversed: Bool::NO];
            }
            raw
        });
        if raw.is_null() {
            return Err(no("MetalFX's temporal scaler"));
        }
        Ok(Self {
            raw,
            queue: objects.queue.cast(),
            event: objects.timeline.cast(),
            input: images.input,
            size: images.size,
            last: std::ptr::null_mut(),
        })
    }

    /// Whether it scales pictures of these sizes.
    #[must_use]
    pub fn fits(&self, images: &Images) -> bool {
        self.input == images.input && self.size == images.size
    }

    /// The last finished command buffer's time on the device, if it has finished.
    fn read_last(&mut self) {
        if self.last.is_null() {
            return;
        }
        // SAFETY: the buffer is retained.
        unsafe {
            let status: usize = msg_send![self.last, status];
            if status >= 4 {
                let start: f64 = msg_send![self.last, GPUStartTime];
                let end: f64 = msg_send![self.last, GPUEndTime];
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                LAST_MICROS.store(((end - start) * 1e6) as u64, Ordering::Relaxed);
            }
            let _: () = msg_send![self.last, release];
        }
        self.last = std::ptr::null_mut();
    }

    /// Scale this frame after timeline value `wait`: the projection moved `jitter` render pixels,
    /// `reset` when nothing before it counts.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn encode(
        &mut self,
        gfx: &Device,
        images: &Images,
        jitter: [f32; 2],
        reset: bool,
        wait: u64,
    ) -> Result<(), dereth_gfx::Error> {
        self.read_last();
        let [color, depth, motion, output] = metal_images(gfx, images)?;
        autoreleasepool(|_| {
            // SAFETY: every object is live; the messages are those of the headers; the buffer is
            // kept past its pool by the retain.
            unsafe {
                let cb: Id = msg_send![self.queue, commandBuffer];
                let _: Id = msg_send![cb, retain];
                let _: () = msg_send![cb, encodeWaitForEvent: self.event, value: wait];
                let _: () = msg_send![self.raw, setColorTexture: color];
                let _: () = msg_send![self.raw, setDepthTexture: depth];
                let _: () = msg_send![self.raw, setMotionTexture: motion];
                let _: () = msg_send![self.raw, setOutputTexture: output];
                let _: () = msg_send![self.raw, setReset: Bool::new(reset)];
                // Its x runs the other way from the projection's move: this way round it gathers
                // the sharpest picture of the four.
                let _: () = msg_send![self.raw, setJitterOffsetX: -jitter[0]];
                let _: () = msg_send![self.raw, setJitterOffsetY: jitter[1]];
                let _: () = msg_send![self.raw, encodeToCommandBuffer: cb];
                let _: () = msg_send![cb, commit];
                self.last = cb;
            }
        });
        Ok(())
    }
}

impl Drop for Scaler {
    fn drop(&mut self) {
        // SAFETY: the scaler and the buffer are this one's to release.
        unsafe {
            if !self.last.is_null() {
                let _: () = msg_send![self.last, release];
            }
            let _: () = msg_send![self.raw, release];
        }
    }
}
