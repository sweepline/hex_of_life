use bevy::{
    core::FixedTimestep,
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    math::Mat2,
    prelude::*,
    utils::HashMap,
};
use bevy_smud::prelude::*;

const CAMERA_SPEED_PER_SEC: f32 = 2.0;
const SQRT_3: f32 = 1.732_050_8;
const HEX_SIZE: f32 = 10.;

#[derive(Default)]
struct HexMap(HashMap<AxialCoordinate, Entity>);

struct GameState {
    pub started: bool,
}

fn main() {
    App::new()
        .insert_resource(Msaa { samples: 4 })
        .insert_resource(ClearColor(Color::DARK_GRAY))
        .insert_resource(GameState { started: false })
        .init_resource::<HexMap>()
        .add_plugins(DefaultPlugins)
        .add_plugin(SmudPlugin)
        .add_plugin(LogDiagnosticsPlugin::default())
        // .add_plugin(FrameTimeDiagnosticsPlugin::default())
        .add_startup_system(setup_system)
        .add_system(player_camera_control)
        .add_system(highlight_hex)
        .add_system(color_hex)
        .add_system(game_control)
        .add_system_set(
            SystemSet::new()
                .with_run_criteria(FixedTimestep::step(0.5))
                .with_system(game_of_life),
        )
        .run();
}

#[derive(Component)]
struct MainCamera;
#[derive(Component)]
struct Alive;
#[derive(Component)]
struct Dead;

fn game_control(buttons: Res<Input<KeyCode>>, mut game_state: ResMut<GameState>) {
    if !buttons.just_pressed(KeyCode::Return) {
        return;
    }
    game_state.started = !game_state.started;
}

fn game_of_life(
    game_state: Res<GameState>,
    mut query: Query<(Entity, &mut SmudShape, &Transform), With<Alive>>,
    mut commands: Commands,
    hex_map: Res<HexMap>,
) {
    if !game_state.started {
        return;
    }
    let mut dead_alive_neighbours: HashMap<Entity, u8> = HashMap::default();
    for (entity, _shape, trans) in query.iter() {
        let cube_float = screen_to_cube_float(HEX_SIZE, trans.translation.truncate());
        let cube = cube_round(cube_float);
        let axial: AxialCoordinate = cube.into();
        let mut alive_alive_neighbours = 0;
        for axial_neighbour in axial.neighbour_iter() {
            // Lookup neighbours (maybe just put them in as component?) and check if they are alive
            if let Some(entity_neighbour) = hex_map.0.get(&axial_neighbour) {
                if query.get(*entity_neighbour).is_ok() {
                    // This neighbour is alive and we should just count it to ourselves
                    // as we will iterate over this one too.
                    alive_alive_neighbours += 1;
                } else {
                    // This neighbour is dead so we add ourself to its alive neighbour count.
                    let dead = dead_alive_neighbours.entry(*entity_neighbour).or_insert(0);
                    *dead += 1;
                }
            }
        }
        if alive_alive_neighbours != 2 {
            commands.entity(entity).remove::<Alive>();
        }
    }
    for (e, n) in dead_alive_neighbours {
        if n == 2 {
            commands.entity(e).insert(Alive);
        }
    }
}

fn highlight_hex(
    // need to get window dimensions
    wnds: Res<Windows>,
    buttons: Res<Input<MouseButton>>,
    // query to get camera transform
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut q_alive: Query<(), With<Alive>>,
    hex_map: Res<HexMap>,
    mut commands: Commands,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let (camera, camera_transform) = q_camera.single();
    let wnd = wnds.get(camera.window).unwrap();
    if let Some(screen_pos) = wnd.cursor_position() {
        let window_size = Vec2::new(wnd.width() as f32, wnd.height() as f32);
        // convert screen position [0..resolution] to ndc [-1..1] (gpu coordinates)
        let ndc = (screen_pos / window_size) * 2.0 - Vec2::ONE;
        // matrix for undoing the projection and camera transform
        let ndc_to_world = camera_transform.compute_matrix() * camera.projection_matrix.inverse();
        // use it to convert ndc to world-space coordinates
        let world_pos = ndc_to_world.project_point3(ndc.extend(-1.0));
        // reduce it to a 2D value
        let world_pos: Vec2 = world_pos.truncate();

        // Make the coordinate into an axial coordinate for getting a hex by hashmap
        let cube_float = screen_to_cube_float(HEX_SIZE, world_pos);
        let cube = cube_round(cube_float);
        let axial: AxialCoordinate = cube.into();
        if let Some(hex_entity) = hex_map.0.get(&axial) {
            if q_alive.get_mut(*hex_entity).is_ok() {
                commands.entity(*hex_entity).remove::<Alive>();
            } else {
                commands.entity(*hex_entity).insert(Alive);
            }
        }
    }
}

fn color_hex(mut query: Query<(&mut SmudShape, Option<&Alive>)>) {
    for (mut hex, alive) in query.iter_mut() {
        if alive.is_some() {
            hex.color = Color::GRAY;
        } else {
            hex.color = Color::BLACK;
        }
    }
}

fn player_camera_control(
    kb: Res<Input<KeyCode>>,
    time: Res<Time>,
    mut query: Query<&mut OrthographicProjection, With<MainCamera>>,
) {
    let dist = CAMERA_SPEED_PER_SEC * time.delta().as_secs_f32();

    for mut projection in query.iter_mut() {
        let mut log_scale = projection.scale.ln();

        if kb.pressed(KeyCode::PageUp) {
            log_scale -= dist;
        }
        if kb.pressed(KeyCode::PageDown) {
            log_scale += dist;
        }

        projection.scale = log_scale.exp();
    }
}

fn setup_system(
    mut commands: Commands,
    mut shaders: ResMut<Assets<Shader>>,
    wnds: Res<Windows>,
    mut hex_map: ResMut<HexMap>,
) {
    let hexagon = shaders.add_sdf_expr("sd_hexagon(p, 8.)");
    let mut half_width: f32 = 0.;
    let mut half_height: f32 = 0.;
    if let Some(wnd) = wnds.get_primary() {
        half_width = wnd.width() / 2.;
        half_height = wnd.height() / 2.;
    }

    commands
        .spawn_bundle(OrthographicCameraBundle::new_2d())
        .insert(MainCamera);
    for q in -100..100 {
        for r in -100..100 {
            let coord = AxialCoordinate::new(q, r);
            let center = axial_to_screen(HEX_SIZE, &coord);
            if center.x < -half_width || center.x > half_width {
                continue;
            }
            if center.y < -half_height || center.y > half_height {
                continue;
            }
            let ent = commands.spawn_bundle(ShapeBundle {
                shape: SmudShape {
                    color: Color::BLACK,
                    sdf: hexagon.clone(),
                    frame: Frame::Quad(HEX_SIZE),
                    fill: SIMPLE_FILL_HANDLE.typed(),
                },
                transform: Transform::from_translation((center, 0.).into()),
                ..Default::default()
            });
            hex_map.0.insert(coord, ent.id().clone());
        }
    }
}

struct HexDimensions {
    width: f32,
    height: f32,
}

fn hex_dimensions(size: f32) -> HexDimensions {
    HexDimensions {
        width: size * 2.0,
        height: 3f32.sqrt() * size,
    }
}

#[derive(Copy, Clone, Component, PartialEq, Eq, Hash)]
struct AxialCoordinate(IVec2);
impl AxialCoordinate {
    fn q(&self) -> i32 {
        self.0.x
    }
    fn r(&self) -> i32 {
        self.0.y
    }
    fn s(&self) -> i32 {
        -self.q() - self.r()
    }

    fn new(q: i32, r: i32) -> Self {
        Self(IVec2::new(q, r))
    }

    fn neighbour_iter(&self) -> impl Iterator<Item = AxialCoordinate> + '_ {
        let neighbour_possibilities = vec![
            IVec2::new(1, 0),
            IVec2::new(1, -1),
            IVec2::new(0, -1),
            IVec2::new(-1, 0),
            IVec2::new(-1, 1),
            IVec2::new(0, 1),
        ];
        neighbour_possibilities
            .into_iter()
            .map(|n| AxialCoordinate(self.0 + n))
    }
}

impl From<CubeCoordinate> for AxialCoordinate {
    fn from(cc: CubeCoordinate) -> Self {
        AxialCoordinate::new(cc.q(), cc.r())
    }
}

#[derive(Copy, Clone, Component, PartialEq, Eq, Hash)]
struct CubeCoordinate(IVec3);
impl CubeCoordinate {
    fn q(&self) -> i32 {
        self.0.x
    }
    fn r(&self) -> i32 {
        self.0.y
    }
    fn s(&self) -> i32 {
        self.0.z
    }

    fn new(q: i32, r: i32, s: i32) -> Self {
        Self(IVec3::new(q, r, s))
    }
}

fn axial_to_screen(size: f32, hex_pos: &AxialCoordinate) -> Vec2 {
    const FLAT_BASIS_C: [f32; 4] = [3. / 2., SQRT_3 / 2., 0., SQRT_3];
    let flat_basis: Mat2 = Mat2::from_cols_array(&FLAT_BASIS_C);
    size * flat_basis * hex_pos.0.as_vec2()
}

fn screen_to_cube_float(size: f32, point: Vec2) -> Vec3 {
    const FLAT_BASIS_C: [f32; 4] = [2. / 3., -1. / 3., 0., SQRT_3 / 3.];
    let flat_basis: Mat2 = Mat2::from_cols_array(&FLAT_BASIS_C);
    let axial_float = flat_basis * point / size;
    Vec3::from((axial_float, -axial_float.x - axial_float.y))
}

fn cube_round(cube: Vec3) -> CubeCoordinate {
    let rounded_cube = cube.round();
    let diff = (rounded_cube - cube).abs();

    let mut ret_cube: IVec3 = rounded_cube.as_ivec3();
    if diff.x > diff.y && diff.x > diff.z {
        ret_cube.x = -ret_cube.y - ret_cube.z;
    } else if diff.y > diff.z {
        ret_cube.y = -ret_cube.x - ret_cube.z;
    } else {
        ret_cube.z = -ret_cube.x - ret_cube.y;
    }

    CubeCoordinate(ret_cube)
}
