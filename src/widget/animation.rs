use anyhow::Result;
use std::collections::VecDeque;
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
    Snake,
    Curtain,
    SineWave,
    Random,
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

        let resolved = if matches!(self.animation, AnimationType::Random) {
            let pick = pick_random_type(&mut rng);
            log::info!("random animation picked: {:?}", pick);
            pick
        } else {
            self.animation.clone()
        };

        match display {
            AnyDisplay::Pixel(ref mut d) => {
                let disp = d.as_mut();
                match resolved {
                    AnimationType::Static => run_static(disp, cancel, &mut rng, self.duration, start),
                    AnimationType::Pong => run_pong(disp, cancel, self.duration, start),
                    AnimationType::MatrixRain => run_matrix_rain(disp, cancel, &mut rng, self.duration, start),
                    AnimationType::Snake => run_snake(disp, cancel, &mut rng, self.duration, start),
                    AnimationType::Curtain => run_curtain(disp, cancel, self.duration, start),
                    AnimationType::SineWave => run_sine_wave(disp, cancel, self.duration, start),
                    AnimationType::Random => unreachable!("random resolved above"),
                }
            }
            AnyDisplay::Segment(ref mut d) => {
                let disp = d.as_mut();
                match resolved {
                    AnimationType::Static => run_segment_static(disp, cancel, &mut rng, self.duration, start),
                    AnimationType::MatrixRain => run_segment_rain(disp, cancel, &mut rng, self.duration, start),
                    AnimationType::Pong => run_segment_pong(disp, cancel, self.duration, start),
                    AnimationType::Snake => run_segment_snake(disp, cancel, self.duration, start),
                    AnimationType::Curtain => run_segment_curtain(disp, cancel, self.duration, start),
                    AnimationType::SineWave => run_segment_sine_wave(disp, cancel, self.duration, start),
                    AnimationType::Random => unreachable!("random resolved above"),
                }
            }
        }
    }
}

fn pick_random_type(rng: &mut SimpleRng) -> AnimationType {
    let options = [
        AnimationType::Static,
        AnimationType::Pong,
        AnimationType::MatrixRain,
        AnimationType::Snake,
        AnimationType::Curtain,
        AnimationType::SineWave,
    ];
    options[rng.next_range(options.len() as u32) as usize].clone()
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
    // A "raindrop" lives on one side (left/right) of one digit and falls
    // through 3 stages: top vertical → bottom vertical → bottom horizontal.
    // Multiple drops can run in parallel; we OR their contributions into
    // the frame so overlapping digits render both sides' lit segments.
    const TOP_LEFT: u16 = 0x20; // f
    const BOT_LEFT: u16 = 0x10; // e
    const TOP_RIGHT: u16 = 0x02; // b
    const BOT_RIGHT: u16 = 0x04; // c
    const BOTTOM: u16 = 0x08; // d

    let frame_interval = Duration::from_millis(150);
    let n = disp.display_length();
    // 2 slots per digit — one per side. -1 = idle, 0..=2 = falling stage.
    let mut drops: Vec<i8> = vec![-1; n * 2];

    loop {
        if check_timeout(cancel, duration, start)? {
            return Ok(());
        }

        // Spawn new drops on idle slots. 1-in-5 per slot per frame gives a
        // steady-but-not-dense rain on 4 digits.
        for d in drops.iter_mut() {
            if *d < 0 && rng.next_range(5) == 0 {
                *d = 0;
            }
        }

        // Render current state.
        let mut segs = vec![0u16; n];
        for (i, d) in drops.iter().enumerate() {
            if *d < 0 {
                continue;
            }
            let digit = i / 2;
            let is_right = (i % 2) == 1;
            let seg = match *d {
                0 => if is_right { TOP_RIGHT } else { TOP_LEFT },
                1 => if is_right { BOT_RIGHT } else { BOT_LEFT },
                2 => BOTTOM,
                _ => 0,
            };
            segs[digit] |= seg;
        }
        disp.write_segments(&segs, false);

        // Advance each active drop; retire after the final stage.
        for d in drops.iter_mut() {
            if *d >= 0 {
                *d += 1;
                if *d > 2 {
                    *d = -1;
                }
            }
        }

        sleep_or_cancel(cancel, frame_interval)?;
    }
}

fn run_segment_pong(
    disp: &mut dyn SegmentDisplay,
    cancel: &CancelToken,
    duration: Duration,
    start: std::time::Instant,
) -> Result<()> {
    // Bounce a "ball" (pair of vertical segments) side-to-side across all
    // digits: digit0-left → digit0-right → digit1-left → digit1-right → ...
    // → last-digit-right, then reverse. Each digit has 2 positions, so on a
    // 4-digit display the ball traverses 8 positions before bouncing.
    const LEFT_VERT: u16 = 0x30; // e + f
    const RIGHT_VERT: u16 = 0x06; // b + c

    let frame_interval = Duration::from_millis(150);
    let n = disp.display_length();
    let max_pos = (n * 2) as i32 - 1;
    let mut pos: i32 = 0;
    let mut dir: i32 = 1;

    loop {
        if check_timeout(cancel, duration, start)? {
            return Ok(());
        }

        let mut segs = vec![0u16; n];
        let digit = (pos as usize) / 2;
        let is_right = (pos % 2) == 1;
        segs[digit] = if is_right { RIGHT_VERT } else { LEFT_VERT };
        disp.write_segments(&segs, false);

        pos += dir;
        if pos >= max_pos || pos <= 0 {
            dir = -dir;
        }

        sleep_or_cancel(cancel, frame_interval)?;
    }
}

fn run_snake(
    disp: &mut dyn PixelDisplay,
    cancel: &CancelToken,
    rng: &mut SimpleRng,
    duration: Duration,
    start: std::time::Instant,
) -> Result<()> {
    let frame_interval = Duration::from_millis(120);
    let target_len: usize = 10;

    let mut body: VecDeque<(i8, i8)> = VecDeque::new();
    body.push_front((16, 4));
    body.push_front((17, 4));
    body.push_front((18, 4));

    let mut food = spawn_snake_food(&body, rng);
    let mut last_dir: (i8, i8) = (1, 0);

    loop {
        if check_timeout(cancel, duration, start)? {
            return Ok(());
        }

        let head = *body.front().unwrap();
        let dir = pick_snake_dir(head, food, last_dir, &body);
        last_dir = dir;

        let new_head = (head.0 + dir.0, head.1 + dir.1);
        body.push_front(new_head);

        let ate = new_head == food;
        if ate {
            food = spawn_snake_food(&body, rng);
        }
        if !ate || body.len() > target_len {
            body.pop_back();
        }

        let mut frame: Frame = [0u8; 32];
        for &(x, y) in body.iter() {
            if x >= 0 && x < DISPLAY_WIDTH as i8 && y >= 0 && y < DISPLAY_HEIGHT as i8 {
                framebuf::set_pixel(&mut frame, x as usize, y as usize, true);
            }
        }
        framebuf::set_pixel(&mut frame, food.0 as usize, food.1 as usize, true);
        disp.write_framebuffer(&frame);

        sleep_or_cancel(cancel, frame_interval)?;
    }
}

fn pick_snake_dir(
    head: (i8, i8),
    food: (i8, i8),
    last: (i8, i8),
    body: &VecDeque<(i8, i8)>,
) -> (i8, i8) {
    let dx = food.0 - head.0;
    let dy = food.1 - head.1;

    let mut candidates: Vec<(i8, i8)> = Vec::with_capacity(4);
    if dx.abs() >= dy.abs() {
        if dx != 0 { candidates.push((dx.signum(), 0)); }
        if dy != 0 { candidates.push((0, dy.signum())); }
    } else {
        if dy != 0 { candidates.push((0, dy.signum())); }
        if dx != 0 { candidates.push((dx.signum(), 0)); }
    }
    for d in &[(1i8, 0i8), (-1, 0), (0, 1), (0, -1)] {
        if !candidates.contains(d) { candidates.push(*d); }
    }
    let reverse = (-last.0, -last.1);
    candidates.retain(|d| *d != reverse);

    let body_len = body.len();
    for d in &candidates {
        let next = (head.0 + d.0, head.1 + d.1);
        if next.0 < 0 || next.0 >= DISPLAY_WIDTH as i8 { continue; }
        if next.1 < 0 || next.1 >= DISPLAY_HEIGHT as i8 { continue; }
        if body.iter().take(body_len.saturating_sub(1)).any(|&p| p == next) { continue; }
        return *d;
    }
    last
}

fn spawn_snake_food(body: &VecDeque<(i8, i8)>, rng: &mut SimpleRng) -> (i8, i8) {
    for _ in 0..40 {
        let x = rng.next_range(DISPLAY_WIDTH as u32) as i8;
        let y = rng.next_range(DISPLAY_HEIGHT as u32) as i8;
        if !body.iter().any(|&p| p == (x, y)) {
            return (x, y);
        }
    }
    (0, 0)
}

fn run_curtain(
    disp: &mut dyn PixelDisplay,
    cancel: &CancelToken,
    duration: Duration,
    start: std::time::Instant,
) -> Result<()> {
    let frame_interval = Duration::from_millis(60);
    let total = DISPLAY_WIDTH * 2;
    let mut phase: usize = 0;

    loop {
        if check_timeout(cancel, duration, start)? {
            return Ok(());
        }

        let mut frame: Frame = [0u8; 32];
        if phase < DISPLAY_WIDTH {
            for c in 0..=phase {
                frame[c] = 0xFF;
            }
        } else {
            let start_col = phase - DISPLAY_WIDTH + 1;
            for c in start_col..DISPLAY_WIDTH {
                frame[c] = 0xFF;
            }
        }
        disp.write_framebuffer(&frame);

        phase = (phase + 1) % total;
        sleep_or_cancel(cancel, frame_interval)?;
    }
}

fn run_sine_wave(
    disp: &mut dyn PixelDisplay,
    cancel: &CancelToken,
    duration: Duration,
    start: std::time::Instant,
) -> Result<()> {
    use core::f32::consts::PI;
    let frame_interval = Duration::from_millis(60);
    let wavelength: f32 = 14.0;
    let amplitude: f32 = 3.0;
    let mid: f32 = 3.5;
    let mut phase: f32 = 0.0;

    loop {
        if check_timeout(cancel, duration, start)? {
            return Ok(());
        }

        let mut frame: Frame = [0u8; 32];
        for x in 0..DISPLAY_WIDTH {
            let y = mid + amplitude * ((x as f32 + phase) * 2.0 * PI / wavelength).sin();
            let yy = y.round() as i32;
            if yy >= 0 && yy < DISPLAY_HEIGHT as i32 {
                framebuf::set_pixel(&mut frame, x, yy as usize, true);
            }
        }
        disp.write_framebuffer(&frame);

        phase += 0.5;
        sleep_or_cancel(cancel, frame_interval)?;
    }
}

fn run_segment_snake(
    disp: &mut dyn SegmentDisplay,
    cancel: &CancelToken,
    duration: Duration,
    start: std::time::Instant,
) -> Result<()> {
    // Snake walks the outer perimeter of each digit, bouncing off the ends.
    // Every digit is entered at 'a' and closes its loop back to 'a' before
    // moving on; segment 'g' (middle) is skipped.
    //   moving right: a, b, c, d, e, f, a  (clockwise, 7 steps)
    //   moving left:  a, f, e, d, c, b, a  (counter-clockwise, 7 steps)
    // The endpoint digit at each bounce drops the closing 'a' (6 steps only).
    const SEG_FWD: [u16; 7] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x01];
    const SEG_REV: [u16; 7] = [0x01, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01];

    let frame_interval = Duration::from_millis(120);
    let n = disp.display_length();
    if n == 0 {
        return Ok(());
    }
    let trail: usize = 3;

    let mut digit: i32 = 0;
    let mut step: usize = 0;
    let mut dir: i32 = 1;
    let mut history: VecDeque<(usize, u16)> = VecDeque::with_capacity(trail);

    loop {
        if check_timeout(cancel, duration, start)? {
            return Ok(());
        }

        let seg = if dir > 0 { SEG_FWD[step] } else { SEG_REV[step] };
        history.push_front((digit as usize, seg));
        while history.len() > trail {
            history.pop_back();
        }

        let mut segs = vec![0u16; n];
        for &(d, s) in history.iter() {
            segs[d] |= s;
        }
        disp.write_segments(&segs, false);

        step += 1;
        let next_digit = digit + dir;
        let at_endpoint = next_digit < 0 || next_digit >= n as i32;
        let max_step = if at_endpoint { 6 } else { 7 };
        if step >= max_step {
            step = 0;
            if at_endpoint {
                dir = -dir;
                digit += dir;
                digit = digit.clamp(0, (n as i32 - 1).max(0));
            } else {
                digit = next_digit;
            }
        }

        sleep_or_cancel(cancel, frame_interval)?;
    }
}

fn run_segment_curtain(
    disp: &mut dyn SegmentDisplay,
    cancel: &CancelToken,
    duration: Duration,
    start: std::time::Instant,
) -> Result<()> {
    // Fill digits left-to-right, 3 stages each:
    //   e,f  →  a,d,e,f,g  →  a,b,c,d,e,f,g
    // Completed digits stay at 0x7F. Once all digits are full, unfill
    // left-to-right, peeling segments away in the same order they were
    // laid down (remove e,f, then a,d,g, then b,c) while digits to the
    // right remain fully lit:
    //   a,b,c,d,e,f,g → a,b,c,d,g → b,c → (off)
    const FILL: [u16; 3] = [0x30, 0x79, 0x7F];
    const UNFILL: [u16; 3] = [0x4F, 0x06, 0x00];
    let frame_interval = Duration::from_millis(150);
    let n = disp.display_length();
    if n == 0 {
        return Ok(());
    }
    let fill_phases = n * 3;
    let total = fill_phases * 2;
    let mut phase: usize = 0;

    loop {
        if check_timeout(cancel, duration, start)? {
            return Ok(());
        }

        let mut segs = vec![0u16; n];
        if phase < fill_phases {
            let current = phase / 3;
            let stage = phase % 3;
            for d in 0..current {
                segs[d] = 0x7F;
            }
            segs[current] = FILL[stage];
        } else {
            let up = phase - fill_phases;
            let current = up / 3;
            let stage = up % 3;
            segs[current] = UNFILL[stage];
            for d in (current + 1)..n {
                segs[d] = 0x7F;
            }
        }
        disp.write_segments(&segs, false);

        phase = (phase + 1) % total;
        sleep_or_cancel(cancel, frame_interval)?;
    }
}

fn run_segment_sine_wave(
    disp: &mut dyn SegmentDisplay,
    cancel: &CancelToken,
    duration: Duration,
    start: std::time::Instant,
) -> Result<()> {
    // Scroll one segment across every digit per step (a, then b, then c, ...),
    // then unroll in the same order. 7 fill + 7 unfill steps per cycle.
    const SEG_ORDER: [u16; 7] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40];
    let frame_interval = Duration::from_millis(150);
    let n = disp.display_length();
    let total = SEG_ORDER.len() * 2;
    let mut phase: usize = 0;

    loop {
        if check_timeout(cancel, duration, start)? {
            return Ok(());
        }

        let mut mask: u16 = 0;
        if phase < SEG_ORDER.len() {
            for i in 0..=phase {
                mask |= SEG_ORDER[i];
            }
        } else {
            let start_i = phase - SEG_ORDER.len() + 1;
            for i in start_i..SEG_ORDER.len() {
                mask |= SEG_ORDER[i];
            }
        }
        let segs = vec![mask; n];
        disp.write_segments(&segs, false);

        phase = (phase + 1) % total;
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
