use {
    crate::fifo::client::{wp_fifo_manager_v1::WpFifoManagerV1, wp_fifo_v1::WpFifoV1},
    memfile::MemFile,
    std::{
        io::Write,
        time::{Duration, Instant},
    },
    wayland_client::{
        delegate_noop,
        protocol::{
            wl_buffer,
            wl_callback::{self, WlCallback},
            wl_compositor,
            wl_keyboard::{self, KeyState},
            wl_registry, wl_seat,
            wl_shm::{Format, WlShm},
            wl_shm_pool::WlShmPool,
            wl_subcompositor,
            wl_subsurface::WlSubsurface,
            wl_surface,
        },
        Connection, Dispatch, QueueHandle, WEnum,
    },
    wayland_protocols::{
        wp::viewporter::client::{wp_viewport::WpViewport, wp_viewporter},
        xdg::{
            decoration::zv1::client::{
                zxdg_decoration_manager_v1::ZxdgDecorationManagerV1,
                zxdg_toplevel_decoration_v1::{self, ZxdgToplevelDecorationV1},
            },
            shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
        },
    },
    wl_buffer::WlBuffer,
    wl_compositor::WlCompositor,
    wl_subcompositor::WlSubcompositor,
    wl_surface::WlSurface,
    wp_viewporter::WpViewporter,
    xdg_surface::XdgSurface,
    xdg_toplevel::XdgToplevel,
    xdg_wm_base::XdgWmBase,
};

mod fifo {
    pub mod client {
        use wayland_client::{self, protocol::*};
        pub mod __interfaces {
            use wayland_client::protocol::__interfaces::*;
            wayland_scanner::generate_interfaces!("fifo-v1.xml");
        }
        use self::__interfaces::*;
        wayland_scanner::generate_client_code!("fifo-v1.xml");
    }
}

fn main() {
    let conn = Connection::connect_to_env().unwrap();

    let mut event_queue = conn.new_event_queue();
    let qhandle = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qhandle, ());

    display.sync(&qhandle, InitialRoundtrip);

    println!("Press ESC to exit.");
    println!("Press SPACE to switch between fifo and mailbox.");
    println!();

    let mut state = State {
        running: true,
        wm_base: None,
        wl_compositor: None,
        wl_shm: None,
        wp_viewporter: None,
        wl_subcompositor: None,
        zxdg_decoration_manager_v1: None,
        wp_fifo_manager_v1: None,
        objects: None,
    };

    while state.running {
        event_queue.blocking_dispatch(&mut state).unwrap();
    }
}

struct State {
    running: bool,
    wm_base: Option<XdgWmBase>,
    wl_compositor: Option<WlCompositor>,
    wl_shm: Option<WlShm>,
    wp_viewporter: Option<WpViewporter>,
    wl_subcompositor: Option<WlSubcompositor>,
    zxdg_decoration_manager_v1: Option<ZxdgDecorationManagerV1>,
    wp_fifo_manager_v1: Option<WpFifoManagerV1>,
    objects: Option<Objects>,
}

struct Objects {
    start: Instant,
    surface1: (WlSurface, WpViewport, Option<WpFifoV1>),
    surface2: (WlSurface, WpViewport, WlSubsurface),
    _xdg_surface: XdgSurface,
    _xdg_toplevel: XdgToplevel,
    white: [WlBuffer; 3],
    free: [bool; 3],
    black: WlBuffer,
    width: i32,
    height: i32,
    frames_rendered: usize,
    log_frame_at: Instant,
    fifo: bool,
}

impl Objects {
    fn render_frame(&mut self) {
        if self.width == 0 || self.height == 0 {
            return;
        }

        let Some(idx) = self.free.iter().position(|v| *v) else {
            return;
        };
        self.free[idx] = false;

        let now = Instant::now();
        if now > self.log_frame_at {
            const SECONDS: u64 = 3;
            println!(
                "rendering at {} FPS",
                (self.frames_rendered as f64 / SECONDS as f64).floor()
            );
            self.log_frame_at = now + Duration::from_secs(SECONDS);
            self.frames_rendered = 0;
        }
        self.frames_rendered += 1;

        let s2_width = self.width / 5;
        let s2_height = self.height / 5;

        let frame = self.start.elapsed().as_millis() as f64 / 500.0;
        let x_pos = (((frame.sin() + 1.0) / 2.0) * (self.width - s2_width) as f64) as i32;
        let y_pos = (((frame.cos() + 1.0) / 2.0) * (self.height - s2_height) as f64) as i32;

        self.surface2.1.set_source(0.0, 0.0, 1.0, 1.0);
        self.surface2.1.set_destination(s2_width, s2_height);
        self.surface2.2.set_position(x_pos, y_pos);
        self.surface2.0.attach(Some(&self.black), 0, 0);
        self.surface2.0.commit();

        self.surface1.1.set_source(0.0, 0.0, 1.0, 1.0);
        self.surface1.1.set_destination(self.width, self.height);
        self.surface1.0.attach(Some(&self.white[idx]), 0, 0);
        if self.fifo {
            if let Some(fifo) = &self.surface1.2 {
                fifo.set_barrier();
                fifo.wait_barrier();
            }
        }
        self.surface1.0.commit();

        self.render_frame();
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            match &interface[..] {
                "wl_compositor" => {
                    state.wl_compositor =
                        Some(registry.bind::<WlCompositor, _, _>(name, 4, qh, ()));
                }
                "wl_subcompositor" => {
                    state.wl_subcompositor =
                        Some(registry.bind::<WlSubcompositor, _, _>(name, 1, qh, ()));
                }
                "zxdg_decoration_manager_v1" => {
                    state.zxdg_decoration_manager_v1 =
                        Some(registry.bind::<ZxdgDecorationManagerV1, _, _>(name, 1, qh, ()));
                }
                "wl_shm" => {
                    state.wl_shm = Some(registry.bind::<WlShm, _, _>(name, 1, qh, ()));
                }
                "wl_seat" => {
                    registry.bind::<wl_seat::WlSeat, _, _>(name, 1, qh, ());
                }
                "wp_viewporter" => {
                    state.wp_viewporter =
                        Some(registry.bind::<WpViewporter, _, _>(name, 1, qh, ()));
                }
                "xdg_wm_base" => {
                    state.wm_base = Some(registry.bind::<XdgWmBase, _, _>(name, 1, qh, ()));
                }
                "wp_fifo_manager_v1" => {
                    state.wp_fifo_manager_v1 =
                        Some(registry.bind::<WpFifoManagerV1, _, _>(name, 1, qh, ()));
                }
                _ => {}
            }
        }
    }
}

struct InitialRoundtrip;

impl Dispatch<WlCallback, InitialRoundtrip> for State {
    fn event(
        state: &mut Self,
        _proxy: &WlCallback,
        _event: wl_callback::Event,
        _data: &InitialRoundtrip,
        _conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        let comp = state.wl_compositor.as_ref().expect("wl_compositor");
        let wm_base = state.wm_base.as_ref().expect("wm_base");
        let shm = state.wl_shm.as_ref().expect("wl_shm");
        let sub = state.wl_subcompositor.as_ref().expect("wl_subcompositor");
        let viewporter = state.wp_viewporter.as_ref().expect("wp_viewporter");
        let buffer = |color: [u8; 4], id: Option<usize>| {
            let mut map = MemFile::create_default("color").unwrap();
            map.write_all(&color).unwrap();
            let pool = shm.create_pool(map.as_fd(), 4, qhandle, ());
            pool.create_buffer(0, 1, 1, 4, Format::Argb8888, qhandle, id)
        };
        let create_surface = || {
            let surface = comp.create_surface(qhandle, ());
            let viewport = viewporter.get_viewport(&surface, qhandle, ());
            let fifo = state
                .wp_fifo_manager_v1
                .as_ref()
                .map(|m| m.get_fifo(&surface, qhandle, ()));
            (surface, viewport, fifo)
        };
        let create_subsurface = |parent: &WlSurface| {
            let (surface, viewport, _) = create_surface();
            let ss = sub.get_subsurface(&surface, parent, qhandle, ());
            ss.set_sync();
            (surface, viewport, ss)
        };
        let (surface1, viewport1, fifo1) = create_surface();
        let (surface2, viewport2, subsurface2) = create_subsurface(&surface1);
        let xdg_surface = wm_base.get_xdg_surface(&surface1, qhandle, ());
        let xdg_toplevel = xdg_surface.get_toplevel(qhandle, ());
        if let Some(decoman) = state.zxdg_decoration_manager_v1.as_ref() {
            let decorations = decoman.get_toplevel_decoration(&xdg_toplevel, qhandle, ());
            decorations.set_mode(zxdg_toplevel_decoration_v1::Mode::ServerSide);
        }
        surface1.commit();
        state.objects = Some(Objects {
            start: Instant::now(),
            surface1: (surface1, viewport1, fifo1),
            surface2: (surface2, viewport2, subsurface2),
            _xdg_surface: xdg_surface,
            _xdg_toplevel: xdg_toplevel,
            white: [
                buffer([255, 255, 255, 255], Some(0)),
                buffer([255, 255, 255, 255], Some(1)),
                buffer([255, 255, 255, 255], Some(2)),
            ],
            free: [true, true, true],
            black: buffer([0, 0, 0, 255], None),
            width: 0,
            height: 0,
            frames_rendered: 0,
            log_frame_at: Instant::now(),
            fifo: true,
        });
    }
}

delegate_noop!(State: ignore WlCompositor);
delegate_noop!(State: ignore WlSurface);
delegate_noop!(State: ignore WpViewporter);
delegate_noop!(State: ignore WlSubsurface);
delegate_noop!(State: ignore WpViewport);
delegate_noop!(State: ignore WlSubcompositor);
delegate_noop!(State: ignore ZxdgDecorationManagerV1);
delegate_noop!(State: ignore ZxdgToplevelDecorationV1);
delegate_noop!(State: ignore WlShm);
delegate_noop!(State: ignore WlShmPool);
delegate_noop!(State: ignore WpFifoManagerV1);
delegate_noop!(State: ignore WpFifoV1);

impl Dispatch<WlBuffer, Option<usize>> for State {
    fn event(
        state: &mut Self,
        _proxy: &WlBuffer,
        _event: wl_buffer::Event,
        data: &Option<usize>,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        if let Some(idx) = *data {
            if let Some(obj) = &mut state.objects {
                obj.free[idx] = true;
                obj.render_frame();
            }
        }
    }
}

impl Dispatch<XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        wm_base: &XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<XdgSurface, ()> for State {
    fn event(
        state: &mut Self,
        xdg_surface: &XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial, .. } = event {
            xdg_surface.ack_configure(serial);
            let obj = state.objects.as_mut().unwrap();
            obj.render_frame();
        }
    }
}

impl Dispatch<XdgToplevel, ()> for State {
    fn event(
        state: &mut Self,
        _: &XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use xdg_toplevel::Event;

        match event {
            Event::Configure { width, height, .. } => {
                let obj = state.objects.as_mut().unwrap();
                obj.width = width.max(100);
                obj.height = height.max(100);
            }
            Event::Close => state.running = false,
            Event::ConfigureBounds { .. } => {}
            Event::WmCapabilities { .. } => {}
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
        {
            if capabilities.contains(wl_seat::Capability::Keyboard) {
                seat.get_keyboard(qh, ());
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key {
            key,
            state: key_state,
            ..
        } = event
        {
            if key_state.into_result().unwrap() != KeyState::Pressed {
                return;
            }
            match key {
                1 => state.running = false,
                57 => {
                    if let Some(obj) = &mut state.objects {
                        println!();
                        obj.fifo = !obj.fifo;
                        let name = match obj.fifo {
                            true => match obj.surface1.2.is_some() {
                                true => "fifo",
                                _ => "fifo (unavailable)",
                            },
                            false => "mailbox",
                        };
                        println!("presenting with {name}");
                        println!();
                    }
                }
                _ => {}
            }
        }
    }
}
