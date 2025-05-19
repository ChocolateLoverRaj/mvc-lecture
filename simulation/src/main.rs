use iced::{
    Element,
    Length::Fill,
    Point, Rectangle, Renderer, Size, Subscription, Theme, mouse,
    time::{Duration, Instant},
    widget::{button, canvas, column, image, row, slider, text},
    window,
};

pub fn main() -> iced::Result {
    iced::application("Following Distance Simulation", App::update, App::view)
        .subscription(App::subscription)
        .run()
}

#[derive(Debug)]
enum RunningState {
    Running(Instant),
    Paused,
}

struct App {
    car_a_initial_velocity: f32,
    car_a_braking_acceleration: f32,
    car_a_reaction_time: Duration,
    car_b_initial_velocity: f32,
    car_b_braking_acceleration: f32,
    initial_distance_between: f32,
    car_a_image: image::Handle,
    /// width / height
    car_a_image_ratio: f32,
    car_a_width_meters: f32,
    car_b_image: image::Handle,
    /// width / height
    car_b_image_ratio: f32,
    car_b_width_meters: f32,
    /// How many pixels is 1m
    /// The cars will be the same size on screen regardless of scale
    car_scale: f32,
    collision_image: image::Handle,
    collision_image_size: Size,
    collision_image_scale: f32,

    // Actual simulation state
    simulation_time: Duration,
    running_state: RunningState,
}

#[derive(Debug, Clone, Copy)]
struct MotionState {
    position: f32,
    velocity: f32,
    acceleration: f32,
}

fn solve_quadratic(a: f32, b: f32, c: f32) -> Option<(f32, f32)> {
    if a == 0.0 {
        // Not a quadratic equation
        if b == 0.0 {
            return None;
        }
        // Linear case: bt + c = 0 → t = -c / b
        let t = -c / b;
        return Some((t, t));
    }

    let discriminant = b * b - 4.0 * a * c;

    if discriminant < 0.0 {
        None
    } else {
        let sqrt_disc = discriminant.sqrt();
        let t1 = (-b + sqrt_disc) / (2.0 * a);
        let t2 = (-b - sqrt_disc) / (2.0 * a);
        Some((t1, t2))
    }
}

#[derive(Debug)]
enum CrashType {
    CrashBeforeReaction,
    CrashAfterReactionBeforeCarBStops,
    CrashAfterReactionAfterCarBStops,
}

#[derive(Debug)]
struct CrashInfo {
    time: Duration,
    crash_type: CrashType,
}

impl App {
    fn get_car_a_stop_time_if_no_crash(&self) -> Duration {
        Duration::from_secs_f32(
            self.car_a_reaction_time.as_secs_f32()
                + self.car_a_initial_velocity / self.car_a_braking_acceleration,
        )
    }

    fn get_car_b_stop_time_if_no_crash(&self) -> Duration {
        Duration::from_secs_f32(self.car_b_initial_velocity / self.car_b_braking_acceleration)
    }

    fn get_crash_info(&self) -> Option<CrashInfo> {
        let car_b_stop_time_if_no_crash = self.get_car_b_stop_time_if_no_crash();

        let crash_time_before_reaction = {
            let a = 0.5 * -self.car_b_braking_acceleration;
            let b = self.car_b_initial_velocity - self.car_a_initial_velocity;
            let c = self.initial_distance_between;

            solve_quadratic(a, b, c)
                .and_then(|(t1, t2)| match (t1 >= 0.0, t2 >= 0.0) {
                    (true, true) => Some(t1.min(t2)),
                    (true, false) => Some(t1),
                    (false, true) => Some(t2),
                    (false, false) => None,
                })
                .map(Duration::from_secs_f32)
                .filter(|time| *time < self.car_a_reaction_time)
        };
        let crash_time_after_reaction_before_car_b_stops = match crash_time_before_reaction {
            None => {
                let car_b_acceleration = if car_b_stop_time_if_no_crash < self.car_a_reaction_time {
                    0.0
                } else {
                    -self.car_b_braking_acceleration
                };
                let car_b_initial_velocity = if car_b_stop_time_if_no_crash
                    < self.car_a_reaction_time
                {
                    0.0
                } else {
                    self.car_b_initial_velocity
                        - self.car_b_braking_acceleration * self.car_a_reaction_time.as_secs_f32()
                };
                let car_b_initial_position = self.initial_distance_between
                    + self.car_b_initial_velocity * self.car_a_reaction_time.as_secs_f32()
                    + 0.5
                        * -self.car_b_braking_acceleration
                        * self.car_a_reaction_time.as_secs_f32().powi(2);

                let car_a_acceleration = -self.car_a_braking_acceleration;
                let car_a_initial_velocity = self.car_a_initial_velocity;
                let car_a_initial_position =
                    self.car_a_initial_velocity * self.car_a_reaction_time.as_secs_f32();

                let a = 0.5 * (car_b_acceleration - car_a_acceleration);
                let b = car_b_initial_velocity - car_a_initial_velocity;
                let c = car_b_initial_position - car_a_initial_position;

                solve_quadratic(a, b, c)
                    .and_then(|(t1, t2)| match (t1 >= 0.0, t2 >= 0.0) {
                        (true, true) => Some(t1.min(t2)),
                        (true, false) => Some(t1),
                        (false, true) => Some(t2),
                        (false, false) => None,
                    })
                    .map(Duration::from_secs_f32)
                    .map(|time| self.car_a_reaction_time + time)
                    .filter(|time| *time < car_b_stop_time_if_no_crash)
            }
            Some(_) => None,
        };
        let crash_time_after_reaction_after_car_b_stops =
            match crash_time_before_reaction.or(crash_time_after_reaction_before_car_b_stops) {
                None => {
                    let car_b_acceleration = 0.0;
                    let car_b_initial_velocity = 0.0;
                    let car_b_initial_position = self.initial_distance_between
                        + self.car_b_initial_velocity * car_b_stop_time_if_no_crash.as_secs_f32()
                        + 0.5
                            * -self.car_b_braking_acceleration
                            * car_b_stop_time_if_no_crash.as_secs_f32().powi(2);

                    let car_a_acceleration = -self.car_a_braking_acceleration;
                    let car_a_initial_velocity = self.car_a_initial_velocity;
                    let car_a_initial_position =
                        self.car_a_initial_velocity * self.car_a_reaction_time.as_secs_f32();

                    let a = 0.5 * (car_b_acceleration - car_a_acceleration);
                    let b = car_b_initial_velocity - car_a_initial_velocity;
                    let c = car_b_initial_position - car_a_initial_position;

                    solve_quadratic(a, b, c)
                        .and_then(|(t1, t2)| match (t1 >= 0.0, t2 >= 0.0) {
                            (true, true) => Some(t1.min(t2)),
                            (true, false) => Some(t1),
                            (false, true) => Some(t2),
                            (false, false) => None,
                        })
                        .map(Duration::from_secs_f32)
                        .map(|time| self.car_a_reaction_time + time)
                }
                Some(_) => None,
            };

        if let Some(time) = crash_time_before_reaction {
            Some(CrashInfo {
                time,
                crash_type: CrashType::CrashBeforeReaction,
            })
        } else if let Some(time) = crash_time_after_reaction_before_car_b_stops {
            Some(CrashInfo {
                time,
                crash_type: CrashType::CrashAfterReactionBeforeCarBStops,
            })
        } else if let Some(time) = crash_time_after_reaction_after_car_b_stops {
            Some(CrashInfo {
                time,
                crash_type: CrashType::CrashAfterReactionAfterCarBStops,
            })
        } else {
            None
        }
    }

    fn get_simulation_end_time(&self) -> Duration {
        match self.get_crash_info() {
            Some(crash_info) => crash_info.time,
            None => self
                .get_car_a_stop_time_if_no_crash()
                .max(self.get_car_b_stop_time_if_no_crash()),
        }
    }

    fn get_motion_states(&self, time: Duration) -> (MotionState, MotionState) {
        let crash_info = self.get_crash_info();

        let car_a_stop_time_if_no_crash = self.get_car_a_stop_time_if_no_crash();
        let car_b_stop_time_if_no_crash = self.get_car_b_stop_time_if_no_crash();

        let get_car_a_if_no_collide = |time: Duration| MotionState {
            acceleration: if time < car_a_stop_time_if_no_crash {
                -self.car_a_braking_acceleration
            } else {
                0.0
            },
            velocity: if time < car_a_stop_time_if_no_crash {
                self.car_a_initial_velocity
                    - self.car_a_braking_acceleration
                        * time.saturating_sub(self.car_a_reaction_time).as_secs_f32()
            } else {
                0.0
            },
            position: {
                let time = time.min(car_a_stop_time_if_no_crash);
                self.car_a_initial_velocity * time.as_secs_f32()
                    + 0.5
                        * -self.car_a_braking_acceleration
                        * time
                            .saturating_sub(self.car_a_reaction_time)
                            .as_secs_f32()
                            .powi(2)
            },
        };
        let get_car_b_if_no_collide = |time: Duration| MotionState {
            acceleration: if time < car_b_stop_time_if_no_crash {
                -self.car_b_braking_acceleration
            } else {
                0.0
            },
            velocity: if time < car_b_stop_time_if_no_crash {
                self.car_b_initial_velocity - self.car_b_braking_acceleration * time.as_secs_f32()
            } else {
                0.0
            },
            position: {
                let time = time.min(car_b_stop_time_if_no_crash);
                self.initial_distance_between
                    + self.car_b_initial_velocity * time.as_secs_f32()
                    + 0.5 * -self.car_b_braking_acceleration * time.as_secs_f32().powi(2)
            },
        };

        match crash_info {
            None => (get_car_a_if_no_collide(time), get_car_b_if_no_collide(time)),
            Some(crash_info) => {
                if time < crash_info.time {
                    (get_car_a_if_no_collide(time), get_car_b_if_no_collide(time))
                } else {
                    (
                        get_car_a_if_no_collide(crash_info.time),
                        get_car_b_if_no_collide(crash_info.time),
                    )
                }
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self {
            car_a_initial_velocity: 30.0,
            car_a_braking_acceleration: 6.0,
            car_a_reaction_time: Duration::from_secs_f32(2.0),
            car_b_initial_velocity: 30.0,
            car_b_braking_acceleration: 6.9,
            initial_distance_between: 90.0,
            car_a_image: image::Handle::from_bytes(
                include_bytes!("../assets/Sedan-car.svg.png").as_slice(),
            ),
            car_a_image_ratio: 2_560.0 / 737.0,
            car_a_width_meters: 4.6,
            car_b_image: image::Handle::from_bytes(
                include_bytes!("../assets/Orange_sport_car.svg.png").as_slice(),
            ),
            car_b_image_ratio: 2_560.0 / 882.0,
            car_b_width_meters: 4.5,
            car_scale: 30.0,
            collision_image: image::Handle::from_bytes(
                include_bytes!("../assets/Explosion-417894_icon.svg.png").as_slice(),
            ),
            collision_image_size: Size::new(250.0, 251.0),
            collision_image_scale: 0.3,

            simulation_time: Duration::ZERO,
            running_state: RunningState::Paused,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Message {
    SetCarAInitialVelocity(f32),
    SetCarABrakingAcceleration(f32),
    SetCarAReactionTime(Duration),
    SetCarBInitialVelocity(f32),
    SetCarBBrakingAcceleration(f32),
    SetInitialDistanceBetween(f32),
    Tick(Instant),
    Start,
    Pause,
    SetTime(Duration),
}

impl App {
    fn subscription(&self) -> Subscription<Message> {
        match self.running_state {
            RunningState::Running(_) => window::frames().map(Message::Tick),
            RunningState::Paused => Subscription::none(),
        }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::SetCarAInitialVelocity(velocity) => {
                self.car_a_initial_velocity = velocity;
            }
            Message::SetCarABrakingAcceleration(acceleration) => {
                self.car_a_braking_acceleration = acceleration;
            }
            Message::SetCarAReactionTime(time) => {
                self.car_a_reaction_time = time;
            }
            Message::SetCarBInitialVelocity(velocity) => {
                self.car_b_initial_velocity = velocity;
            }
            Message::SetCarBBrakingAcceleration(acceleration) => {
                self.car_b_braking_acceleration = acceleration;
            }
            Message::SetInitialDistanceBetween(distance) => {
                self.initial_distance_between = distance;
            }
            Message::Tick(now) => match &mut self.running_state {
                RunningState::Running(last_updated) => {
                    self.simulation_time += now - *last_updated;
                    *last_updated = now;
                }
                _ => {}
            },
            Message::Start => self.running_state = RunningState::Running(Instant::now()),
            Message::Pause => {
                self.running_state = RunningState::Paused;
            }
            Message::SetTime(time) => {
                self.simulation_time = time;
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let velocity_range = 1.0..=150.0;
        let acceleration_range = 1.0..=13.0;
        let reaction_time_range = 0.1..=6.0;
        let distance_between_range = 0.1..=300.0;
        let slider_step = 0.1;

        let start_pause_button: Element<_> = match self.running_state {
            RunningState::Paused => button("Start")
                .style(button::primary)
                .on_press(Message::Start)
                .into(),
            RunningState::Running(_) => button("Pause")
                .style(button::secondary)
                .on_press(Message::Pause)
                .into(),
        };

        let view_stats = |name: &str, car: MotionState| {
            column![
                text(format!("Car {}", name)),
                text(format!("Position: {} m", car.position)),
                text(format!("Velocity: {} m/s", car.velocity)),
                text(format!("Acceleration: {} m/s^2", car.acceleration))
            ]
            .width(Fill)
        };
        let (car_a, car_b) = self.get_motion_states(self.simulation_time);

        let simulation_result_message = match self.get_crash_info() {
            Some(crash_info) => {
                let crash_reason = match crash_info.crash_type {
                    CrashType::CrashBeforeReaction => {
                        "car a crashed into car b before car a reacted"
                    }
                    CrashType::CrashAfterReactionBeforeCarBStops => {
                        "car a started braking, but it was too late"
                    }
                    CrashType::CrashAfterReactionAfterCarBStops => {
                        "car b came to a stop and car a crashed into it"
                    }
                };
                format!(
                    "at {:?}, the cars crash because {}.",
                    crash_info.time, crash_reason
                )
            }
            None => {
                format!(
                    "at {:?}, both cars come to a stop without crashing.",
                    self.get_simulation_end_time()
                )
            }
        };

        column![
            text("Settings").style(text::primary),
            row![
                column![
                    text(format!(
                        "Car a Initial Velocity: {} m/s",
                        self.car_a_initial_velocity
                    )),
                    slider(
                        velocity_range.clone(),
                        self.car_a_initial_velocity,
                        Message::SetCarAInitialVelocity
                    )
                    .step(slider_step),
                    text(format!(
                        "Car a Braking Acceleration: {} m/s^2",
                        self.car_a_braking_acceleration
                    )),
                    slider(
                        acceleration_range.clone(),
                        self.car_a_braking_acceleration,
                        Message::SetCarABrakingAcceleration
                    )
                    .step(slider_step),
                    text(format!(
                        "Car a reaction time: {:?}",
                        self.car_a_reaction_time
                    )),
                    slider(
                        reaction_time_range,
                        self.car_a_reaction_time.as_secs_f32(),
                        |time| Message::SetCarAReactionTime(Duration::from_secs_f32(time))
                    )
                    .step(slider_step),
                ],
                column![
                    text(format!(
                        "Car b Initial Velocity: {} m/s",
                        self.car_b_initial_velocity
                    )),
                    slider(
                        velocity_range,
                        self.car_b_initial_velocity,
                        Message::SetCarBInitialVelocity
                    )
                    .step(slider_step),
                    text(format!(
                        "Car b Braking Acceleration: {} m/s^2",
                        self.car_b_braking_acceleration
                    )),
                    slider(
                        acceleration_range,
                        self.car_b_braking_acceleration,
                        Message::SetCarBBrakingAcceleration
                    )
                    .step(slider_step),
                    text(format!(
                        "Initial Distance Between: {} m",
                        self.initial_distance_between
                    )),
                    slider(
                        distance_between_range,
                        self.initial_distance_between,
                        Message::SetInitialDistanceBetween
                    )
                    .step(slider_step),
                ],
            ],
            row![
                start_pause_button,
                button("Reset Time")
                    .style(button::danger)
                    .on_press(Message::SetTime(Duration::ZERO))
            ],
            text("Simulation").style(text::primary),
            canvas(self).width(Fill).height(Fill),
            text("Every tick mark is spaced 50 meters apart. Cars are not drawn to scale so that they are big enough to see. The first tick starts at the 0m position."),
            row![view_stats("a", car_a), view_stats("b", car_b)].width(Fill),
            text(format!("Distance between: {} m", car_b.position - car_a.position)),
            text(format!("Time in simulation: {:?}", self.simulation_time)),
            slider(0.0..=self.get_simulation_end_time().as_secs_f32(), self.simulation_time.as_secs_f32(), |time| Message::SetTime(Duration::from_secs_f32(time)))
                .step(Duration::from_millis(1).as_secs_f32()),
            text(format!("Simulation result: {}", simulation_result_message))
        ]
        .into()
    }
}

impl canvas::Program<Message> for App {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry<Renderer>> {
        let (car_a, car_b) = self.get_motion_states(self.simulation_time);

        // We prepare a new `Frame`
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        // Have a minimum road length shown in the simulation, extending it if necessary
        let real_width_meters = 200.0_f32.max(self.get_motion_states(Duration::MAX).1.position);
        let real_width_pixels =
            bounds.width - (self.car_a_width_meters + self.car_b_width_meters) * self.car_scale;
        // Pixels per meter of real distance (doesn't apply to cars)
        let real_scale = real_width_pixels / real_width_meters;

        let car_a_height_pixels = self.car_a_width_meters / self.car_a_image_ratio * self.car_scale;
        let car_b_height_pixels = self.car_b_width_meters / self.car_b_image_ratio * self.car_scale;
        let max_car_height_pixels = car_a_height_pixels.max(car_b_height_pixels);

        // Draw the background
        frame.fill_rectangle(Point::default(), bounds.size(), theme.palette().background);

        // Draw car a
        let car_a_width_pixels = self.car_a_width_meters * self.car_scale;
        frame.draw_image(
            Rectangle::new(
                Point::new(
                    car_a.position * real_scale,
                    max_car_height_pixels - car_a_height_pixels,
                ),
                Size::new(car_a_width_pixels, car_a_height_pixels),
            ),
            &self.car_a_image,
        );
        // Draw car b
        frame.draw_image(
            Rectangle::new(
                Point::new(
                    car_a_width_pixels + car_b.position * real_scale,
                    max_car_height_pixels - car_b_height_pixels,
                ),
                Size::new(
                    self.car_b_width_meters * self.car_scale,
                    car_b_height_pixels,
                ),
            ),
            &self.car_b_image,
        );

        // Draw collision image
        if self
            .get_crash_info()
            .is_some_and(|crash_info| self.simulation_time >= crash_info.time)
        {
            frame.draw_image(
                Rectangle::new(
                    Point::new(
                        car_a.position * real_scale + car_a_width_pixels
                            - self.collision_image_size.width * self.collision_image_scale / 2.0,
                        max_car_height_pixels
                            - car_a_height_pixels * 0.45
                            - self.collision_image_size.height * self.collision_image_scale / 2.0,
                    ),
                    self.collision_image_size * self.collision_image_scale,
                ),
                &self.collision_image,
            );
        }

        // Draw the road
        let road_thickness = 10.0;
        frame.fill_rectangle(
            Point::new(0.0, max_car_height_pixels),
            Size::new(bounds.width, road_thickness),
            theme.palette().text,
        );

        // Draw tick marks
        let tick_mark_thickness = 5.0;
        let tick_mark_height = 30.0;

        let mut draw_tick_mark = |position_meters: f32| {
            let position_pixels = car_a_width_pixels + position_meters * real_scale;
            frame.fill_rectangle(
                Point::new(
                    position_pixels - tick_mark_thickness / 2.0,
                    max_car_height_pixels + road_thickness,
                ),
                Size::new(tick_mark_thickness, tick_mark_height),
                theme.palette().primary,
            );
        };

        let every_x_meters = 50.0;
        for i in 0..=(real_width_meters / every_x_meters).ceil() as u32 {
            draw_tick_mark(i as f32 * every_x_meters)
        }

        // Then, we produce the geometry
        vec![frame.into_geometry()]
    }
}
