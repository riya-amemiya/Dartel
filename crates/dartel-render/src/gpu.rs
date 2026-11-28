use std::collections::HashMap;
use std::error::Error;
use std::pin::pin;
use std::sync::{Arc, Weak};
use std::task::{Context, Poll, Waker};

use dartel_term::Picture;
use winit::window::Window;

pub const ATLAS_SIZE: u32 = 2048;

const INITIAL_CAPACITY: u64 = 16 * 1024;
const PICTURE_BUDGET: u64 = 256 << 20;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Quad {
    pub rect: [i16; 4],
    pub source: [u16; 4],
    pub color: [u8; 4],
}

const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Sint16x4, 1 => Uint16x4, 2 => Unorm8x4];

#[derive(Clone, Debug)]
pub struct Layer {
    pub picture: Option<Arc<Picture>>,
    pub count: u32,
}

struct Canvas {
    bindings: wgpu::BindGroup,
    picture: Weak<Picture>,
    bytes: u64,
    frame: u64,
}

pub struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    bindings: wgpu::BindGroup,
    globals: wgpu::Buffer,
    instances: wgpu::Buffer,
    capacity: u64,
    atlas: wgpu::Texture,
    pictures: HashMap<usize, Canvas>,
    frame: u64,
}

fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        std::thread::yield_now();
    }
}

fn instance_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: capacity * size_of::<Quad>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn bytes(quads: &[Quad]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(quads.as_ptr().cast::<u8>(), size_of_val(quads)) }
}

fn texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    texture: &wgpu::Texture,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(
                    &texture.create_view(&wgpu::TextureViewDescriptor::default()),
                ),
            },
        ],
    })
}

fn write(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    origin: [u32; 2],
    size: [u32; 2],
    pixels: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: origin[0],
                y: origin[1],
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size[0] * 4),
            rows_per_image: None,
        },
        wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
    );
}

fn key(picture: &Arc<Picture>) -> usize {
    Arc::as_ptr(picture).addr()
}

impl Gpu {
    pub fn new(window: Arc<Window>) -> Result<Gpu, Box<dyn Error>> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window)?;
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))?;
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))?;
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("the adapter cannot present to this window")?;
        config.format = surface
            .get_capabilities(&adapter)
            .formats
            .into_iter()
            .find(|format| !format.is_srgb())
            .unwrap_or(config.format);
        config.desired_maximum_frame_latency = 1;
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                }),
            ),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Quad>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &ATTRIBUTES,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let atlas = texture(&device, ATLAS_SIZE, ATLAS_SIZE);
        let gpu = Gpu {
            instances: instance_buffer(&device, INITIAL_CAPACITY),
            capacity: INITIAL_CAPACITY,
            bindings: bind(&device, &layout, &globals, &atlas),
            surface,
            device,
            queue,
            config,
            pipeline,
            layout,
            globals,
            atlas,
            pictures: HashMap::new(),
            frame: 0,
        };
        gpu.write_globals();
        Ok(gpu)
    }

    fn write_globals(&self) {
        let mut data = [0; 16];
        data[..4].copy_from_slice(&(self.config.width as f32).to_ne_bytes());
        data[4..8].copy_from_slice(&(self.config.height as f32).to_ne_bytes());
        self.queue.write_buffer(&self.globals, 0, &data);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
        self.write_globals();
    }

    pub fn upload(&self, origin: [u32; 2], size: [u32; 2], pixels: &[u8]) {
        write(&self.queue, &self.atlas, origin, size, pixels);
    }

    fn prepare(&mut self, picture: &Arc<Picture>) {
        if let Some(canvas) = self.pictures.get_mut(&key(picture)) {
            canvas.frame = self.frame;
            return;
        }
        let limit = self.device.limits().max_texture_dimension_2d;
        if picture.width > limit || picture.height > limit {
            return;
        }
        let texture = texture(&self.device, picture.width, picture.height);
        write(
            &self.queue,
            &texture,
            [0; 2],
            [picture.width, picture.height],
            &picture.pixels,
        );
        self.pictures.insert(
            key(picture),
            Canvas {
                bindings: bind(&self.device, &self.layout, &self.globals, &texture),
                picture: Arc::downgrade(picture),
                bytes: picture.pixels.len() as u64,
                frame: self.frame,
            },
        );
    }

    fn prepare_pictures(&mut self, layers: &[Layer]) {
        self.frame += 1;
        self.pictures
            .retain(|_, canvas| canvas.picture.strong_count() > 0);
        for picture in layers.iter().filter_map(|layer| layer.picture.as_ref()) {
            self.prepare(picture);
        }
        if self
            .pictures
            .values()
            .map(|canvas| canvas.bytes)
            .sum::<u64>()
            > PICTURE_BUDGET
        {
            let frame = self.frame;
            self.pictures.retain(|_, canvas| canvas.frame == frame);
        }
    }

    pub fn draw(&mut self, quads: &[Quad], layers: &[Layer], clear: [f64; 3]) -> bool {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return false;
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return true,
        };
        if quads.len() as u64 > self.capacity {
            self.capacity = (quads.len() as u64).next_power_of_two();
            self.instances = instance_buffer(&self.device, self.capacity);
        }
        self.queue.write_buffer(&self.instances, 0, bytes(quads));
        self.prepare_pictures(layers);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default()),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear[0],
                            g: clear[1],
                            b: clear[2],
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            if !quads.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_vertex_buffer(0, self.instances.slice(..size_of_val(quads) as u64));
                let mut first = 0;
                for layer in layers {
                    let bindings = match &layer.picture {
                        Some(picture) => self
                            .pictures
                            .get(&key(picture))
                            .map(|canvas| &canvas.bindings),
                        None => Some(&self.bindings),
                    };
                    if let Some(bindings) = bindings {
                        pass.set_bind_group(0, bindings, &[]);
                        pass.draw(0..4, first..first + layer.count);
                    }
                    first += layer.count;
                }
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        true
    }
}
