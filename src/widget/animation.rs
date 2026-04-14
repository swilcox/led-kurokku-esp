use anyhow::Result;
use std::time::Duration;

use crate::display::{AnyDisplay, PixelDisplay, SegmentDisplay};
use crate::framebuf::{self, Frame};
use super::{Widget, CancelToken, sleep_or_cancel};

const DISPLAY_WIDTH: usize = 32;
const DISPLAY_HEIGHT: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub enum AnimationType {
    Static,
    Pong,
    MatrixRain,
}

pub struct Animation {
    animation: AnimationType,
    duration: Duration,
}

impl Animation {
    pub fn new(animation: AnimationType, duration: Duration) -> Self {
        Self { animation, duration }
    }
}

impl Widget for Animation {
    fn name(&self) -> &str {
        "animation"
    }

    fn run(&mut self, display: &mut AnyDisplay, cancel: &CancelToken) -> Result<()> {
        let start = std::time::Instant::now();
        let mut rng = SimpleRng::new();

        match display {
            AnyDisplay::Pixel(ref mut d) => {
                let disp = d.as_mut();
                match self.animation {
                    AnimationType::Static => run_static(disp, cancel, &mut rng, self.duration, start),
                    AnimationType::Pong => run_pong(disp, cancel, self.duration, start),
                    AnimationType::MatrixRain => run_matrix_rain(disp, cancel, &mut rng, self.duration, start),
                }
            }
            AnyDisplay::Segment(ref mut d) => {
                let disp = d.as_mut();
                match self.animation {
                    AnimationType::Static => run_segment_static(disp, cancel, &mut rng, self.duration, start),
                    AnimationType::MatrixRain => run_segment_rain(disp, cancel, &mut rng, self.duration, start),
                    // Pong has no meaningful 7-seg analog. Return Ok so the
                    // engine reverts to clock rather than error-looping.
                    AnimationType::Pong => Ok(()),
                }
            }
        }
    }
}

struct SimpleRng {
    state: u32,
}

impl SimpleRng {
    fn new() -> Self {
        let mut t: esp_idf_svc::sys::time_t = 0;
        unsafe { esp_idf_svc::sys::time(&mut t); }
        Self { state: t as u32 ^ 0xDEAD_BEEF }
    }

    fn next(&mut self) -> u32 {
        // xorshift32
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        self.state
    }

    fn next_range(&mut self, max: u32) -> u32 {
        self.next() % max
    }
}

fn check_timeout(
    cancel: &CancelToken,
    duration: Duration,
    start: std::time::Instant,
) -> Result<bool> {
    if cancel.is_cancelled() {
        anyhow::bail!("cancelled");
    }
    Ok(duration != Duration::ZERO && start.elapsed() >= duration)
}

fn run_static(
    disp: &mut dyn PixelDisplay,
    cancel: &CancelToken,
    rng: &mut SimpleRng,
    duration: Duration,
    start: std::time::Instant,
) -> Result<()> {
    let frame_interval = Duration::from_millis(80);

    loop {
        if check_timeout(cancel, duration, start)? {
            return Ok(());
        }

        let mut frame: Frame = [0u8; 32];
        for col in frame.iter_mut() {
            *col = rng.next() as u8;
        }
        disp.write_framebuffer(&frame);
        sleep_or_cancel(cancel, frame_interval)?;
    }
}

fn run_pong(
    disp: &mut dyn PixelDisplay,
    cancel: &CancelToken,
    duration: Duration,
    start: std::time::Instant,
) -> Result<()> {
    let frame_interval = Duration::from_millis(60);

    let mut ball_x: i32 = 16;
    let mut ball_y: i32 = 4;
    let mut dx: i32 = 1;
    let mut dy: i32 = 1;

    let mut paddle_l: i32 = 2;
    let mut paddle_r: i32 = 2;
    let paddle_height: i32 = 3;

    loop {
        if check_timeout(cancel, duration, start)? {
            return Ok(());
        }

        ball_x += dx;
        ball_y += dy;

        // Bounce off top/bottom
        if ball_y <= 0 {
            ball_y = 0;
            dy = 1;
        }
        if ball_y >= DISPLAY_HEIGHT as i32 - 1 {
            ball_y = DISPLAY_HEIGHT as i32 - 1;
            dy = -1;
        }

        // AI paddles track ball
        let target_l = ball_y - paddle_height / 2;
        if paddle_l < target_l { paddle_l += 1; }
        if paddle_l > target_l { paddle_l -= 1; }

        let target_r = ball_y - paddle_height / 2;
        if paddle_r < target_r { paddle_r += 1; }
        if paddle_r > target_r { paddle_r -= 1; }

        paddle_l = paddle_l.clamp(0, DISPLAY_HEIGHT as i32 - paddle_height);
        paddle_r = paddle_r.clamp(0, DISPLAY_HEIGHT as i32 - paddle_height);

        // Bounce off paddles
        if ball_x <= 1 {
            if ball_y >= paddle_l && ball_y < paddle_l + paddle_height {
                dx = 1;
            } else {
                // Reset
                ball_x = 16;
                ball_y = 4;
                dx = 1;
            }
        }
        if ball_x >= DISPLAY_WIDTH as i32 - 2 {
            if ball_y >= paddle_r && ball_y < paddle_r + paddle_height {
                dx = -1;
            } else {
                ball_x = 16;
                ball_y = 4;
                dx = -1;
            }
        }

        let mut frame: Frame = [0u8; 32];

        // Draw paddles
        for py in 0..paddle_height {
            framebuf::set_pixel(&mut frame, 0, (paddle_l + py) as usize, true);
            framebuf::set_pixel(&mut frame, DISPLAY_WIDTH - 1, (paddle_r + py) as usize, true);
        }

        // Draw ball
        framebuf::set_pixel(&mut frame, ball_x as usize, ball_y as usize, true);

        // Draw center line (dotted)
        for y in (0..DISPLAY_HEIGHT).step_by(2) {
            framebuf::set_pixel(&mut frame, DISPLAY_WIDTH / 2, y, true);
        }

        disp.write_framebuffer(&frame);
        sleep_or_cancel(cancel, frame_interval)?;
    }
}

fn run_segment_static(
    disp: &mut dyn SegmentDisplay,
    cancel: &CancelToken,
    rng: &mut SimpleRng,
    duration: Duration,
    start: std::time::Instant,
) -> Result<()> {
    let frame_interval = Duration::from_millis(80);
    let n = disp.display_length();

    loop {
        if check_timeout(cancel, duration, start)? {
            return Ok(());
        }
        let mut segs = vec![0u16; n];
        for s in segs.iter_mut() {
            // 7 segments only (bit 7 is DP/colon managed by driver)
            *s = (rng.next() as u16) & 0x7F;
        }
        let colon = rng.next_range(2) == 0;
        disp.write_segments(&segs, colon);
        sleep_or_cancel(cancel, frame_interval)?;
    }
}

fn run_segment_rain(
    disp: &mut dyn SegmentDisplay,
    cancel: &CancelToken,
    rng: &mut SimpleRng,
    duration: Duration,
    start: std::time::Instant,
) -> Result<()> {
    // 7-seg rain stages: top (a) → upper sides (f,b) → middle (g) →
    // lower sides (e,c) → bottom (d) → off. Matches the Go segment Rain.
    const STAGES: [u16; 6] = [0x01, 0x22, 0x40, 0x14, 0x08, 0x00];

    let frame_interval = Duration::from_millis(120);
    let n = disp.display_length();
    let mut drops: Vec<i16> = vec![-1; n];

    loop {
        if check_timeout(cancel, duration, start)? {
            return Ok(());
        }

        // Randomly spawn new drops on idle digits
        for d in drops.iter_mut() {
            if *d < 0 && rng.next_range(6) == 0 {
                *d = 0;
            }
        }

        let mut segs = vec![0u16; n];
        for (i, d) in drops.iter_mut().enumerate() {
            if *d < 0 {
                continue;
            }
            if (*d as usize) < STAGES.len() {
                segs[i] = STAGES[*d as usize];
            }
            *d += 1;
            if *d >= (STAGES.len() + 2) as i16 {
                *d = -1;
            }
        }

        disp.write_segments(&segs, false);
        sleep_or_cancel(cancel, frame_interval)?;
    }
}

fn run_matrix_rain(
    disp: &mut dyn PixelDisplay,
    cancel: &CancelToken,
    rng: &mut SimpleRng,
    duration: Duration,
    start: std::time::Instant,
) -> Result<()> {
    let frame_interval = Duration::from_millis(100);

    // Each column has a "drop" position that falls downward
    let mut drops: [i8; DISPLAY_WIDTH] = [0; DISPLAY_WIDTH];
    let mut speeds: [u8; DISPLAY_WIDTH] = [0; DISPLAY_WIDTH];

    for i in 0..DISPLAY_WIDTH {
        drops[i] = -(rng.next_range(8) as i8);
        speeds[i] = 1 + rng.next_range(2) as u8;
    }

    let mut tick: u8 = 0;

    loop {
        if check_timeout(cancel, duration, start)? {
            return Ok(());
        }

        tick = tick.wrapping_add(1);

        let mut frame: Frame = [0u8; 32];

        for col in 0..DISPLAY_WIDTH {
            if tick % speeds[col] != 0 {
                // This column doesn't advance this tick
            } else {
                drops[col] += 1;
            }

            let head = drops[col];
            let trail_len = 4i8;

            for y in 0..DISPLAY_HEIGHT as i8 {
                if y <= head && y > head - trail_len {
                    framebuf::set_pixel(&mut frame, col, y as usize, true);
                }
            }

            // Reset when fully off screen
            if head - trail_len >= DISPLAY_HEIGHT as i8 {
                drops[col] = -(rng.next_range(12) as i8);
                speeds[col] = 1 + rng.next_range(2) as u8;
            }
        }

        disp.write_framebuffer(&frame);
        sleep_or_cancel(cancel, frame_interval)?;
    }
}
