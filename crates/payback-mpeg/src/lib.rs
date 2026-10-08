//! Minimal safe wrapper around the vendored pl_mpeg decoder
//! (`vendor/pl_mpeg.h`, MIT): MPEG-1 system streams with MPEG-1 video and
//! MP2 audio, as in the Mac release's intro film.
//!
//! The stream is decoded from memory. Video frames convert to RGBA; audio
//! comes out as interleaved stereo `f32` (mono streams are duplicated).

use std::ffi::{c_double, c_int, c_uint, c_void};
use std::sync::Arc;

/// Samples per channel in one decoded MP2 frame (`PLM_AUDIO_SAMPLES_PER_FRAME`).
pub const SAMPLES_PER_FRAME: usize = 1152;

mod ffi {
    use super::*;

    #[repr(C)]
    pub struct Plane {
        pub width: c_uint,
        pub height: c_uint,
        pub data: *mut u8,
    }

    #[repr(C)]
    pub struct Frame {
        pub time: c_double,
        pub width: c_uint,
        pub height: c_uint,
        pub y: Plane,
        pub cr: Plane,
        pub cb: Plane,
    }

    #[repr(C)]
    pub struct Samples {
        pub time: c_double,
        pub count: c_uint,
        pub interleaved: [f32; SAMPLES_PER_FRAME * 2],
    }

    unsafe extern "C" {
        pub fn plm_create_with_memory(bytes: *mut u8, length: usize, free_when_done: c_int) -> *mut c_void;
        pub fn plm_destroy(plm: *mut c_void);
        pub fn plm_has_headers(plm: *mut c_void) -> c_int;
        pub fn plm_get_width(plm: *mut c_void) -> c_int;
        pub fn plm_get_height(plm: *mut c_void) -> c_int;
        pub fn plm_get_framerate(plm: *mut c_void) -> c_double;
        pub fn plm_get_samplerate(plm: *mut c_void) -> c_int;
        pub fn plm_get_num_video_streams(plm: *mut c_void) -> c_int;
        pub fn plm_get_num_audio_streams(plm: *mut c_void) -> c_int;
        pub fn plm_set_video_enabled(plm: *mut c_void, enabled: c_int);
        pub fn plm_set_audio_enabled(plm: *mut c_void, enabled: c_int);
        pub fn plm_has_ended(plm: *mut c_void) -> c_int;
        pub fn plm_decode_video(plm: *mut c_void) -> *mut Frame;
        pub fn plm_decode_audio(plm: *mut c_void) -> *mut Samples;
        pub fn plm_frame_to_rgba(frame: *mut Frame, dest: *mut u8, stride: c_int);
    }
}

/// An MPEG-1 program stream being decoded.
pub struct Mpeg {
    plm: *mut c_void,
    /// The stream bytes; pl_mpeg reads them in place (fixed-memory buffers
    /// are never written to) and must not outlive them.
    _data: Arc<[u8]>,
    width: u32,
    height: u32,
    framerate: f64,
    samplerate: u32,
    has_video: bool,
    has_audio: bool,
}

// SAFETY: the decoder state is owned by this value and only touched through
// `&mut self`; the shared bytes are read-only.
unsafe impl Send for Mpeg {}
// SAFETY: no `&self` method calls into pl_mpeg (stream properties are cached).
unsafe impl Sync for Mpeg {}

impl Mpeg {
    /// Opens a stream held in memory. Several decoders may share the bytes,
    /// e.g. one for video and one for audio. `None` if it is not MPEG-PS.
    pub fn new(data: Arc<[u8]>) -> Option<Mpeg> {
        // SAFETY: the buffer outlives the decoder (kept in `_data`); with
        // free_when_done = 0 pl_mpeg neither frees nor writes it.
        let plm = unsafe { ffi::plm_create_with_memory(data.as_ptr() as *mut u8, data.len(), 0) };
        if plm.is_null() {
            return None;
        }
        // SAFETY: `plm` is a live decoder.
        unsafe {
            let mut m = Mpeg { plm, _data: data, width: 0, height: 0, framerate: 0.0, samplerate: 0, has_video: false, has_audio: false };
            if ffi::plm_has_headers(plm) == 0 {
                return None;
            }
            m.has_video = ffi::plm_get_num_video_streams(plm) > 0;
            m.has_audio = ffi::plm_get_num_audio_streams(plm) > 0;
            m.width = ffi::plm_get_width(plm).max(0) as u32;
            m.height = ffi::plm_get_height(plm).max(0) as u32;
            m.framerate = ffi::plm_get_framerate(plm);
            m.samplerate = ffi::plm_get_samplerate(plm).max(0) as u32;
            Some(m)
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Video frames per second (0 without video).
    pub fn framerate(&self) -> f64 {
        self.framerate
    }

    /// Audio sample rate in Hz (0 without audio).
    pub fn samplerate(&self) -> u32 {
        self.samplerate
    }

    pub fn has_video(&self) -> bool {
        self.has_video
    }

    pub fn has_audio(&self) -> bool {
        self.has_audio
    }

    /// Turns video decoding off, e.g. for a decoder that only extracts audio.
    pub fn set_video_enabled(&mut self, enabled: bool) {
        // SAFETY: live decoder.
        unsafe { ffi::plm_set_video_enabled(self.plm, enabled as c_int) }
    }

    /// Turns audio decoding off.
    pub fn set_audio_enabled(&mut self, enabled: bool) {
        // SAFETY: live decoder.
        unsafe { ffi::plm_set_audio_enabled(self.plm, enabled as c_int) }
    }

    pub fn has_ended(&mut self) -> bool {
        // SAFETY: live decoder.
        unsafe { ffi::plm_has_ended(self.plm) != 0 }
    }

    /// Decodes the next video frame, or `None` at the end of the stream.
    pub fn decode_video(&mut self) -> Option<Frame<'_>> {
        // SAFETY: live decoder; the frame stays valid until the next decode
        // call, which the borrow on `self` prevents.
        let frame = unsafe { ffi::plm_decode_video(self.plm) };
        (!frame.is_null()).then_some(Frame { frame, _mpeg: std::marker::PhantomData })
    }

    /// Decodes the next MP2 frame ([`SAMPLES_PER_FRAME`] stereo samples),
    /// or `None` at the end of the stream.
    pub fn decode_audio(&mut self) -> Option<Samples<'_>> {
        // SAFETY: as for `decode_video`.
        let samples = unsafe { ffi::plm_decode_audio(self.plm) };
        (!samples.is_null()).then(|| Samples { samples: unsafe { &*samples } })
    }

    /// Decodes all remaining audio as interleaved stereo `f32`.
    pub fn decode_all_audio(&mut self) -> Vec<f32> {
        let mut out = Vec::new();
        while let Some(s) = self.decode_audio() {
            out.extend_from_slice(s.interleaved());
        }
        out
    }
}

impl Drop for Mpeg {
    fn drop(&mut self) {
        // SAFETY: `plm` was created by plm_create_with_memory and is freed once.
        unsafe { ffi::plm_destroy(self.plm) }
    }
}

/// A decoded video frame, valid until the next decode call.
pub struct Frame<'a> {
    frame: *mut ffi::Frame,
    _mpeg: std::marker::PhantomData<&'a mut Mpeg>,
}

impl Frame<'_> {
    /// Presentation time in seconds.
    pub fn time(&self) -> f64 {
        // SAFETY: the frame is live for 'a.
        unsafe { (*self.frame).time }
    }

    pub fn width(&self) -> u32 {
        // SAFETY: as above.
        unsafe { (*self.frame).width }
    }

    pub fn height(&self) -> u32 {
        // SAFETY: as above.
        unsafe { (*self.frame).height }
    }

    /// Converts to RGBA (alpha 255) into `dest`, `width * height * 4` bytes.
    pub fn to_rgba(&self, dest: &mut [u8]) {
        let (w, h) = (self.width() as usize, self.height() as usize);
        assert!(dest.len() >= w * h * 4, "RGBA buffer too small");
        // SAFETY: `dest` holds a full frame; pl_mpeg writes w*h pixels
        // with the given stride and leaves alpha untouched.
        unsafe { ffi::plm_frame_to_rgba(self.frame, dest.as_mut_ptr(), (w * 4) as c_int) }
        for px in dest[..w * h * 4].chunks_exact_mut(4) {
            px[3] = 255;
        }
    }

    /// The frame as a new RGBA buffer.
    pub fn rgba(&self) -> Vec<u8> {
        let mut v = vec![0; self.width() as usize * self.height() as usize * 4];
        self.to_rgba(&mut v);
        v
    }
}

/// One decoded MP2 frame, valid until the next decode call.
pub struct Samples<'a> {
    samples: &'a ffi::Samples,
}

impl Samples<'_> {
    /// Presentation time in seconds.
    pub fn time(&self) -> f64 {
        self.samples.time
    }

    /// Interleaved stereo samples in -1..1.
    pub fn interleaved(&self) -> &[f32] {
        let n = (self.samples.count as usize).min(SAMPLES_PER_FRAME) * 2;
        &self.samples.interleaved[..n]
    }
}
