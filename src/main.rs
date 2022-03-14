use bevy::{
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

fn main() {
    App::new()
        .insert_resource(Msaa { samples: 4 })
        .insert_resource(ClearColor(Color::DARK_GRAY))
        .init_resource::<HexMap>()
        .add_plugins(DefaultPlugins)
        .add_plugin(SmudPlugin)
        .add_plugin(LogDiagnosticsPlugin::default())
        .add_plugin(FrameTimeDiagnosticsPlugin::default())
        .add_startup_system(setup_system)
        .add_system(player_camera_control)
        .add_system(highlight_hex)
        .run();
}

#[derive(Component)]
struct MainCamera;

fn highlight_hex(
    // need to get window dimensions
    wnds: Res<Windows>,
    // query to get camera transform
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    hex_map: Res<HexMap>,
) {
    let (camera, camera_transform) = q_camera.single();
    let wnd = wnds.get(camera.window).unwrap();
    // check if the cursor is inside the window and get its position
    if let Some(screen_pos) = wnd.cursor_position() {
        // get the size of the window
        let window_size = Vec2::new(wnd.width() as f32, wnd.height() as f32);

        // convert screen position [0..resolution] to ndc [-1..1] (gpu coordinates)
        let ndc = (screen_pos / window_size) * 2.0 - Vec2::ONE;

        // matrix for undoing the projection and camera transform
        let ndc_to_world = camera_transform.compute_matrix() * camera.projection_matrix.inverse();

        // use it to convert ndc to world-space coordinates
        let world_pos = ndc_to_world.project_point3(ndc.extend(-1.0));

        // reduce it to a 2D value
        let world_pos: Vec2 = world_pos.truncate();

        let axial = screen_to_axial_float(HEX_SIZE, world_pos);
        eprintln!("World coords: ({}, {}),  Axial: ({}, {})", world_pos.x, world_pos.y, axial.x, axial.y);
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
    let dimensions = hex_dimensions(HEX_SIZE);
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
                    color: Color::GRAY,
                    sdf: hexagon.clone(),
                    frame: Frame::Quad(HEX_SIZE),
                    fill: SIMPLE_FILL_HANDLE.typed(),
                },
                transform: Transform::from_translation((center, 0.).into()),
                ..Default::default()
            });
            hex_map.0.insert(coord, ent.id());
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

#[derive(Component, PartialEq, Eq, Hash)]
struct AxialCoordinate {
    q: i32,
    r: i32,
}

impl AxialCoordinate {
    fn get_s(&self) -> i32 {
        -self.q - self.r
    }

    fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }
}

impl From<&AxialCoordinate> for Vec2 {
    fn from(ac: &AxialCoordinate) -> Self {
        Vec2::new(ac.q as f32, ac.r as f32)
    }
}


fn axial_to_screen(size: f32, hex_pos: &AxialCoordinate) -> Vec2 {
    const FLAT_BASIS_C: [f32; 4] = [3. / 2., SQRT_3 / 2., 0., SQRT_3];
    let flat_basis: Mat2 = Mat2::from_cols_array(&FLAT_BASIS_C);
    size * flat_basis * Vec2::from(hex_pos)
}

fn screen_to_axial_float(size: f32, point: Vec2) -> Vec2 {
    const FLAT_BASIS_C: [f32; 4] = [2. / 3., -1. / 3., 0., SQRT_3 / 3.];
    let flat_basis: Mat2 = Mat2::from_cols_array(&FLAT_BASIS_C);
    flat_basis * point / size
}
