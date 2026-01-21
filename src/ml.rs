use std::{collections::{HashMap, HashSet}, io::Write, process::{Command, Stdio}};

use image::{DynamicImage, EncodableLayout, GenericImage, GenericImageView, Rgb, Rgba};
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rand::{seq::{IndexedRandom, IteratorRandom}, thread_rng};
use rten::Model;
use serde::{Deserialize, Serialize};

use crate::Opt;

#[derive(Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, PartialEq)]
pub struct Bitmap {
    pixels: Vec<(u16, u16, [u8;3])>,
    has_dead_characters: bool,
    info: DungeonInfo,
}
impl Bitmap {
    pub fn get_pixel(&self, x:u16, y:u16) -> &[u8; 3] {
        self.pixels.iter().find_map(|(px, py, color)|if (x, y) == (*px, *py){Some(color)}else{None}).expect(&format!("{x}x{y} not found"))
    }
    pub fn set_pixel(&mut self, x:u16, y:u16, color:[u8;3]) {
        self.pixels.push((x, y, color));
    }
    pub fn with_capacity(capacity:usize) -> Self {
        Self {
            pixels: Vec::with_capacity(capacity),
            info: DungeonInfo {
                floor: "".to_owned(),
                coordinates: None,
            },
            has_dead_characters: false,
        }
    }
    pub fn set_has_dead_characters(&mut self, has_dead_characters:bool) {
        self.has_dead_characters = has_dead_characters;
    }
    pub fn set_info(&mut self, info:DungeonInfo) {
        self.info = info;
    }
    pub fn get_has_dead_characters(&self) -> bool {
        self.has_dead_characters
    }
    pub fn get_info(&self) -> &DungeonInfo {
        &self.info
    }
}

pub fn create_ocr_engine() -> OcrEngine {
    let recognition_model = Model::load_file("ocr/text-recognition.rten").expect("load_file");
    let detection_model = Model::load_file("ocr/text-detection.rten").expect("load_file");

    OcrEngine::new(OcrEngineParams {
        detection_model: Some(detection_model),
        recognition_model: Some(recognition_model),
        ..Default::default()
    }).expect("OcrEngine::new")
}
#[derive(Debug, Copy, Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Coords {
    pub x: u32,
    pub y: u32,
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
    color: Rgb<u8>,
}
impl From<(u32, u32, Rgb<u8>)> for Pixel {
    fn from(value: (u32, u32, Rgb<u8>)) -> Self {
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
    pub state_type: StateType,
    pub dungeon: Dungeon,
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
        let city_tile = self.dungeon.tiles.iter().find(|tile|tile.is_city).cloned();
        for mut tile in old.dungeon.tiles {
            if let Some(new_tile) = self.dungeon.tiles.iter_mut().find(|v|v.position == tile.position) {
                if city_tile.is_none() {
                    new_tile.is_city = tile.is_city || new_tile.is_city;
                }
            }
            else {
                tile.is_city = if city_tile.is_none() {
                    tile.is_city 
                }
                else {
                    false
                };
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
pub struct Character {
    health: Health,
}
impl Default for Character {
    fn default() -> Self {
        Self { health: Health::Unknown }
    }
}
impl Character {
    pub fn is_dead(&self) -> bool {
        if let Health::Dead = self.health {
            true
        }
        else {
            false
        }
    }
}
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct Enemy {
    health: Health,
}

#[derive(Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, PartialEq)]
pub struct DungeonInfo {
    pub floor: String,
    pub coordinates: Option<Coords>,
}

pub fn get_info(ocr:&OcrEngine, image:&DynamicImage, old_position:Option<Coords>) -> DungeonInfo {
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
pub struct Tile {
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
    pub fn get_position(&self) -> Coords {
        self.position
    }
}

fn get_tiles(info:&DungeonInfo, image:&Bitmap) -> Vec<Tile> {
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
                is_city: pixel_color(image, (x-2, y).into(), Rgb([244, 67, 54])),
                position: Coords{x: x_base + x_count, y: y_base + y_count},
                north_passable: !pixel_color(image, (x, TILE_START.1 + y_count * TILE_SIZE.1 + 1).into(), HEALTH_GREY),
                east_passable: !pixel_color(image, (TILE_START.0 + x_count * TILE_SIZE.0 + TILE_SIZE.0 - 4, y).into(), HEALTH_GREY),
                south_passable: !pixel_color(image, (x, TILE_START.1 + y_count * TILE_SIZE.1 + TILE_SIZE.1 - 4).into(), HEALTH_GREY),
                west_passable: !pixel_color(image, (TILE_START.0 + x_count * TILE_SIZE.0 + 1, y).into(), HEALTH_GREY),
            };

            if pixel_color(image, (TILE_START.0 + x_count * TILE_SIZE.0 + 1, y).into(), TILE_UNEXPLORED) && !pixel_color(image, (x, y).into(), TILE_UNEXPLORED) {
                continue;
            }
            
            /*if tile.position.x == 16 && tile.position.y == 15 {
                if tile.west_passable {
                    println!("{tile:?} {}x{y} {:?} {x}x{y} {:?}", TILE_START.0 + x_count * TILE_SIZE.0 + 1, image.get_pixel(TILE_START.0 + x_count * TILE_SIZE.0 + TILE_SIZE.0 - 4, y), image.get_pixel(x, y));
                    panic!();
                }
            }*/
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

#[derive(Debug)]
enum RandomTarget {
    City,
    Unexplored,
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

    pub fn new(state:DungeonState, image:&Bitmap, old_position:Option<Coords>) -> Self {
        let state = Self {
            state,
            characters: get_characters(image),
            info: if let Some(p) = image.info.coordinates {
                image.info.clone()
            }
            else {
                DungeonInfo {
                    floor: image.info.floor.to_owned(),
                    coordinates: old_position,
                }
            },
            tiles: get_tiles(&image.info, image),
        };
        state
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

    fn get_random_tile_from_current(&self, avoid_position:Option<Coords>, random_target:RandomTarget) -> Tile {
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
        if tiles.len() > 1 {
            match random_target {
                RandomTarget::City => {
                    if let Some(city_tile) = tiles.iter().find(|tile|tile.is_city) {
                        tiles = vec![*city_tile];
                    }
                },
                RandomTarget::Unexplored => {
                    let unexplored_tiles = tiles.iter().filter_map(|tile|{
                        if tile.north_passable && !self.get_tile(tile.position.x, tile.position.y - 1).explored
                            || tile.south_passable && !self.get_tile(tile.position.x, tile.position.y + 1).explored
                            || tile.east_passable && !self.get_tile(tile.position.x + 1, tile.position.y).explored
                            || tile.west_passable && !self.get_tile(tile.position.x - 1, tile.position.y).explored {
                            Some(*tile)
                        }
                        else {
                            None
                        }
                    }).collect::<Vec<_>>();
                    if !unexplored_tiles.is_empty() {
                        tiles = unexplored_tiles;
                    }
                },
            }
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
        //let map: HashMap<Coords, &Tile> = self.tiles.iter().map(|t| (t.position, t)).collect();
        let successors = |pos: &Coords| -> Vec<(Coords, u32)> {
            let tile = self.get_tile(pos.x, pos.y);

            let mut out = Vec::with_capacity(4);

            // Norr: y - 1 (anpassa om ditt koordinatsystem är tvärtom)
            if tile.north_passable {
                let n = Coords { x: pos.x, y: pos.y - 1 };
                    out.push((n, 1));
            }
            // Öst: x + 1
            if tile.east_passable {
                let e = Coords { x: pos.x + 1, y: pos.y };
                    out.push((e, 1));
            }
            // Syd: y + 1
            if tile.south_passable {
                let s = Coords { x: pos.x, y: pos.y + 1 };
                    out.push((s, 1));
            }
            // Väst: x - 1
            if tile.west_passable {
                let w = Coords { x: pos.x - 1, y: pos.y };
                    out.push((w, 1));
            }
            out
        };
        if let Some((path, _cost)) = astar(&current_tile.position, successors, |p|manhattan(*p, goal.position), |p|*p == goal.position) {
            //println!("{path:?}");
            let pos = path.get(1).unwrap();
            Some(self.get_tile(pos.x, pos.y))
        }
        else {
            None
        }
    }

    fn get_closest_unexplored_tile(&self, current_tile:Tile) -> Option<Tile> {
        use pathfinding::prelude::astar;
        //let map: HashMap<Coords, &Tile> =
            //self.tiles.iter().map(|t| (t.position, t)).collect();

        let successors = |pos: &Coords| -> Vec<(Coords, u32)> {
            //let Some(tile) = map.get(pos) else { return vec![]; };
            let tile = self.get_tile(pos.x, pos.y);
            let mut out = Vec::with_capacity(4);
            if tile.north_passable {
                let n = Coords { x: pos.x, y: pos.y - 1 };
                //if map.contains_key(&n) {
                    out.push((n, 1));
                //}
            }
            if tile.east_passable {
                let e = Coords { x: pos.x + 1, y: pos.y };
                //if map.contains_key(&e) {
                    out.push((e, 1));
                //}
            }
            if tile.south_passable {
                let s = Coords { x: pos.x, y: pos.y + 1 };
                //if map.contains_key(&s) {
                    out.push((s, 1));
                //}
            }
            if tile.west_passable {
                let w = Coords { x: pos.x - 1, y: pos.y };
                //if map.contains_key(&w) {
                    out.push((w, 1));
                //}
            }

            out
        };

        let is_goal = |pos: &Coords| {
            !self.get_tile(pos.x, pos.y).explored
            //map.get(pos).map_or(false, |t| !t.explored)
        };

        if let Some(result) = astar(
            &current_tile.position,
            successors,
            |_| 0u32,
            is_goal,
        ) {
            //println!("astar {result:?}");
            if !result.0.is_empty() {
                let pos = result.0.last().unwrap();
                return Some(self.get_tile(pos.x, pos.y));
            }
        }
        else {
            println!("found no unexplored tile");
        }
        None
    }
    
    fn get_unexplored_tile(&self, old_position: Option<Coords>) -> Tile {
        let me = self.get_current_tile();
        if let Some(tile) = self.get_closest_unexplored_tile(me) {
            return tile;
        }
        if me.west_passable && me.position.x > 0 {
            let tile = self.get_tile(me.position.x - 1, me.position.y);
            if !tile.explored {
                return tile;
            }
        }
        if me.east_passable {
            let tile = self.get_tile(me.position.x + 1, me.position.y);
            if !tile.explored {
                return tile;
            }
        }
        if me.north_passable && me.position.y > 0 {
            let tile = self.get_tile(me.position.x, me.position.y - 1);
            if !tile.explored {
                return tile;
            }
        }
        if me.south_passable {
            let tile = self.get_tile(me.position.x, me.position.y + 1);
            if !tile.explored {
                return tile;
            }
        }
        if let Some(tile) = self.tiles.iter().filter(|tile|self.has_unexplored_neighbour(tile)).choose(&mut rand::rng()) {
            return *tile;
        }
        self.get_random_tile_from_current(old_position, RandomTarget::Unexplored)
    }
    
    fn has_unexplored_neighbour(&self, tile: &Tile) -> bool {
        if tile.north_passable && tile.position.y > 0 {
            if !self.get_tile(tile.position.x, tile.position.y - 1).explored {
                return true;
            }
        }
        if tile.south_passable {
            if !self.get_tile(tile.position.x, tile.position.y + 1).explored {
                return true;
            }
        }
        if tile.east_passable {
            if !self.get_tile(tile.position.x + 1, tile.position.y).explored {
                return true;
            }
        }
        if tile.west_passable && tile.position.x > 0 {
            if !self.get_tile(tile.position.x - 1, tile.position.y).explored {
                return true;
            }
        }
        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DungeonState {
    Idle(bool),
    IdleChest,
    Fight(Enemy),
}

const WHITE:image::Rgb<u8> = image::Rgb([255, 255, 255]);
const CITY_1:image::Rgb<u8> = image::Rgb([1, 0, 31]);
const CITY_2:image::Rgb<u8> = image::Rgb([3, 2, 20]);
const FIGHT:image::Rgb<u8> = image::Rgb([208, 188, 255]);
const HEALTH_GREY:image::Rgb<u8> = image::Rgb([158, 158, 158]);
const HEALTH_RED:image::Rgb<u8> = image::Rgb([244, 67, 54]);
const HEALTH_RED_PLAYER:image::Rgb<u8> = image::Rgb([211, 47, 47]);
const HEALTH_GREEN:image::Rgb<u8> = image::Rgb([56, 142, 60]);
const HEALTH_ORANGE:image::Rgb<u8> = image::Rgb([245, 124, 0]);

const IDLE_1:image::Rgb<u8> = image::Rgb([202, 196, 208]);

const TILE_UNEXPLORED:image::Rgb<u8> = image::Rgb([29, 27, 32]);

pub fn get_characters(image:&Bitmap) -> [Character; 4] {
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

pub fn has_dead_characters(ocr:&OcrEngine, image:&DynamicImage) -> bool {
    let img = image.clone().sub_image(143, 520, 375, 394).to_image();
    let img_source = ImageSource::from_bytes(img.as_bytes(), (375, 394)).expect("from_bytes");
    //let img_source = ImageSource::from_bytes(image.as_bytes(), image.dimensions()).expect("from_bytes");
    let ocr_input = ocr.prepare_input(img_source).expect("prepare_input");
    
    let text = ocr.get_text(&ocr_input).expect("get_text");
    text.contains("dead")
}

fn get_enemy(image:&Bitmap) -> Enemy {
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

fn write_coord_to_file(x:u32, y: u32) {
    //let mut f = std::fs::OpenOptions::new().write(true).create(true).append(true).open("coords.txt").unwrap();
    //write!(f, "{x},{y}\n").unwrap();    
}

fn pixels_color(image: &Bitmap, pixels:impl Iterator<Item = Pixel>) -> bool {
    pixels.into_iter().all(|pixel|{
        write_coord_to_file(pixel.x, pixel.y);
        //let c = image.get_pixel(pixel.x, pixel.y);
        //println!("{}x{} {:?} {:?}", pixel.x, pixel.y, pixel.color, c);
        *image.get_pixel(pixel.x as u16, pixel.y as u16) == pixel.color.0
    })
}
fn pixels_same_color(image: &Bitmap, pixels:impl Iterator<Item = Coords>, color: Rgb<u8>) -> bool {
    pixels.into_iter().all(|coords|{
        write_coord_to_file(coords.x, coords.y);
        //let c = image.get_pixel(coords.x, coords.y);
        //println!("{}x{} {:?} {:?}", coords.x, coords.y, color, c);
        *image.get_pixel(coords.x as u16, coords.y as u16) == color.0
    })
}
fn pixel_color(image: &Bitmap, coords:Coords, color: Rgb<u8>) -> bool {
    write_coord_to_file(coords.x, coords.y);
    //println!("{}x{} {:?} {:?}", coords.x, coords.y, color, image.get_pixel(coords.x, coords.y));
    *image.get_pixel(coords.x as u16, coords.y as u16) == color.0
}
fn pixel_either_color(image: &Bitmap, coords:Coords, colors: impl Iterator<Item = Rgb<u8>>) -> bool {
    write_coord_to_file(coords.x, coords.y);
    let color = image.get_pixel(coords.x as u16, coords.y as u16);
    colors.into_iter().any(|v|v.0 == *color)
}

pub fn get_state(old_state:State, image:&Bitmap) -> Result<State, StateError> {
    if pixels_same_color(&image, [(918, 138).into(), (949, 138).into(), (919, 168).into(), (949, 168).into()].into_iter(), image::Rgb([202, 196, 208])) {
        return Ok(Into::<State>::into(StateType::Ad).merge(old_state));
    }
    if pixel_color(&image, (466, 1116).into(), image::Rgb([185, 207, 220])) && pixels_same_color(&image, [(690, 1306).into(), (717, 1326).into()].into_iter(), image::Rgb([56, 30, 114])) {
        return Ok(Into::<State>::into((StateType::Dungeon, Dungeon::new(DungeonState::IdleChest, &image, old_state.get_position()))).merge(old_state));
    }
    if (pixel_either_color(&image, (827, 1306).into(), [FIGHT, image::Rgb([192, 172, 241])].into_iter()) ||
        pixel_either_color(&image, (827, 1260).into(), [FIGHT, image::Rgb([192, 172, 241])].into_iter())) &&
        !pixel_color(&image, (671, 1309).into(), image::Rgb([56, 30, 114])) {
        return Ok(Into::<State>::into((StateType::Dungeon, Dungeon::new(DungeonState::Fight(get_enemy(&image)), &image, old_state.get_position()))).merge(old_state));
    }
    if pixel_color(&image, (979, 1083).into(), IDLE_1) && pixel_color(&image, (1023, 1116).into(), IDLE_1) {
        let on_city_tile = pixel_color(&image, (716, 1279).into(), FIGHT);
        return Ok(Into::<State>::into((StateType::Dungeon, Dungeon::new(DungeonState::Idle(on_city_tile), &image, old_state.get_position()))).merge(old_state));
    }
    if pixels_color(&image, [(752, 1926, CITY_1).into(), (75, 1512, CITY_2).into()].into_iter()) {
        return Ok(Into::<State>::into(StateType::City(image.has_dead_characters)).merge(old_state));
    }
    if pixels_same_color(&image, [(462, 1254).into(), (536, 1262).into(), (615, 1270).into()].into_iter(), WHITE) {
        return Ok(Into::<State>::into(StateType::Main).merge(old_state));
    }
    Err(StateError::UnknownState)
}

#[derive(Debug, Copy, Clone)]
pub enum MoveDirection {
    North,
    East,
    South,
    West,
}
#[derive(Debug, Copy, Clone)]
pub enum Action {
    CloseAd, 
    GotoTown,
    GotoDungeon,

    FindFight(MoveDirection, Tile),
    Fight,
    OpenChest,

    ReturnToTown(bool, MoveDirection),
    Resurrect,
}

pub fn determine_action(state:&State, last_action:Action, old_position:Option<Coords>) -> Action {
   // println!("{state:?}");
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
                                println!("This tile {:?}", dungeon.get_current_tile());
                                println!("City tile {:?}", city_tile);
                                println!("Next tile {:?}", next_tile);
                                Action::ReturnToTown(false, next_tile.direction_from(dungeon.get_current_tile()))
                            }
                            else {
                                println!("This tile {:?}", dungeon.get_current_tile());
                                println!("City tile {:?}", city_tile);
                                println!("Found no path to city tile");
                                let tile = dungeon.get_random_tile_from_current(None, RandomTarget::City);
                                Action::ReturnToTown(false, tile.direction_from(dungeon.get_current_tile()))
                            }
                        }
                        else {
                            println!("This tile {:?}", dungeon.get_current_tile());
                            println!("Don't know where city tile is");
                            let tile = dungeon.get_random_tile_from_current(None, RandomTarget::City);
                            Action::ReturnToTown(false, tile.direction_from(dungeon.get_current_tile()))
                        }
                    }
                    else {
                        let tile = if let Action::FindFight(_move_direction, target_tile) = last_action {
                            if target_tile.position == dungeon.get_current_tile().position {
                                println!("looking for unexplored tile");
                                dungeon.get_unexplored_tile(old_position)
                            }
                            else {
                                println!("using last target tile");
                                target_tile
                            }
                        }
                        else {
                            println!("looking for unexplored tile");
                            dungeon.get_unexplored_tile(old_position)
                        };
                        if let Some(next_tile) = dungeon.get_next_tile_to_goal(dungeon.get_current_tile(), tile) {
                            Action::FindFight(next_tile.direction_from(dungeon.get_current_tile()), tile)
                        }
                        else {
                            println!("Found no path to {:?}", tile);
                            let tile = dungeon.get_random_tile_from_current(None, RandomTarget::Unexplored);
                            Action::FindFight(tile.direction_from(dungeon.get_current_tile()), tile)
                        }
                    }
                },
                DungeonState::IdleChest => {
                    Action::OpenChest
                },
                DungeonState::Fight(_enemy) => {
                    if dungeon.has_low_character() || dungeon.has_dead_character() {
                        if let Some(city_tile) = dungeon.get_city_tile() {
                            if let Some(next_tile) = dungeon.get_next_tile_to_goal(dungeon.get_current_tile(), city_tile) {
                                println!("This tile {:?}", dungeon.get_current_tile());
                                println!("City tile {:?}", city_tile);
                                println!("Next tile {:?}", next_tile);
                                Action::ReturnToTown(false, next_tile.direction_from(dungeon.get_current_tile()))
                            }
                            else {
                                println!("This tile {:?}", dungeon.get_current_tile());
                                println!("City tile {:?}", city_tile);
                                println!("Found no path to city tile");
                                let tile = dungeon.get_random_tile_from_current(None, RandomTarget::City);
                                Action::ReturnToTown(false, tile.direction_from(dungeon.get_current_tile()))
                            }
                        }
                        else {
                            println!("This tile {:?}", dungeon.get_current_tile());
                            println!("Don't know where city tile is");
                            println!("{:?}", dungeon.tiles);
                            let tile = dungeon.get_random_tile_from_current(None, RandomTarget::City);
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

pub fn run_action(device:&str, opt:&Opt, _state:&State, action:&Action) -> Option<Coords> {
    match action {
        Action::CloseAd => {
            adb_tap(device, opt, 935, 153);
        },
        Action::GotoTown => {

        },
        Action::GotoDungeon => {
            adb_tap(device, opt, 890, 1928);
        },
        Action::FindFight(move_direction, _target_tile) => {
            adb_move(device, opt, move_direction);
        },
        Action::Fight => {
            adb_tap(device, opt, 711, 1308);
        },
        Action::OpenChest => {
            adb_tap(device, opt, 798, 1312);
        },
        Action::ReturnToTown(on_city_tile, move_direction) => {
            if *on_city_tile {
                adb_tap(device, opt, 715, 1316);
            }
            else {
                adb_move(device, opt, move_direction);
            }
        },
        Action::Resurrect => {

        },
    }
    None
}

fn adb_move(device:&str, opt:&Opt, move_direction:&MoveDirection) {
    match move_direction {
        MoveDirection::North => adb_tap(device, opt, 774, 2085),
        MoveDirection::East => adb_tap(device, opt, 953, 2277),
        MoveDirection::South => adb_tap(device, opt, 774, 2264),
        MoveDirection::West => adb_tap(device, opt, 575, 2277),
    }
}

/*fn adb_input(device:&str, opt:&Opt, key:&str) {
    let _ = if opt.local {
        Command::new("input").arg("keyevent").arg(key)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .spawn().unwrap().wait().unwrap();
    }
    else {
        Command::new("adb").arg("-s").arg(device).arg("shell").arg("input").arg("keyevent").arg(key)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .spawn().unwrap().wait().unwrap();
    };
}*/

fn adb_tap(device:&str, opt:&Opt, x:u32, y:u32) {
    let _ = if opt.local {
        Command::new("input").arg("tap").arg(x.to_string()).arg(y.to_string())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .spawn().unwrap().wait().unwrap();
    }
    else {
        Command::new("adb").arg("-s").arg(device).arg("shell").arg("input").arg("tap").arg(x.to_string()).arg(y.to_string())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .spawn().unwrap().wait().unwrap();
    };
}