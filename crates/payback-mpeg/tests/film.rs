//! Decodes the Mac intro film when the extracted Mac data is present
//! (`extracted/mac/Payback`, never committed); skipped otherwise.

use std::sync::Arc;

#[test]
fn mac_intro_film() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../extracted/mac/Payback/Videos/payback_high_mpeg1.mpg");
    let Ok(bytes) = std::fs::read(path) else {
        eprintln!("no Mac film, skipped");
        return;
    };
    let data: Arc<[u8]> = bytes.into();
    let t = std::time::Instant::now();
    let mut audio = payback_mpeg::Mpeg::new(data.clone()).expect("MPEG-PS");
    audio.set_video_enabled(false);
    assert_eq!(audio.samplerate(), 48000);
    let samples = audio.decode_all_audio();
    let secs = samples.len() as f64 / 2.0 / 48000.0;
    eprintln!("audio: {} stereo samples, {secs:.2} s, decoded in {:?}", samples.len() / 2, t.elapsed());
    assert!((71.0..73.0).contains(&secs));
    assert!(samples.iter().any(|s| s.abs() > 0.1));

    let t = std::time::Instant::now();
    let mut video = payback_mpeg::Mpeg::new(data).expect("MPEG-PS");
    video.set_audio_enabled(false);
    assert_eq!((video.width(), video.height()), (640, 480));
    assert_eq!(video.framerate(), 25.0);
    let mut frames = 0;
    let mut rgba = vec![0; 640 * 480 * 4];
    while let Some(f) = video.decode_video() {
        if frames == 0 {
            f.to_rgba(&mut rgba);
        }
        frames += 1;
    }
    eprintln!("video: {frames} frames decoded in {:?}", t.elapsed());
    assert!((1790..=1800).contains(&frames));
}
