//! Conversion BGRA → NV12 sur la carte graphique (processeur vidéo D3D11), avec mise à
//! l'échelle sans déformation (bandes noires) : la taille encodée reste fixe même quand la
//! fenêtre capturée change de taille. Couleurs BT.709, plage limitée (16–235), comme attendu par
//! les décodeurs H.264 des navigateurs.

use std::mem::ManuallyDrop;

use windows::core::{Error, Interface, Result};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_RATIONAL, DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};

use crate::remote_play::capture::d3d::Device;

use super::fit_rect;

/// Nombre de textures NV12 en rotation : l'encodeur peut encore lire la précédente.
const RING: usize = 4;

/// RGB plage complète (entrée) ; YCbCr BT.709 plage 16–235 (sortie).
const RGB_FULL: u32 = 0;
const YCBCR_709_STUDIO: u32 = (1 << 2) | (1 << 4);

struct Processor {
    input: (u32, u32),
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
    outputs: Vec<ID3D11VideoProcessorOutputView>,
    staging: ID3D11Texture2D,
    staging_view: ID3D11VideoProcessorInputView,
}

pub struct Converter {
    device: Device,
    video: ID3D11VideoDevice,
    context: ID3D11VideoContext,
    output: (u32, u32),
    fps: u32,
    targets: Vec<ID3D11Texture2D>,
    next: usize,
    processor: Option<Processor>,
}

pub(crate) fn texture(device: &ID3D11Device, size: (u32, u32), format: DXGI_FORMAT, bind: D3D11_BIND_FLAG) -> Result<ID3D11Texture2D> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: size.0,
        Height: size.1,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: bind.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut out = None;
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut out))? };
    out.ok_or_else(Error::empty)
}

impl Converter {
    pub fn new(device: &Device, output: (u32, u32), fps: u32) -> Result<Converter> {
        let video: ID3D11VideoDevice = device.device.cast()?;
        let context: ID3D11VideoContext = device.context.cast()?;
        let mut targets = Vec::with_capacity(RING);
        for _ in 0..RING {
            // certains encodeurs préfèrent des surfaces marquées « encodeur vidéo »
            let t = texture(&device.device, output, DXGI_FORMAT_NV12, D3D11_BIND_RENDER_TARGET | D3D11_BIND_VIDEO_ENCODER)
                .or_else(|_| texture(&device.device, output, DXGI_FORMAT_NV12, D3D11_BIND_RENDER_TARGET))?;
            targets.push(t);
        }
        Ok(Converter { device: device.clone(), video, context, output, fps: fps.max(1), targets, next: 0, processor: None })
    }

    pub fn output_size(&self) -> (u32, u32) {
        self.output
    }

    fn processor_for(&mut self, input: (u32, u32)) -> Result<&Processor> {
        if self.processor.as_ref().is_some_and(|p| p.input == input) {
            return Ok(self.processor.as_ref().unwrap());
        }
        self.processor = None;
        let rate = DXGI_RATIONAL { Numerator: self.fps, Denominator: 1 };
        let desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: rate,
            InputWidth: input.0,
            InputHeight: input.1,
            OutputFrameRate: rate,
            OutputWidth: self.output.0,
            OutputHeight: self.output.1,
            Usage: D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
        };
        unsafe {
            let enumerator = self.video.CreateVideoProcessorEnumerator(&desc)?;
            let processor = self.video.CreateVideoProcessor(&enumerator, 0)?;
            let out_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 } },
            };
            let mut outputs = Vec::with_capacity(self.targets.len());
            for t in &self.targets {
                let mut view = None;
                self.video.CreateVideoProcessorOutputView(t, &enumerator, &out_desc, Some(&mut view))?;
                outputs.push(view.ok_or_else(Error::empty)?);
            }
            // copie privée de l'image WGC : la réserve de capture est libérée tout de suite
            let staging = texture(&self.device.device, input, DXGI_FORMAT_B8G8R8A8_UNORM, D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE)?;
            let in_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: 0 } },
            };
            let mut staging_view = None;
            self.video.CreateVideoProcessorInputView(&staging, &enumerator, &in_desc, Some(&mut staging_view))?;

            let (l, t, r, b) = fit_rect(input, self.output);
            self.context.VideoProcessorSetStreamFrameFormat(&processor, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
            self.context.VideoProcessorSetStreamAutoProcessingMode(&processor, 0, false);
            self.context.VideoProcessorSetStreamSourceRect(&processor, 0, true, Some(&RECT { left: 0, top: 0, right: input.0 as i32, bottom: input.1 as i32 }));
            self.context.VideoProcessorSetStreamDestRect(&processor, 0, true, Some(&RECT { left: l, top: t, right: r, bottom: b }));
            self.context.VideoProcessorSetStreamColorSpace(&processor, 0, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: RGB_FULL });
            self.context.VideoProcessorSetOutputColorSpace(&processor, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: YCBCR_709_STUDIO });
            let black = D3D11_VIDEO_COLOR { Anonymous: D3D11_VIDEO_COLOR_0 { RGBA: D3D11_VIDEO_COLOR_RGBA { R: 0.0, G: 0.0, B: 0.0, A: 1.0 } } };
            self.context.VideoProcessorSetOutputBackgroundColor(&processor, false, &black);

            self.processor = Some(Processor {
                input,
                enumerator,
                processor,
                outputs,
                staging,
                staging_view: staging_view.ok_or_else(Error::empty)?,
            });
        }
        Ok(self.processor.as_ref().unwrap())
    }

    /// Convertit la zone `size` (en haut à gauche) de `source` dans la prochaine texture NV12 de
    /// la rotation et la renvoie. Le travail est seulement soumis au GPU : l'encodeur, sur le
    /// même device, l'attendra.
    pub fn convert(&mut self, source: &ID3D11Texture2D, size: (u32, u32)) -> Result<ID3D11Texture2D> {
        let slot = self.next;
        self.next = (self.next + 1) % self.targets.len();
        let device_context = self.device.context.clone();
        let video_context = self.context.clone();
        let p = self.processor_for(size)?;
        unsafe {
            let area = D3D11_BOX { left: 0, top: 0, front: 0, right: size.0, bottom: size.1, back: 1 };
            device_context.CopySubresourceRegion(&p.staging, 0, 0, 0, 0, source, 0, Some(&area));
            let stream = D3D11_VIDEO_PROCESSOR_STREAM {
                Enable: true.into(),
                pInputSurface: ManuallyDrop::new(Some(p.staging_view.clone())),
                ..Default::default()
            };
            let mut streams = [stream];
            let result = video_context.VideoProcessorBlt(&p.processor, &p.outputs[slot], 0, &streams);
            ManuallyDrop::drop(&mut streams[0].pInputSurface);
            result?;
        }
        Ok(self.targets[slot].clone())
    }
}
