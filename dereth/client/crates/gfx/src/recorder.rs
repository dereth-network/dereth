//! Recording: passes declared with what they touch, the barriers the graph places in front of
//! each, dynamic rendering for the raster passes, and the draws, dispatches and copies inside.

use ash::vk;

use crate::device::{vkr, Device, FrameClock, Garbage, Retire, Ticket};
use crate::graph::Use;
use crate::pipeline::PUSH_BYTES;
use crate::query::{Slot, MARKS};
use crate::upload::{buffer_barrier, image_barrier};
use crate::{Buffer, Error, IndexType, Pipeline, Rect, Texture};

/// What a colour target holds when a pass begins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Load {
    /// What it held.
    Keep,
    /// This colour.
    Clear([f32; 4]),
    /// Anything: the pass covers every pixel it keeps.
    Discard,
}

/// One colour target of a raster pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorAttachment {
    /// The texture drawn into.
    pub texture: Texture,
    /// What it holds when the pass begins.
    pub load: Load,
}

/// The depth target of a raster pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DepthAttachment {
    /// The texture.
    pub texture: Texture,
    /// The depth it is cleared to when the pass begins, or `None` to keep what it held.
    pub clear: Option<f32>,
    /// Whether the pass writes it; otherwise it only tests against it.
    pub write: bool,
}

/// A resource a pass touches besides its targets, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Touch {
    /// A texture.
    Texture(Texture, Use),
    /// A buffer.
    Buffer(Buffer, Use),
}

/// A raster pass.
#[derive(Debug, Clone, Copy)]
pub struct RasterPass<'a> {
    /// Its name.
    pub name: &'a str,
    /// Its colour targets.
    pub color: &'a [ColorAttachment],
    /// Its depth target.
    pub depth: Option<DepthAttachment>,
    /// What else it reads or writes.
    pub touches: &'a [Touch],
    /// The part of the targets it draws in: all of the first target when `None`.
    pub area: Option<Rect>,
}

/// One recording: a command buffer of the device's own, or one an adopted device's owner lent.
#[derive(Debug)]
pub struct Recorder<'d> {
    dev: &'d mut Device,
    cmd: vk::CommandBuffer,
    at: Retire,
    /// Whether the command buffer is the device's own, to submit.
    own: bool,
    timer: Option<Slot>,
    done: bool,
}

impl Device {
    /// Begin a recording in a command buffer of the device's own.
    ///
    /// # Errors
    /// Any failure of the driver.
    pub fn begin(&mut self) -> Result<Recorder<'_>, Error> {
        self.collect();
        let cmd = match self.free_commands.pop() {
            Some(c) => c,
            None => {
                let info = vk::CommandBufferAllocateInfo::default()
                    .command_pool(self.pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1);
                // SAFETY: the pool is the device's own; the buffer is freed with it.
                vkr("vkAllocateCommandBuffers", unsafe {
                    self.device.allocate_command_buffers(&info)
                })?[0]
            }
        };
        let begin = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        // SAFETY: the buffer is free (never submitted, or retired); beginning resets it.
        vkr("vkBeginCommandBuffer", unsafe {
            self.device.begin_command_buffer(cmd, &begin)
        })?;
        let value = self.signal_next();
        let mut r = Recorder {
            dev: self,
            cmd,
            at: Retire { value, frame: -1 },
            own: true,
            timer: None,
            done: false,
        };
        r.start()?;
        Ok(r)
    }

    /// Record into `cmd`, a command buffer an adopted device's owner lent for its frame `clock`
    /// names, outside any rendering. The owner submits it; the layer neither ends nor submits it.
    ///
    /// # Safety
    /// `cmd` must be recording, on the adopted device, outside a render pass, and submitted by
    /// its owner after the recording is finished. Every frame up to `clock.stamp -
    /// clock.in_flight` must have finished on the device.
    ///
    /// # Errors
    /// Any failure of the driver.
    pub unsafe fn record_into(
        &mut self,
        cmd: vk::CommandBuffer,
        clock: FrameClock,
    ) -> Result<Recorder<'_>, Error> {
        let stamp = i64::try_from(clock.stamp).unwrap_or(i64::MAX);
        let in_flight = i64::try_from(clock.in_flight).unwrap_or(i64::MAX);
        self.finished_frame = self.finished_frame.max(stamp.saturating_sub(in_flight));
        self.last_frame = self.last_frame.max(stamp);
        self.collect();
        let mut r = Recorder {
            dev: self,
            cmd,
            at: Retire {
                value: 0,
                frame: stamp,
            },
            own: false,
            timer: None,
            done: false,
        };
        r.start()?;
        Ok(r)
    }

    /// One level of `t`, read back: tightly packed rows. Submits and waits for a recording of
    /// its own.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a texture the device does not hold; any failure of the driver.
    pub fn read_texture(&mut self, t: Texture, level: u32) -> Result<Vec<u8>, Error> {
        let e = self.texture(t)?;
        let (w, h) = ((e.width >> level).max(1), (e.height >> level).max(1));
        let bytes = e.format.level_bytes(w, h);
        let readback = self.create_buffer(&crate::BufferDesc {
            label: "read back",
            size: bytes,
            usage: crate::BufferUsage::TRANSFER_DST,
            location: crate::Location::Readback,
        })?;
        let result = (|| {
            let mut r = self.begin()?;
            r.copy_texture_to_buffer(t, level, readback, 0)?;
            r.touch(&[Touch::Buffer(readback, Use::HostRead)])?;
            let ticket = r.submit()?;
            self.wait(ticket)?;
            self.read_buffer(readback, 0, bytes)
        })();
        self.destroy_buffer(readback);
        result
    }

    /// The spans the last finished recording's marks measured.
    #[must_use]
    pub fn timings(&mut self) -> Vec<crate::Timing> {
        self.collect();
        self.timers.last.clone()
    }
}

impl Recorder<'_> {
    /// The marks' pool, then the staged copies.
    fn start(&mut self) -> Result<(), Error> {
        if self.dev.caps.timestamps {
            let pool = self.dev.timers.take(&self.dev.device)?;
            // SAFETY: recording outside any rendering; the pool is free.
            unsafe {
                self.dev
                    .device
                    .cmd_reset_query_pool(self.cmd, pool, 0, MARKS)
            };
            self.timer = Some(Slot {
                pool,
                labels: Vec::new(),
            });
        }
        self.dev.flush_uploads(self.cmd, self.at);
        self.dev.build_pending(self.cmd, self.at);
        Ok(())
    }

    /// The command buffer being recorded, for what the layer does not wrap.
    #[must_use]
    pub fn raw(&self) -> vk::CommandBuffer {
        self.cmd
    }

    /// The device recorded on.
    #[must_use]
    pub fn device(&mut self) -> &mut Device {
        self.dev
    }

    /// Write a mark named `label`: the span from it to the next mark is measured. Nothing on a
    /// device whose queue writes no timestamps, or past [`MARKS`] marks.
    pub fn timestamp(&mut self, label: &'static str) {
        let Some(t) = self.timer.as_mut() else {
            return;
        };
        #[allow(clippy::cast_possible_truncation)] // at most `MARKS`
        let index = t.labels.len() as u32;
        if index >= MARKS {
            return;
        }
        // SAFETY: recording; the query was reset at the recording's head.
        unsafe {
            self.dev.device.cmd_write_timestamp2(
                self.cmd,
                vk::PipelineStageFlags2::ALL_COMMANDS,
                t.pool,
                index,
            );
        }
        t.labels.push(label);
    }

    /// Put in the barriers `touches` need, now.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a resource the device does not hold.
    pub fn touch(&mut self, touches: &[Touch]) -> Result<(), Error> {
        let mut images = Vec::new();
        let mut buffers = Vec::new();
        for t in touches {
            self.track(*t, &mut images, &mut buffers)?;
        }
        self.barriers(&images, &buffers);
        Ok(())
    }

    fn track(
        &mut self,
        t: Touch,
        images: &mut Vec<vk::ImageMemoryBarrier2<'static>>,
        buffers: &mut Vec<vk::BufferMemoryBarrier2<'static>>,
    ) -> Result<(), Error> {
        match t {
            Touch::Texture(tex, u) => {
                let e = self
                    .dev
                    .textures
                    .get_mut(tex.0)
                    .ok_or_else(|| Error::Invalid(format!("no texture {}", tex.0)))?;
                if let Some(b) = e.track.to(u, true) {
                    images.push(image_barrier(b, e.image, e.format.aspect(), e.mips));
                }
            }
            Touch::Buffer(buf, u) => {
                let e = self
                    .dev
                    .buffers
                    .get_mut(buf.0)
                    .ok_or_else(|| Error::Invalid(format!("no buffer {}", buf.0)))?;
                if let Some(b) = e.track.to(u, false) {
                    buffers.push(buffer_barrier(b, e.buffer));
                }
            }
        }
        Ok(())
    }

    fn barriers(
        &self,
        images: &[vk::ImageMemoryBarrier2<'_>],
        buffers: &[vk::BufferMemoryBarrier2<'_>],
    ) {
        if images.is_empty() && buffers.is_empty() {
            return;
        }
        let dep = vk::DependencyInfo::default()
            .image_memory_barriers(images)
            .buffer_memory_barriers(buffers);
        // SAFETY: recording outside any rendering; every resource is live.
        unsafe { self.dev.device.cmd_pipeline_barrier2(self.cmd, &dep) };
    }

    /// Record a raster pass: its barriers, its targets begun as `pass` says, `draw` inside, then
    /// its targets ended.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a pass with no target, or a resource the device does not hold.
    pub fn raster(
        &mut self,
        pass: &RasterPass<'_>,
        draw: impl FnOnce(&mut crate::front::Draw<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mut images = Vec::new();
        let mut buffers = Vec::new();
        for c in pass.color {
            self.track(
                Touch::Texture(c.texture, Use::ColorTarget),
                &mut images,
                &mut buffers,
            )?;
        }
        if let Some(d) = pass.depth {
            let u = if d.write {
                Use::DepthTarget
            } else {
                Use::DepthTest
            };
            self.track(Touch::Texture(d.texture, u), &mut images, &mut buffers)?;
        }
        for t in pass.touches {
            self.track(*t, &mut images, &mut buffers)?;
        }
        self.barriers(&images, &buffers);
        let first = pass
            .color
            .first()
            .map(|c| c.texture)
            .or(pass.depth.map(|d| d.texture))
            .ok_or_else(|| Error::Invalid(format!("the pass {} has no target", pass.name)))?;
        let (w, h) = self.dev.texture_size(first)?;
        let area = pass.area.unwrap_or(Rect {
            x: 0,
            y: 0,
            width: w,
            height: h,
        });
        let mut colors = Vec::with_capacity(pass.color.len());
        for c in pass.color {
            let e = self.dev.texture(c.texture)?;
            let (load, clear) = match c.load {
                Load::Keep => (vk::AttachmentLoadOp::LOAD, [0.0; 4]),
                Load::Clear(v) => (vk::AttachmentLoadOp::CLEAR, v),
                Load::Discard => (vk::AttachmentLoadOp::DONT_CARE, [0.0; 4]),
            };
            colors.push(
                vk::RenderingAttachmentInfo::default()
                    .image_view(e.view)
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(load)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .clear_value(vk::ClearValue {
                        color: vk::ClearColorValue { float32: clear },
                    }),
            );
        }
        let depth = match pass.depth {
            Some(d) => {
                let e = self.dev.texture(d.texture)?;
                Some(
                    vk::RenderingAttachmentInfo::default()
                        .image_view(e.view)
                        .image_layout(if d.write {
                            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
                        } else {
                            vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL
                        })
                        .load_op(if d.clear.is_some() {
                            vk::AttachmentLoadOp::CLEAR
                        } else {
                            vk::AttachmentLoadOp::LOAD
                        })
                        .store_op(vk::AttachmentStoreOp::STORE)
                        .clear_value(vk::ClearValue {
                            depth_stencil: vk::ClearDepthStencilValue {
                                depth: d.clear.unwrap_or(1.0),
                                stencil: 0,
                            },
                        }),
                )
            }
            None => None,
        };
        let rect = vk::Rect2D {
            offset: vk::Offset2D {
                x: area.x,
                y: area.y,
            },
            extent: vk::Extent2D {
                width: area.width,
                height: area.height,
            },
        };
        let mut info = vk::RenderingInfo::default()
            .render_area(rect)
            .layer_count(1)
            .color_attachments(&colors);
        if let Some(d) = depth.as_ref() {
            info = info.depth_attachment(d);
        }
        // SAFETY: recording outside any rendering; the targets are in the layouts the barriers
        // above put them in.
        unsafe { self.dev.device.cmd_begin_rendering(self.cmd, &info) };
        let mut d = Draw {
            dev: &*self.dev,
            cmd: self.cmd,
            bind_point: vk::PipelineBindPoint::GRAPHICS,
            set_bound: false,
        };
        d.set_viewport(area);
        d.set_scissor(area);
        let mut d = crate::front::Draw {
            b: crate::front::DrawB::Vk(d),
        };
        let result = draw(&mut d);
        // SAFETY: rendering was begun above.
        unsafe { self.dev.device.cmd_end_rendering(self.cmd) };
        result
    }

    /// Record a compute pass: its barriers, then `dispatch` inside.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a resource the device does not hold, or what `dispatch` returns.
    pub fn compute(
        &mut self,
        touches: &[Touch],
        dispatch: impl FnOnce(&mut crate::front::Draw<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.touch(touches)?;
        let mut d = crate::front::Draw {
            b: crate::front::DrawB::Vk(Draw {
                dev: &*self.dev,
                cmd: self.cmd,
                bind_point: vk::PipelineBindPoint::COMPUTE,
                set_bound: false,
            }),
        };
        dispatch(&mut d)
    }

    /// Fill `size` bytes of `b` from `offset` with copies of `value`.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold.
    pub fn fill_buffer(
        &mut self,
        b: Buffer,
        offset: u64,
        size: u64,
        value: u32,
    ) -> Result<(), Error> {
        self.touch(&[Touch::Buffer(b, Use::TransferDst)])?;
        let buffer = self.dev.buffer(b)?.buffer;
        // SAFETY: recording outside any rendering; the range is the caller's.
        unsafe {
            self.dev
                .device
                .cmd_fill_buffer(self.cmd, buffer, offset, size, value);
        }
        Ok(())
    }

    /// Copy `size` bytes of `src` from `src_offset` into `dst` at `dst_offset`.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold.
    pub fn copy_buffer(
        &mut self,
        src: Buffer,
        src_offset: u64,
        dst: Buffer,
        dst_offset: u64,
        size: u64,
    ) -> Result<(), Error> {
        self.touch(&[
            Touch::Buffer(src, Use::TransferSrc),
            Touch::Buffer(dst, Use::TransferDst),
        ])?;
        let (s, d) = (self.dev.buffer(src)?.buffer, self.dev.buffer(dst)?.buffer);
        let region = vk::BufferCopy {
            src_offset,
            dst_offset,
            size,
        };
        // SAFETY: recording outside any rendering; both buffers are live.
        unsafe { self.dev.device.cmd_copy_buffer(self.cmd, s, d, &[region]) };
        Ok(())
    }

    /// Copy one level of `t` into `dst` at `offset`, tightly packed.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a resource the device does not hold.
    pub fn copy_texture_to_buffer(
        &mut self,
        t: Texture,
        level: u32,
        dst: Buffer,
        offset: u64,
    ) -> Result<(), Error> {
        self.touch(&[
            Touch::Texture(t, Use::TransferSrc),
            Touch::Buffer(dst, Use::TransferDst),
        ])?;
        let e = self.dev.texture(t)?;
        let (w, h) = ((e.width >> level).max(1), (e.height >> level).max(1));
        let aspect = if e.format.is_depth() {
            vk::ImageAspectFlags::DEPTH
        } else {
            vk::ImageAspectFlags::COLOR
        };
        let image = e.image;
        let buffer = self.dev.buffer(dst)?.buffer;
        let region = vk::BufferImageCopy::default()
            .buffer_offset(offset)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: aspect,
                mip_level: level,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_extent(vk::Extent3D {
                width: w,
                height: h,
                depth: 1,
            });
        // SAFETY: recording outside any rendering; the image is in the copy layout.
        unsafe {
            self.dev.device.cmd_copy_image_to_buffer(
                self.cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                buffer,
                &[region],
            );
        }
        Ok(())
    }

    /// Copy `src`'s first level into `dst`'s, every texel: two textures of one size and format.
    ///
    /// # Errors
    /// [`Error::Invalid`] for textures of different sizes or formats; a handle that is not live.
    pub fn copy_texture(&mut self, src: Texture, dst: Texture) -> Result<(), Error> {
        let (a, b) = (self.dev.texture(src)?, self.dev.texture(dst)?);
        if (a.width, a.height, a.format) != (b.width, b.height, b.format) {
            return Err(Error::Invalid(
                "a copy between textures of different sizes or formats".into(),
            ));
        }
        let aspect = if a.format.is_depth() {
            vk::ImageAspectFlags::DEPTH
        } else {
            vk::ImageAspectFlags::COLOR
        };
        let layers = vk::ImageSubresourceLayers {
            aspect_mask: aspect,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        };
        let region = vk::ImageCopy::default()
            .src_subresource(layers)
            .dst_subresource(layers)
            .extent(vk::Extent3D {
                width: a.width,
                height: a.height,
                depth: 1,
            });
        let (from, to) = (a.image, b.image);
        self.touch(&[
            Touch::Texture(src, Use::TransferSrc),
            Touch::Texture(dst, Use::TransferDst),
        ])?;
        // SAFETY: recording outside any rendering; the two images are in the copy layouts the
        // touch above moved them to, and the region is both images' whole first level.
        unsafe {
            self.dev.device.cmd_copy_image(
                self.cmd,
                from,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                to,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            );
        }
        Ok(())
    }

    /// End and submit a recording of the device's own.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a recording into a lent command buffer; any failure of the driver.
    pub fn submit(mut self) -> Result<Ticket, Error> {
        if !self.own {
            return Err(Error::Invalid(
                "a lent command buffer is submitted by its owner".into(),
            ));
        }
        self.close()?;
        Ok(Ticket(self.at.value))
    }

    /// Finish a recording into a lent command buffer: what it holds goes when the owner's frame
    /// is done. A recording of the device's own is submitted.
    ///
    /// # Errors
    /// Any failure of the driver.
    pub fn finish(mut self) -> Result<(), Error> {
        self.close()
    }

    fn close(&mut self) -> Result<(), Error> {
        if self.done {
            return Ok(());
        }
        self.done = true;
        if let Some(t) = self.timer.take() {
            self.dev.garbage.push((self.at, Garbage::Timer(t)));
        }
        self.dev.stats.recordings += 1;
        if !self.own {
            return Ok(());
        }
        // SAFETY: recording is finished.
        vkr("vkEndCommandBuffer", unsafe {
            self.dev.device.end_command_buffer(self.cmd)
        })?;
        let cmds = [vk::CommandBufferSubmitInfo::default().command_buffer(self.cmd)];
        let signals = [vk::SemaphoreSubmitInfo::default()
            .semaphore(self.dev.timeline)
            .value(self.at.value)
            .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)];
        let submit = vk::SubmitInfo2::default()
            .command_buffer_infos(&cmds)
            .signal_semaphore_infos(&signals);
        // SAFETY: every handle is live; the queue is driven from this one thread.
        let r = vkr("vkQueueSubmit2", unsafe {
            self.dev
                .device
                .queue_submit2(self.dev.queue, &[submit], vk::Fence::null())
        });
        self.dev.garbage.push((self.at, Garbage::Command(self.cmd)));
        r
    }
}

impl Drop for Recorder<'_> {
    fn drop(&mut self) {
        // A recording dropped unfinished is finished, so the copies at its head are not lost.
        if let Err(e) = self.close() {
            tracing::error!(target: "dereth::gfx", "a recording could not be submitted: {e}");
        }
    }
}

/// Inside a pass: the pipeline, push constants, viewport and the draws or dispatches.
#[derive(Debug)]
pub struct Draw<'r> {
    dev: &'r Device,
    cmd: vk::CommandBuffer,
    bind_point: vk::PipelineBindPoint,
    set_bound: bool,
}

impl Draw<'_> {
    /// Use pipeline `p` for what follows. The bindless table is bound with the first.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a pipeline the device does not hold, or one of the other kind.
    pub fn set_pipeline(&mut self, p: Pipeline) -> Result<(), Error> {
        let e = self
            .dev
            .pipelines
            .entries
            .get(p.0 as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| Error::Invalid(format!("no pipeline {}", p.0)))?;
        if e.bind_point != self.bind_point {
            return Err(Error::Invalid("a pipeline of the other kind".into()));
        }
        // SAFETY: recording inside the pass; the pipeline is live.
        unsafe {
            self.dev
                .device
                .cmd_bind_pipeline(self.cmd, self.bind_point, e.pipeline);
            if !self.set_bound {
                self.dev.device.cmd_bind_descriptor_sets(
                    self.cmd,
                    self.bind_point,
                    self.dev.pipelines.layout,
                    0,
                    &[self.dev.bindless.set],
                    &[],
                );
                self.set_bound = true;
            }
        }
        Ok(())
    }

    /// Push `bytes` at `offset` into the push constants every stage reads.
    ///
    /// # Errors
    /// [`Error::Invalid`] past [`PUSH_BYTES`].
    pub fn push(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Error> {
        if offset as usize + bytes.len() > PUSH_BYTES as usize {
            return Err(Error::Invalid("push constants past their end".into()));
        }
        // SAFETY: recording; the range is inside the layout's.
        unsafe {
            self.dev.device.cmd_push_constants(
                self.cmd,
                self.dev.pipelines.layout,
                vk::ShaderStageFlags::ALL,
                offset,
                bytes,
            );
        }
        Ok(())
    }

    /// Draw into `r` of the targets. The viewport has a negative height, so clip space is the
    /// ordinary renderer's: y up, z from 0 to 1.
    pub fn set_viewport(&mut self, r: Rect) {
        #[allow(clippy::cast_precision_loss)] // a target's extent
        let v = vk::Viewport {
            x: r.x as f32,
            y: r.y as f32 + r.height as f32,
            width: r.width as f32,
            height: -(r.height as f32),
            min_depth: 0.0,
            max_depth: 1.0,
        };
        // SAFETY: recording inside a raster pass.
        unsafe { self.dev.device.cmd_set_viewport(self.cmd, 0, &[v]) };
    }

    /// Draw only inside `r`.
    pub fn set_scissor(&mut self, r: Rect) {
        let s = vk::Rect2D {
            offset: vk::Offset2D { x: r.x, y: r.y },
            extent: vk::Extent2D {
                width: r.width,
                height: r.height,
            },
        };
        // SAFETY: recording inside a raster pass.
        unsafe { self.dev.device.cmd_set_scissor(self.cmd, 0, &[s]) };
    }

    fn raw_buffer(&self, b: Buffer) -> Result<vk::Buffer, Error> {
        Ok(self.dev.buffer(b)?.buffer)
    }

    /// Take indices from `b` at `offset`.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold.
    pub fn bind_index_buffer(
        &mut self,
        b: Buffer,
        offset: u64,
        ty: IndexType,
    ) -> Result<(), Error> {
        let buffer = self.raw_buffer(b)?;
        let ty = match ty {
            IndexType::U16 => vk::IndexType::UINT16,
            IndexType::U32 => vk::IndexType::UINT32,
        };
        // SAFETY: recording inside a raster pass; the buffer is live.
        unsafe {
            self.dev
                .device
                .cmd_bind_index_buffer(self.cmd, buffer, offset, ty);
        }
        Ok(())
    }

    /// Draw `vertices` vertices, `instances` times.
    pub fn draw(&mut self, vertices: u32, instances: u32, first_vertex: u32, first_instance: u32) {
        // SAFETY: recording inside a raster pass with a pipeline bound.
        unsafe {
            self.dev
                .device
                .cmd_draw(self.cmd, vertices, instances, first_vertex, first_instance);
        }
    }

    /// Draw `indices` indices, `instances` times.
    pub fn draw_indexed(
        &mut self,
        indices: u32,
        instances: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        // SAFETY: recording inside a raster pass with a pipeline and indices bound.
        unsafe {
            self.dev.device.cmd_draw_indexed(
                self.cmd,
                indices,
                instances,
                first_index,
                vertex_offset,
                first_instance,
            );
        }
    }

    /// `count` draws from the commands in `b` at `offset`, `stride` bytes apart.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold.
    pub fn draw_indirect(
        &mut self,
        b: Buffer,
        offset: u64,
        count: u32,
        stride: u32,
    ) -> Result<(), Error> {
        let buffer = self.raw_buffer(b)?;
        // SAFETY: recording inside a raster pass; the commands are the caller's.
        unsafe {
            self.dev
                .device
                .cmd_draw_indirect(self.cmd, buffer, offset, count, stride);
        }
        Ok(())
    }

    /// `count` indexed draws from the commands in `b` at `offset`, `stride` bytes apart.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold.
    pub fn draw_indexed_indirect(
        &mut self,
        b: Buffer,
        offset: u64,
        count: u32,
        stride: u32,
    ) -> Result<(), Error> {
        let buffer = self.raw_buffer(b)?;
        // SAFETY: as in `draw_indirect`.
        unsafe {
            self.dev
                .device
                .cmd_draw_indexed_indirect(self.cmd, buffer, offset, count, stride);
        }
        Ok(())
    }

    /// The indexed draws in `b` at `offset`, as many as the count in `count_buffer` at
    /// `count_offset` says and at most `max`, `stride` bytes apart.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_indexed_indirect_count(
        &mut self,
        b: Buffer,
        offset: u64,
        count_buffer: Buffer,
        count_offset: u64,
        max: u32,
        stride: u32,
    ) -> Result<(), Error> {
        let buffer = self.raw_buffer(b)?;
        let count = self.raw_buffer(count_buffer)?;
        if !self.dev.caps.indirect_count {
            // SAFETY: as in `draw_indirect`; the commands past the count draw nothing.
            unsafe {
                self.dev
                    .device
                    .cmd_draw_indexed_indirect(self.cmd, buffer, offset, max, stride);
            }
            return Ok(());
        }
        // SAFETY: as in `draw_indirect`.
        unsafe {
            self.dev.device.cmd_draw_indexed_indirect_count(
                self.cmd,
                buffer,
                offset,
                count,
                count_offset,
                max,
                stride,
            );
        }
        Ok(())
    }

    /// The draws in `b` at `offset`, as many as the count in `count_buffer` at `count_offset`
    /// says and at most `max`, `stride` bytes apart.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold.
    pub fn draw_indirect_count(
        &mut self,
        b: Buffer,
        offset: u64,
        count_buffer: Buffer,
        count_offset: u64,
        max: u32,
        stride: u32,
    ) -> Result<(), Error> {
        let buffer = self.raw_buffer(b)?;
        let count = self.raw_buffer(count_buffer)?;
        if !self.dev.caps.indirect_count {
            // SAFETY: as in `draw_indirect`; the commands past the count draw nothing.
            unsafe {
                self.dev
                    .device
                    .cmd_draw_indirect(self.cmd, buffer, offset, max, stride);
            }
            return Ok(());
        }
        // SAFETY: as in `draw_indirect`.
        unsafe {
            self.dev.device.cmd_draw_indirect_count(
                self.cmd,
                buffer,
                offset,
                count,
                count_offset,
                max,
                stride,
            );
        }
        Ok(())
    }

    /// Run `x * y * z` workgroups of the bound compute pipeline.
    pub fn dispatch(&mut self, x: u32, y: u32, z: u32) {
        // SAFETY: recording a compute pass with a pipeline bound.
        unsafe { self.dev.device.cmd_dispatch(self.cmd, x, y, z) };
    }
}
