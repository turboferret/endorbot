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
        #[cfg(not(debug_assertions))]
        {
        self.pixels.iter().find_map(|(px, py, color)|if (x, y) == (*px, *py){Some(color)}else{None}).expect(&format!("{x}x{y} not found"))
        }
        #[cfg(debug_assertions)]
        self.pixels.iter().find_map(|(px, py, color)|if (x, y) == (*px, *py){Some(color)}else{None}).unwrap_or_else(||{println!("missing ({x},{y})"); &[0u8, 0, 0]})
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
    TeleportToCity,
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
        let down_tile = self.dungeon.tiles.iter().find(|tile|tile.is_go_down).cloned();
        for mut tile in old.dungeon.tiles {
            if let Some(new_tile) = self.dungeon.tiles.iter_mut().find(|v|v.position == tile.position) {
                if city_tile.is_none() {
                    new_tile.is_city = tile.is_city || new_tile.is_city;
                }
                if down_tile.is_none() {
                    new_tile.is_go_down = tile.is_go_down || new_tile.is_go_down;
                }
                new_tile.visited = tile.visited || new_tile.visited;
            }
            else {
                tile.is_city = if city_tile.is_none() {
                    tile.is_city 
                }
                else {
                    false
                };
                tile.is_go_down = if down_tile.is_none() {
                    tile.is_go_down
                }
                else {
                    false
                };
                self.dungeon.tiles.push(tile);
            }
        }
        self.clone()
    }
    
    pub fn set_position(&mut self, new_position: Coords) {
        self.dungeon.info.coordinates = Some(new_position);
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
    is_go_down: bool,
    visited: bool,
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
        (coords.x as i32 - (TILE_COUNT.0 + 1 ) as i32 / 2, coords.y as i32 - (TILE_COUNT.1 + 1 ) as i32 / 2 + 1)
    }
    else {
        (0, 0)
    };
    /*let (x_skip, y_skip, x_base, y_base) = if x_base < 0 || y_base < 0 {
        println!("{} {}", if x_base < 0 {x_base.abs()as u32}else{0}, if y_base < 0{y_base.abs() as u32}else{0});
        (if x_base < 0 {x_base.abs()as u32}else{0}, if y_base < 0{y_base.abs() as u32}else{0}, if x_base < 0{0}else{x_base}, if y_base < 0{0}else{y_base})
//        panic!("{x_base}/{y_base} {info:?}");
    }
    else {
        (0, 0, x_base, y_base)
    };*/
    //let (x_base, y_base) = (x_base as u32, y_base as u32);
    let mut tiles = Vec::new();
    for x_count in 0..TILE_COUNT.0 {
        for y_count in 0..TILE_COUNT.1 {
            if (x_base + x_count as i32) < 0 || (y_base + y_count as i32) < 0 {
                continue;
            }
//            println!("{x_base} {x_count} x {y_base} {y_count}");
            let x = TILE_START.0 + x_count * TILE_SIZE.0 + TILE_SIZE.0 / 2;
            let y = TILE_START.1 + y_count * TILE_SIZE.1 + TILE_SIZE.1 / 2;

            //panic!("{x}x{y} {x_base} + {x_count} {y_base} + {y_count}");

            if pixel_color(image, (x, y).into(), TILE_UNEXPLORED) {
                continue;
                //println!("{}x{}", x_base + x_count, y_base + y_count);
            }

          //  println!("{x}x{y} {}x{}", (x_base + x_count as i32) as u32, (y_base + y_count as i32) as u32);

            //println!("{x}x{} {}x{} {:?}", TILE_START.1 + y_count * TILE_SIZE.1 + TILE_SIZE.1 - 1, x_base + x_count, y_base + y_count, image.get_pixel(x, TILE_START.1 + y_count * TILE_SIZE.1 + TILE_SIZE.1 - 1));

           // println!("{x}x{y} {:?}", image.get_pixel(x, y));

            fn is_wall(image:&Bitmap, x:u32, y:u32) -> bool {
                let color = image.get_pixel(x as u16, y as u16);
                let color2 = image.get_pixel(x as u16, y as u16 + 1);
                color.iter().all(|v|*v >= 125) || color2.iter().all(|v|*v >= 125)
                || color.iter().all(|v|*v >= 40 && *v <= 64)
                || color2.iter().all(|v|*v >= 40 && *v <= 64)
            }

            fn is_city(image:&Bitmap, x:u32, y:u32) -> bool {
                let clr = [244u8, 67, 54];
                let clr_faded = [165u8, 118, 66];
                let color = image.get_pixel(x as u16, y as u16);
                let color2 = image.get_pixel(x as u16 + 4, y as u16 + 8);
                if (*color == clr || *color == clr_faded)  && *color2 != clr && *color2 != clr_faded  {
                    //println!("{x}x{y}");
                    true
                }
                else {
                    false
                }
            }
            fn is_go_down(image:&Bitmap, x:u32, y:u32) -> bool {
                let clr = [244u8, 67, 54];
                let clr_faded = [165u8, 118, 66];
                let color = image.get_pixel(x as u16, y as u16);
                let color2 = image.get_pixel(x as u16 + 4, y as u16 + 8);
                let color3 = image.get_pixel(x as u16 + 5, y as u16);
                let color4 = image.get_pixel(x as u16 - 5, y as u16);
                //println!("{x}x{y} {color:?} {color2:?} {color3:?}");
                if (*color == clr || *color == clr_faded)  && (*color2 == clr || *color2 == clr_faded) && *color3 != clr && *color3 == clr_faded && *color4 == clr && *color4 == clr_faded  {
                    //println!("{x}x{y}");
                    true
                }
                else {
                    false
                }
            }

            fn is_go_up(image:&Bitmap, x:u32, y:u32) -> bool {
                let clr = [244u8, 67, 54];
                let clr_faded = [165u8, 118, 66];
                let color = image.get_pixel(x as u16, y as u16);
                let color2 = image.get_pixel(x as u16 + 4, y as u16 + 8);
                let color3 = image.get_pixel(x as u16 + 5, y as u16);
                let color4 = image.get_pixel(x as u16 - 5, y as u16);
                //println!("{x}x{y} {color:?} {color2:?} {color3:?}");
                if (*color == clr || *color == clr_faded)  && *color2 != clr && *color2 != clr_faded && (*color3 == clr || *color3 == clr_faded) && (*color4 == clr || *color4 == clr_faded)  {
                    //println!("{x}x{y}");
                    true
                }
                else {
                    false
                }
            }

            let is_go_up = is_go_up(image, x-2, y);
            let position = Coords{x: (x_base + x_count as i32) as u32, y: (y_base + y_count as i32) as u32};
            let tile = Tile {
                explored: !pixel_color(image, (x, y).into(), TILE_UNEXPLORED),
                trap: false,
                visited: false,
                is_city: is_city(image, x-2, y),
                is_go_down: position != (15, 15).into() && !is_go_up && is_go_down(image, x-2, y),
                //is_city: pixel_color(image, (x-2, y).into(), Rgb([244, 67, 54])),
                position: position,
                north_passable: !is_wall(image, x, TILE_START.1 + y_count * TILE_SIZE.1 + 1),
                east_passable: !is_wall(image, TILE_START.0 + x_count * TILE_SIZE.0 + TILE_SIZE.0 - 4, y),
                south_passable: !is_wall(image, x, TILE_START.1 + y_count * TILE_SIZE.1 + TILE_SIZE.1 - 4),
                west_passable: !is_wall(image, TILE_START.0 + x_count * TILE_SIZE.0 + 1, y),
                //north_passable: !pixel_color(image, (x, TILE_START.1 + y_count * TILE_SIZE.1 + 1).into(), HEALTH_GREY) && !pixel_color(image, (x, TILE_START.1 + y_count * TILE_SIZE.1 + 1).into(), WHITE),
                //east_passable: !pixel_color(image, (TILE_START.0 + x_count * TILE_SIZE.0 + TILE_SIZE.0 - 4, y).into(), HEALTH_GREY) && !pixel_color(image, (TILE_START.0 + x_count * TILE_SIZE.0 + TILE_SIZE.0 - 4, y).into(), WHITE),
                //south_passable: !pixel_color(image, (x, TILE_START.1 + y_count * TILE_SIZE.1 + TILE_SIZE.1 - 4).into(), HEALTH_GREY) && !pixel_color(image, (x, TILE_START.1 + y_count * TILE_SIZE.1 + TILE_SIZE.1 - 4).into(), WHITE),
                //west_passable: !pixel_color(image, (TILE_START.0 + x_count * TILE_SIZE.0 + 1, y).into(), HEALTH_GREY) && !pixel_color(image, (TILE_START.0 + x_count * TILE_SIZE.0 + 1, y).into(), WHITE),
            };

            if tile.position.x == 18 && tile.position.y == 4 {
               // println!("{tile:?} {}x{} {:?}", TILE_START.0 + x_count * TILE_SIZE.0 + 1, y, image.get_pixel((TILE_START.0 + x_count * TILE_SIZE.0 + 1) as u16, y as u16));
            }

            if false && tile.position.x == 18 && tile.position.y == 4 {
                println!("{tile:?}");
                println!("west {}x{} {:?}", TILE_START.0 + x_count * TILE_SIZE.0 + 1, y, image.get_pixel((TILE_START.0 + x_count * TILE_SIZE.0 + 1) as u16, y as u16));
                println!("east {}x{} {:?}", x, TILE_START.1 + y_count * TILE_SIZE.1 + 1, image.get_pixel(x as u16, (TILE_START.1 + y_count * TILE_SIZE.1 + 1) as u16));
                println!("south {}x{} {:?}", TILE_START.0 as u16 + x_count as u16 * TILE_SIZE.0 as u16 + TILE_SIZE.0 as u16 - 4, y as u16, image.get_pixel(TILE_START.0 as u16 + x_count as u16 * TILE_SIZE.0 as u16 + TILE_SIZE.0 as u16 - 4, y as u16));
            }

            if pixel_color(image, (TILE_START.0 + x_count * TILE_SIZE.0 + 1, y).into(), TILE_UNEXPLORED) && !pixel_color(image, (x, y).into(), TILE_UNEXPLORED) {
                continue;
            }

            //println!("{x}x{y} = {}x{} n={} e={} s={} w={} ", tile.position.x, tile.position.y, tile.north_passable, tile.east_passable, tile.south_passable, tile.west_passable);
            
            if tile.position.x == 22 && tile.position.y == 14 {
                if tile.north_passable {
                    println!("{tile:?} {}x{}", x, TILE_START.1 + y_count * TILE_SIZE.1 + 1);
                    panic!();
                }
            }
            //println!("{x}x{y} {tile:?}");

            /*if 806 == x && 686 == y {
                println!("west {}x{y} {:?}", TILE_START.0 + x_count * TILE_SIZE.0 + 1, image.get_pixel(TILE_START.0 + x_count * TILE_SIZE.0 + 1, y));
                println!("east {}x{y} {:?}", TILE_START.0 + x_count * TILE_SIZE.0 + TILE_SIZE.0 - 1, image.get_pixel(TILE_START.0 + x_count * TILE_SIZE.0 + TILE_SIZE.0 - 1, y));

                println!("south {x}x{} {:?}", TILE_START.1 + y_count * TILE_SIZE.1 + TILE_SIZE.1 - 4, image.get_pixel(x, TILE_START.1 + y_count * TILE_SIZE.1 + TILE_SIZE.1 - 4));
            }*/

            tiles.push(tile);
        }
    }
   // std::process::exit(0);
    tiles
}

#[derive(Debug)]
enum RandomTarget {
    GoDown,
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
        let mut state = Self {
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
        if let Some(pos) = state.info.coordinates {
            state.set_tile_visited(pos.x, pos.y);
        }
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
            is_go_down: false,
            visited: false,
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

    fn get_go_down_tile(&self) -> Option<Tile> {
        for tile in &self.tiles {
            if tile.is_go_down {
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
            if !tile.is_city && !tile.is_go_down {
                tiles.push(tile);
            }
        }
        if current.east_passable {
            let tile = self.get_tile(current.position.x + 1, current.position.y);
            if !tile.is_city && !tile.is_go_down {
                tiles.push(tile);
            }
        }
        if current.south_passable {
            let tile = self.get_tile(current.position.x, current.position.y + 1);
            if !tile.is_city && !tile.is_go_down {
                tiles.push(tile);
            }
        }
        if current.west_passable {
            let tile = self.get_tile(current.position.x - 1, current.position.y);
            if !tile.is_city && !tile.is_go_down {
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
                RandomTarget::GoDown => {
                    if let Some(city_tile) = tiles.iter().find(|tile|tile.is_go_down) {
                        tiles = vec![*city_tile];
                    }
                },
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
            if tile.north_passable && pos.y > 0 {
                let n = Coords { x: pos.x, y: pos.y - 1 };
                    out.push((n, 1));
            }
            // Öst: x + 1
            if tile.east_passable && pos.x < 29 {
                let e = Coords { x: pos.x + 1, y: pos.y };
                    out.push((e, 1));
            }
            // Syd: y + 1
            if tile.south_passable && pos.y < 29 {
                let s = Coords { x: pos.x, y: pos.y + 1 };
                    out.push((s, 1));
            }
            // Väst: x - 1
            if tile.west_passable && pos.x > 0 {
                let w = Coords { x: pos.x - 1, y: pos.y };
                    out.push((w, 1));
            }
            out
        };
        if let Some((path, _cost)) = astar(&current_tile.position, successors, |p|manhattan(*p, goal.position), |p|*p == goal.position) {
            let l = path.get(path.len()-2).unwrap();
            //println!("{path:?} {:?}", self.get_tile(l.x, l.y));
            //println!("{:?}", self.get_current_tile());
            let pos = path.get(1).unwrap();
            Some(self.get_tile(pos.x, pos.y))
        }
        else {
            None
        }
    }

    fn get_closest_unvisited_tile(&self, current_tile:Tile) -> Option<Tile> {
        use pathfinding::prelude::astar;
        //let map: HashMap<Coords, &Tile> =
            //self.tiles.iter().map(|t| (t.position, t)).collect();

        let successors = |pos: &Coords| -> Vec<(Coords, u32)> {
            //let Some(tile) = map.get(pos) else { return vec![]; };
            let tile = self.get_tile(pos.x, pos.y);
            let mut out = Vec::with_capacity(4);
            if tile.north_passable && pos.y > 0 {
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
            if tile.west_passable && pos.x > 0 {
                let w = Coords { x: pos.x - 1, y: pos.y };
                //if map.contains_key(&w) {
                    out.push((w, 1));
                //}
            }

            out
        };

        let is_goal = |pos: &Coords| {
            !self.get_tile(pos.x, pos.y).visited
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
            println!("found no ununvisited tile");
        }
        None
    }
    
    fn get_unexplored_tile(&self, old_position: Option<Coords>) -> Tile {
        let me = self.get_current_tile();
        if let Some(tile) = self.get_closest_unvisited_tile(me) {
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
    
    fn clear_visited(&mut self) {
        for tile in self.tiles.iter_mut() {
            tile.visited = false;
        }
    }
    
    fn set_tile_visited(&mut self, x: u32, y: u32) {
        for tile in self.tiles.iter_mut() {
            if tile.position.x == x && tile.position.y == y {
                tile.visited = true;
            }
        }
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
    if pixels_same_color(&image, [(911, 940).into(), (155, 940).into(), (919, 168).into(), (949, 168).into()].into_iter(), image::Rgb([43, 41, 48])) {
        return Ok(Into::<State>::into(StateType::TeleportToCity).merge(old_state));
    }
    if pixels_same_color(&image, [(918, 138).into(), (949, 138).into(), (919, 168).into(), (949, 168).into()].into_iter(), image::Rgb([202, 196, 208])) {
        return Ok(Into::<State>::into(StateType::Ad).merge(old_state));
    }
    if pixel_color(&image, (466, 1116).into(), image::Rgb([185, 207, 220])) && pixels_same_color(&image, [(690, 1306).into(), (717, 1326).into()].into_iter(), image::Rgb([56, 30, 114])) {
        return Ok(Into::<State>::into((StateType::Dungeon, Dungeon::new(DungeonState::IdleChest, &image, old_state.get_position()))).merge(old_state));
    }
    if (image.get_info().coordinates.is_none() &&
        (pixel_either_color(&image, (827, 1306).into(), [FIGHT, image::Rgb([192, 172, 241])].into_iter()) ||
        pixel_either_color(&image, (827, 1260).into(), [FIGHT, image::Rgb([192, 172, 241])].into_iter())) &&
        !pixel_color(&image, (671, 1309).into(), image::Rgb([56, 30, 114]))) {
        return Ok(Into::<State>::into((StateType::Dungeon, Dungeon::new(DungeonState::Fight(get_enemy(&image)), &image, old_state.get_position()))).merge(old_state));
    }
    if pixel_color(&image, (979, 1083).into(), IDLE_1) && pixel_color(&image, (1023, 1116).into(), IDLE_1) {
        let on_city_tile = pixel_color(&image, (716, 1279).into(), FIGHT)
            && !pixels_same_color(image, [(642, 1201).into(), (608, 1307).into(), (609, 1329).into()].into_iter(), image::Rgb([56, 30, 114]));
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
    GoDown,

    CancelTeleportToCity,
    TeleportToCity,

    FindFight(MoveDirection, (Tile, u32)),
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
        StateType::TeleportToCity => {
            if state.dungeon.has_dead_character() {
                Action::TeleportToCity
            }
            else {
                Action::CancelTeleportToCity
            }
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
                    if dungeon.has_dead_character() {
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
                        println!("{:?}", dungeon.get_current_tile());
                        if let Some(go_down_tile) = dungeon.get_go_down_tile() {
                            if go_down_tile.position == dungeon.get_current_tile().position {
                                return Action::GoDown;
                            }
                        }
                        let (tile, ticks_same_target) = if let Action::FindFight(_move_direction, (target_tile, ticks_same_target)) = last_action {
                            if target_tile.position == dungeon.get_current_tile().position {
                                println!("looking for unexplored tile");
                                (dungeon.get_unexplored_tile(old_position), 1)
                            }
                            else {
                                println!("using last target tile");
                                (target_tile, ticks_same_target + 1)
                            }
                        }
                        else {
                            println!("looking for unexplored tile");
                            (dungeon.get_unexplored_tile(old_position), 1)
                        };

                        let (tile, ticks_same_target) = if ticks_same_target > 30 {
                            println!("Too many ticks spent on moving to target");
                            (dungeon.get_unexplored_tile(old_position), 1)
                        }
                        else {
                            (tile, ticks_same_target)
                        };

                        let (tile, ticks_same_target) = if let Some(go_down_tile) = dungeon.get_go_down_tile() {
                            if go_down_tile.position != tile.position {
                                (go_down_tile, 1)
                            }
                            else {
                                (tile, ticks_same_target)
                            }
                        }
                        else {
                            (tile, ticks_same_target)
                        };

                        if let Some(next_tile) = dungeon.get_next_tile_to_goal(dungeon.get_current_tile(), tile) {
                            Action::FindFight(next_tile.direction_from(dungeon.get_current_tile()), (tile, ticks_same_target))
                        }
                        else {
                            println!("Found no path to {:?}", tile);
                            let tile = dungeon.get_random_tile_from_current(None, RandomTarget::Unexplored);
                            Action::FindFight(tile.direction_from(dungeon.get_current_tile()), (tile, 0))
                        }
                    }
                },
                DungeonState::IdleChest => {
                    Action::OpenChest
                },
                DungeonState::Fight(_enemy) => {
                    if false && dungeon.has_low_character() || dungeon.has_dead_character() {
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

pub fn run_action(device:&str, opt:&Opt, state:&mut State, action:&Action) -> Option<Coords> {
    match action {
        Action::CloseAd => {
            adb_tap(device, opt, 935, 153);
        },
        Action::GotoTown => {

        },
        Action::GotoDungeon => {
            state.dungeon.clear_visited();
            adb_tap(device, opt, 890, 1928);
        },
        Action::CancelTeleportToCity => {
            adb_tap(device, opt, 331, 1440);
        },
        Action::TeleportToCity => {
            adb_tap(device, opt, 680, 1440);
        },
        Action::GoDown => {
            state.dungeon.tiles = Vec::new();
            adb_tap(device, opt, 715, 1316);
        },
        Action::FindFight(move_direction, _target_tile) => {
            adb_move(device, opt, move_direction);
            return Some(state.get_position().unwrap().move_direction(*move_direction));
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
                return Some(state.get_position().unwrap().move_direction(*move_direction));
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