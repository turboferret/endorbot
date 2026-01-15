use std::{collections::{HashMap, HashSet}, process::{Command, Stdio}};

use image::{DynamicImage, EncodableLayout, GenericImage, GenericImageView, Rgba};
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rand::{seq::IndexedRandom, thread_rng};
use rten::Model;
use serde::{Deserialize, Serialize};

pub fn create_ocr_engine() -> OcrEngine {
    let recognition_model = Model::load_file("ocr/text-recognition.rten").expect("load_file");
    let detection_model = Model::load_file("ocr/text-detection.rten").expect("load_file");

    OcrEngine::new(OcrEngineParams {
        detection_model: Some(detection_model),
        recognition_model: Some(recognition_model),
        ..Default::default()
    }).expect("OcrEngine::new")
}
#[derive(Debug, Copy, Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Coords {
    x: u32,
    y: u32,
}
impl Coords {
    pub fn move_direction(&self, direction:MoveDirection) -> Self {
        match direction {
            MoveDirection::North => Self {x: self.x, y: self.y - 1},
            MoveDirection::East => Self {x: self.x + 1, y: self.y},
            MoveDirection::South => Self {x: self.x, y: self.y + 1},
            MoveDirection::West => Self {x: self.x - 1, y: self.y},
        }
    }
}
impl From<(u32, u32)> for Coords {
    fn from(value: (u32, u32)) -> Self {
        Self { x: value.0, y: value.1 }
    }
}
struct Pixel {
    x: u32,
    y: u32,
    color: Rgba<u8>,
}
impl From<(u32, u32, Rgba<u8>)> for Pixel {
    fn from(value: (u32, u32, Rgba<u8>)) -> Self {
        Self { x: value.0, y: value.1, color: value.2 }
    }
}

#[derive(Debug)]
pub enum StateError {
    UnknownState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateType {
    Ad,
    Main,
    City(bool),
    Dungeon,
}
impl Into<State> for StateType {
    fn into(self) -> State {
        State {
            state_type: self,
            dungeon: Dungeon::default(),
        }
    }
}
impl Into<State> for (StateType, Dungeon) {
    fn into(self) -> State {
        State {
            state_type: self.0,
            dungeon: self.1,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    state_type: StateType,
    dungeon: Dungeon,
}
impl Default for State {
    fn default() -> Self {
        Self { state_type: StateType::Main, dungeon: Default::default() }
    }
}

impl State {
    pub fn get_position(&self) -> Option<Coords> {
        self.dungeon.info.coordinates
    }

    pub fn merge(&mut self, old:State) -> State {
        for tile in old.dungeon.tiles {
            if let Some(new_tile) = self.dungeon.tiles.iter_mut().find(|v|v.position == tile.position) {
                new_tile.is_city = tile.is_city || new_tile.is_city;
            }
            else {
                self.dungeon.tiles.push(tile);
            }
        }
        self.clone()
    }
}

#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
enum Health {
    Unknown,
    Dead,
    Low,
    Hurt,
    Healthy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Character {
    health: Health,
}
impl Default for Character {
    fn default() -> Self {
        Self { health: Health::Unknown }
    }
}
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
struct Enemy {
    health: Health,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DungeonInfo {
    floor: String,
    coordinates: Option<Coords>,
}

fn get_info(ocr:&OcrEngine, image:&DynamicImage, old_position:Option<Coords>) -> DungeonInfo {
    let img = image.clone().sub_image(211, 1039, 365, 51).to_image();
    let img_source = ImageSource::from_bytes(img.as_bytes(), (365, 51)).expect("from_bytes");
    let ocr_input = ocr.prepare_input(img_source).expect("prepare_input");
    
    let text = ocr.get_text(&ocr_input).expect("get_text");
    let coords = if let Some(p) = text.find("(") {
        let text = &text[p+1..];
        if let Some(p) = text.find(",") {
            let x = text[..p].parse::<u32>().expect("parse");
            let y = text[p+1..text.find(")").expect("find )")].parse::<u32>().expect("parse");
            Some(Coords{x, y})
        }
        else {
            old_position
        }
    }
    else {
        old_position
    };

    DungeonInfo {
        floor: if let Some(p) = text.find(" ") {
            text[0..p].to_string()
        }
        else {
            "".to_owned()
        },
        coordinates: coords,
    }
}

const TILE_SIZE:(u32, u32) = (60, 60);
const TILE_START:(u32, u32) = (536, 536);
const TILE_COUNT:(u32, u32) = (7, 7);

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
struct Tile {
    explored: bool,
    trap: bool,
    is_city: bool,
    position: Coords,
    north_passable: bool,
    east_passable: bool,
    south_passable: bool,
    west_passable: bool,
}

impl Tile {
    fn direction_from(&self, other:Tile) -> MoveDirection {
        if self.position.x == other.position.x {
            if self.position.y > other.position.y {
                MoveDirection::South
            }
            else {
                MoveDirection::North
            }
        }
        else if self.position.x < other.position.x {
            MoveDirection::West
        }
        else {
            MoveDirection::East
        }
    }
}

fn get_tiles(info:DungeonInfo, image:&DynamicImage) -> Vec<Tile> {
    let (x_base, y_base) = if let Some(coords) = info.coordinates {
        (coords.x - (TILE_COUNT.0 + 1 ) / 2, coords.y - (TILE_COUNT.1 + 1 ) / 2 + 1)
    }
    else {
        (0, 0)
    };
    let mut tiles = Vec::new();
    for x_count in 0..TILE_COUNT.0 {
        for y_count in 0..TILE_COUNT.1 {
            let x = TILE_START.0 + x_count * TILE_SIZE.0 + TILE_SIZE.0 / 2;
            let y = TILE_START.1 + y_count * TILE_SIZE.1 + TILE_SIZE.1 / 2;

            if pixel_color(image, (x, y).into(), TILE_UNEXPLORED) {
                continue;
                //println!("{}x{}", x_base + x_count, y_base + y_count);
            }

            //println!("{x}x{} {}x{} {:?}", TILE_START.1 + y_count * TILE_SIZE.1 + TILE_SIZE.1 - 1, x_base + x_count, y_base + y_count, image.get_pixel(x, TILE_START.1 + y_count * TILE_SIZE.1 + TILE_SIZE.1 - 1));

           // println!("{x}x{y} {:?}", image.get_pixel(x, y));

            let tile = Tile {
                explored: !pixel_color(image, (x, y).into(), TILE_UNEXPLORED),
                trap: false,
                is_city: pixel_color(image, (x-2, y).into(), Rgba([244, 67, 54, 255])),
                position: Coords{x: x_base + x_count, y: y_base + y_count},
                north_passable: !pixel_color(image, (x, TILE_START.1 + y_count * TILE_SIZE.1 + 1).into(), HEALTH_GREY),
                east_passable: !pixel_color(image, (TILE_START.0 + x_count * TILE_SIZE.0 + TILE_SIZE.0 - 4, y).into(), HEALTH_GREY),
                south_passable: !pixel_color(image, (x, TILE_START.1 + y_count * TILE_SIZE.1 + TILE_SIZE.1 - 4).into(), HEALTH_GREY),
                west_passable: !pixel_color(image, (TILE_START.0 + x_count * TILE_SIZE.0 + 1, y).into(), HEALTH_GREY),
            };
            //println!("{x}x{y} {tile:?}");

            /*if 806 == x && 686 == y {
                println!("west {}x{y} {:?}", TILE_START.0 + x_count * TILE_SIZE.0 + 1, image.get_pixel(TILE_START.0 + x_count * TILE_SIZE.0 + 1, y));
                println!("east {}x{y} {:?}", TILE_START.0 + x_count * TILE_SIZE.0 + TILE_SIZE.0 - 1, image.get_pixel(TILE_START.0 + x_count * TILE_SIZE.0 + TILE_SIZE.0 - 1, y));

                println!("south {x}x{} {:?}", TILE_START.1 + y_count * TILE_SIZE.1 + TILE_SIZE.1 - 4, image.get_pixel(x, TILE_START.1 + y_count * TILE_SIZE.1 + TILE_SIZE.1 - 4));
            }*/

            tiles.push(tile);
        }
    }
    tiles
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dungeon {
    state: DungeonState,
    characters: [Character; 4],
    info: DungeonInfo,
    tiles: Vec<Tile>,
}
impl Default for Dungeon {
    fn default() -> Self {
        Self { state: DungeonState::Idle(false), characters: Default::default(), info: DungeonInfo {floor: "".to_owned(), coordinates: None}, tiles: Default::default() }
    }
}
impl Dungeon {
    fn has_low_character(&self) -> bool {
        self.characters.iter().any(|v|v.health == Health::Low)
    }
    fn has_dead_character(&self) -> bool {
        self.characters.iter().any(|v|v.health == Health::Dead)
    }

    pub fn new(ocr:&OcrEngine, state:DungeonState, image:&DynamicImage, mut explored_tiles:&mut HashMap<String, HashSet<(u32, u32)>>, old_position:Option<Coords>) -> Self {
        let info = get_info(ocr, image, old_position);
        let mut state = Self {
            state,
            characters: get_characters(image),
            info: info.clone(),
            tiles: get_tiles(info, image),
        };
        state.update_explored_tiles(&mut explored_tiles);
        state
    }
    
    fn update_explored_tiles(&mut self, mut explored_tiles:&mut HashMap<String, HashSet<(u32, u32)>>) {
        if let Some(coords) = self.info.coordinates {
            match explored_tiles.entry(self.info.floor.clone()) {
                std::collections::hash_map::Entry::Occupied(mut occupied_entry) => {
                    occupied_entry.get_mut().insert((coords.x, coords.y));
                },
                std::collections::hash_map::Entry::Vacant(vacant_entry) => {
                    let mut set = HashSet::new();
                    set.insert((coords.x, coords.y));
                    vacant_entry.insert(set);
                },
            }
        }
    }

    fn get_current_tile(&self) -> Tile {
        self.get_tile(self.info.coordinates.unwrap().x, self.info.coordinates.unwrap().y)
    }
    fn get_tile(&self, x:u32, y:u32) -> Tile {
        for tile in &self.tiles {
            if tile.position.x == x && tile.position.y == y {
                return *tile
            }
        }
        Tile {
            explored: false,
            trap: false,
            is_city: false,
            position: Coords { x, y },
            north_passable: true,
            east_passable: true,
            south_passable: true,
            west_passable: true,
        }
    }

    fn get_city_tile(&self) -> Option<Tile> {
        for tile in &self.tiles {
            if tile.is_city {
                return Some(*tile);
            }
        }
        None
    }

    fn get_random_tile_from_current(&self, avoid_position:Option<Coords>) -> Tile {
        let current = self.get_current_tile();
        let mut tiles = Vec::new();
        if current.north_passable {
            let tile = self.get_tile(current.position.x, current.position.y - 1);
            if !tile.is_city {
                tiles.push(tile);
            }
        }
        if current.east_passable {
            let tile = self.get_tile(current.position.x + 1, current.position.y);
            if !tile.is_city {
                tiles.push(tile);
            }
        }
        if current.south_passable {
            let tile = self.get_tile(current.position.x, current.position.y + 1);
            if !tile.is_city {
                tiles.push(tile);
            }
        }
        if current.west_passable {
            let tile = self.get_tile(current.position.x - 1, current.position.y);
            if !tile.is_city {
                tiles.push(tile);
            }
        }
        if tiles.len() > 1 && avoid_position.is_some() {
            tiles = tiles.iter().filter_map(|tile|{
                if tile.position == avoid_position.unwrap() {
                    None
                }
                else {
                    Some(*tile)
                }
            }).collect::<Vec<_>>();
        }
        *tiles.choose(&mut rand::rng()).unwrap()
    }
    
    fn get_next_tile_to_goal(&self, current_tile:Tile, goal:Tile) -> Option<Tile> {
        use pathfinding::prelude::astar;
        fn manhattan(a: Coords, b: Coords) -> u32 {
            ((a.x as i32 - b.x as i32).abs() + (a.y as i32 - b.y as i32).abs()) as u32
        }
        if current_tile.position == goal.position {
            return Some(current_tile);
        }
        let map: HashMap<Coords, &Tile> = self.tiles.iter().map(|t| (t.position, t)).collect();
        let successors = |pos: &Coords| -> Vec<(Coords, u32)> {
            let Some(tile) = map.get(pos) else { return vec![]; };

            let mut out = Vec::with_capacity(4);

            // Norr: y - 1 (anpassa om ditt koordinatsystem är tvärtom)
            if tile.north_passable {
                let n = Coords { x: pos.x, y: pos.y - 1 };
                if map.contains_key(&n) {
                    out.push((n, 1));
                }
            }
            // Öst: x + 1
            if tile.east_passable {
                let e = Coords { x: pos.x + 1, y: pos.y };
                if map.contains_key(&e) {
                    out.push((e, 1));
                }
            }
            // Syd: y + 1
            if tile.south_passable {
                let s = Coords { x: pos.x, y: pos.y + 1 };
                if map.contains_key(&s) {
                    out.push((s, 1));
                }
            }
            // Väst: x - 1
            if tile.west_passable {
                let w = Coords { x: pos.x - 1, y: pos.y };
                if map.contains_key(&w) {
                    out.push((w, 1));
                }
            }
            out
        };
        if let Some((path, _cost)) = astar(&current_tile.position, successors, |p|manhattan(*p, goal.position), |p|*p == goal.position) {
            println!("{path:?}");
            map.get(path.get(1).unwrap()).copied().copied()
        }
        else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DungeonState {
    Idle(bool),
    IdleChest,
    Fight(Enemy),
}

const WHITE:image::Rgba<u8> = image::Rgba([255, 255, 255, 255]);
const CITY_1:image::Rgba<u8> = image::Rgba([1, 0, 31, 255]);
const CITY_2:image::Rgba<u8> = image::Rgba([3, 2, 20, 255]);
const FIGHT:image::Rgba<u8> = image::Rgba([208, 188, 255, 255]);
const HEALTH_GREY:image::Rgba<u8> = image::Rgba([158, 158, 158, 255]);
const HEALTH_RED:image::Rgba<u8> = image::Rgba([244, 67, 54, 255]);
const HEALTH_RED_PLAYER:image::Rgba<u8> = image::Rgba([211, 47, 47, 255]);
const HEALTH_GREEN:image::Rgba<u8> = image::Rgba([56, 142, 60, 255]);
const HEALTH_ORANGE:image::Rgba<u8> = image::Rgba([245, 124, 0, 255]);

const IDLE_1:image::Rgba<u8> = image::Rgba([202, 196, 208, 255]);
const IDLE_2:image::Rgba<u8> = image::Rgba([24, 30, 49, 255]);

const TILE_UNEXPLORED:image::Rgba<u8> = image::Rgba([29, 27, 32, 255]);
const BLACK:image::Rgba<u8> = image::Rgba([0, 0, 0, 255]);

fn get_characters(image:&DynamicImage) -> [Character; 4] {
    std::array::from_fn(|i|{
        let y = 560 + i as u32 * 120;
        let health = if pixel_color(image, (514, y).into(), HEALTH_GREEN) {
            Health::Healthy
        }
        else if pixel_color(image, (291, y).into(), HEALTH_GREEN) {
            Health::Hurt
        }
        else if pixel_either_color(image, (147, y).into(), [HEALTH_RED_PLAYER, HEALTH_GREEN, HEALTH_ORANGE].into_iter()) {
            Health::Low
        }
        else if pixel_color(image, (147, y).into(), HEALTH_GREY) {
            Health::Dead
        }
        else {
            Health::Unknown
        };
        Character { health }
    })
}

fn has_dead_characters(ocr:&OcrEngine, image:&DynamicImage) -> bool {
    let img_source = ImageSource::from_bytes(image.as_bytes(), image.dimensions()).expect("from_bytes");
    let ocr_input = ocr.prepare_input(img_source).expect("prepare_input");
    
    let text = ocr.get_text(&ocr_input).expect("get_text");
    text.contains("dead")
}

fn get_enemy(image:&DynamicImage) -> Enemy {
    let x = if pixel_either_color(image, (90, 1472).into(), [HEALTH_RED, HEALTH_GREY].into_iter()) {
        89
    }
    else {
        0
    };

    Enemy {
        health: if pixel_color(image, (511 - x, 1471).into(), HEALTH_RED) {
            Health::Healthy
        }
        else if pixel_color(image, (355 - x, 1471).into(), HEALTH_RED) {
            Health::Hurt
        }
        else if pixel_color(image, (181 - x, 1471).into(), HEALTH_RED) {
            Health::Low
        }
        else if pixel_color(image, (181 - x, 1471).into(), HEALTH_GREY) {
            Health::Dead
        }
        else {
            Health::Unknown
        }
    }
}

fn pixels_color(image: &DynamicImage, pixels:impl Iterator<Item = Pixel>) -> bool {
    pixels.into_iter().all(|pixel|{
        //let c = image.get_pixel(pixel.x, pixel.y);
        //println!("{}x{} {:?} {:?}", pixel.x, pixel.y, pixel.color, c);
        image.get_pixel(pixel.x, pixel.y) == pixel.color
    })
}
fn pixels_same_color(image: &DynamicImage, pixels:impl Iterator<Item = Coords>, color: Rgba<u8>) -> bool {
    pixels.into_iter().all(|coords|{
        //let c = image.get_pixel(coords.x, coords.y);
        //println!("{}x{} {:?} {:?}", coords.x, coords.y, color, c);
        image.get_pixel(coords.x, coords.y) == color
    })
}
fn pixel_color(image: &DynamicImage, coords:Coords, color: Rgba<u8>) -> bool {
    //println!("{}x{} {:?} {:?}", coords.x, coords.y, color, image.get_pixel(coords.x, coords.y));
    image.get_pixel(coords.x, coords.y) == color
}
fn pixel_either_color(image: &DynamicImage, coords:Coords, colors: impl Iterator<Item = Rgba<u8>>) -> bool {
    let color = image.get_pixel(coords.x, coords.y);
    colors.into_iter().any(|v|v == color)
}

pub fn get_state(ocr:&OcrEngine, old_state:State, image:DynamicImage, mut explored_tiles:&mut HashMap<String, HashSet<(u32, u32)>>) -> Result<State, StateError> {
    if pixels_same_color(&image, [(918, 138).into(), (949, 138).into(), (919, 168).into(), (949, 168).into()].into_iter(), image::Rgba([202, 196, 208, 255])) {
        return Ok(Into::<State>::into(StateType::Ad).merge(old_state));
    }
    if pixel_color(&image, (466, 1116).into(), image::Rgba([185, 207, 220, 255])) && pixels_same_color(&image, [(690, 1306).into(), (717, 1326).into()].into_iter(), image::Rgba([56, 30, 114, 255])) {
        return Ok(Into::<State>::into((StateType::Dungeon, Dungeon::new(ocr, DungeonState::IdleChest, &image, explored_tiles, old_state.get_position()))).merge(old_state));
    }
    if (pixel_either_color(&image, (827, 1306).into(), [FIGHT, image::Rgba([192, 172, 241, 255])].into_iter()) ||
        pixel_either_color(&image, (827, 1260).into(), [FIGHT, image::Rgba([192, 172, 241, 255])].into_iter())) &&
        !pixel_color(&image, (671, 1309).into(), image::Rgba([56, 30, 114, 255])) {
        return Ok(Into::<State>::into((StateType::Dungeon, Dungeon::new(ocr, DungeonState::Fight(get_enemy(&image)), &image, explored_tiles, old_state.get_position()))).merge(old_state));
    }
    if pixel_color(&image, (979, 1083).into(), IDLE_1) && pixel_color(&image, (1023, 1116).into(), IDLE_1) {
        let on_city_tile = pixel_color(&image, (716, 1279).into(), FIGHT);
        return Ok(Into::<State>::into((StateType::Dungeon, Dungeon::new(ocr, DungeonState::Idle(on_city_tile), &image, explored_tiles, old_state.get_position()))).merge(old_state));
    }
    if pixels_color(&image, [(752, 1926, CITY_1).into(), (75, 1512, CITY_2).into()].into_iter()) {
        return Ok(Into::<State>::into(StateType::City(has_dead_characters(ocr, &image))).merge(old_state));
    }
    if pixels_same_color(&image, [(462, 1254).into(), (536, 1262).into(), (615, 1270).into()].into_iter(), WHITE) {
        return Ok(Into::<State>::into(StateType::Main).merge(old_state));
    }
    Err(StateError::UnknownState)
}

#[derive(Debug)]
pub enum MoveDirection {
    North,
    East,
    South,
    West,
}
#[derive(Debug)]
pub enum Action {
    CloseAd, 
    GotoTown,
    GotoDungeon,

    FindFight(MoveDirection),
    Fight,
    OpenChest,

    ReturnToTown(bool, MoveDirection),
    Resurrect,
}

pub fn determine_action(state:&State, old_position:Option<Coords>, explored_tiles:&HashMap<String, HashSet<(u32, u32)>>) -> Action {
    match state.state_type {
        StateType::Ad => {
            Action::CloseAd
        },
        StateType::Main => {
            Action::GotoTown
        },
        StateType::City(has_dead_characters) => {
            if has_dead_characters {
                Action::Resurrect
            }
            else {
                Action::GotoDungeon
            }
        },
        StateType::Dungeon => {
            let dungeon = &state.dungeon;
            match dungeon.state {
                DungeonState::Idle(on_city_tile) => {
                    if dungeon.has_low_character() || dungeon.has_dead_character() {
                        if on_city_tile {
                            Action::ReturnToTown(true, MoveDirection::East)
                        }
                        else if let Some(city_tile) = dungeon.get_city_tile() {
                            if let Some(next_tile) = dungeon.get_next_tile_to_goal(dungeon.get_current_tile(), city_tile) {
                                println!("City tile {:?}", city_tile);
                                println!("Next tile {:?}", next_tile);
                                Action::ReturnToTown(false, next_tile.direction_from(dungeon.get_current_tile()))
                            }
                            else {
                                println!("City tile {:?}", city_tile);
                                println!("Found no path to city tile");
                                let tile = dungeon.get_random_tile_from_current(None);
                                Action::ReturnToTown(false, tile.direction_from(dungeon.get_current_tile()))
                            }
                        }
                        else {
                            println!("Don't know where city tile is");
                            let tile = dungeon.get_random_tile_from_current(None);
                            Action::ReturnToTown(false, tile.direction_from(dungeon.get_current_tile()))
                        }
                    }
                    else {
                        let tile = dungeon.get_random_tile_from_current(old_position);
                        Action::FindFight(tile.direction_from(dungeon.get_current_tile()))
                    }
                },
                DungeonState::IdleChest => {
                    Action::OpenChest
                },
                DungeonState::Fight(_enemy) => {
                    if dungeon.has_low_character() || dungeon.has_dead_character() {
                        if let Some(city_tile) = dungeon.get_city_tile() {
                            if let Some(next_tile) = dungeon.get_next_tile_to_goal(dungeon.get_current_tile(), city_tile) {
                                println!("City tile {:?}", city_tile);
                                println!("Next tile {:?}", next_tile);
                                Action::ReturnToTown(false, next_tile.direction_from(dungeon.get_current_tile()))
                            }
                            else {
                                println!("City tile {:?}", city_tile);
                                println!("Found no path to city tile");
                                let tile = dungeon.get_random_tile_from_current(None);
                                Action::ReturnToTown(false, tile.direction_from(dungeon.get_current_tile()))
                            }
                        }
                        else {
                            println!("Don't know where city tile is");
                            println!("{:?}", dungeon.tiles);
                            let tile = dungeon.get_random_tile_from_current(None);
                            Action::ReturnToTown(false, tile.direction_from(dungeon.get_current_tile()))
                        }
                    }
                    else {
                        Action::Fight
                    }
                },
            }
        },
    }
}

pub fn run_action(device:&str, state:&State, action:&Action) -> Option<Coords> {
    match action {
        Action::CloseAd => {
            adb_tap(device, 935, 153);
        },
        Action::GotoTown => {

        },
        Action::GotoDungeon => {
            adb_tap(device, 890, 1928);
        },
        Action::FindFight(move_direction) => {
            adb_move(device, move_direction);
        },
        Action::Fight => {
            adb_tap(device, 711, 1308);
        },
        Action::OpenChest => {
            adb_tap(device, 798, 1312);
        },
        Action::ReturnToTown(on_city_tile, move_direction) => {
            if *on_city_tile {
                adb_tap(device, 715, 1316);
            }
            else {
                adb_move(device, move_direction);
            }
        },
        Action::Resurrect => {

        },
    }
    None
}

fn adb_move(device:&str, move_direction:&MoveDirection) {
    match move_direction {
        MoveDirection::North => adb_tap(device, 774, 2085),
        MoveDirection::East => adb_tap(device, 953, 2277),
        MoveDirection::South => adb_tap(device, 774, 2264),
        MoveDirection::West => adb_tap(device, 575, 2277),
    }
}

fn adb_input(device:&str, key:&str) {
    let cmd = Command::new("adb").arg("-s").arg(device).arg("shell").arg("input").arg("keyevent").arg(key)
    .stdin(Stdio::null())
    .stderr(Stdio::null())
    .stdout(Stdio::null())
    .spawn().unwrap().wait().unwrap();
}

fn adb_tap(device:&str, x:u32, y:u32) {
    let cmd = Command::new("adb").arg("-s").arg(device).arg("shell").arg("input").arg("tap").arg(x.to_string()).arg(y.to_string())
    .stdin(Stdio::null())
    .stderr(Stdio::null())
    .stdout(Stdio::null())
    .spawn().unwrap().wait().unwrap();
}