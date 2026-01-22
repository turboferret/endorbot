use std::{collections::{HashMap, HashSet}, convert::Infallible, io::Write, path::PathBuf, sync::Arc};

use astra::{Body, Request, ResponseBuilder};
use clap::Parser;
use image::GenericImageView;
use ocrs::OcrEngine;
use rkyv::rancor::Panic;

use crate::ml::{Action, Bitmap, State};

mod screencap;
mod ml;

#[derive(Parser, Clone)]
struct Opt {
    #[clap(long, action, default_value_t = false)]
    step: bool,
    #[clap(long, action, default_value_t = false)]
    no_action: bool,
    #[clap(long, action, default_value_t = false)]
    local: bool,
    #[clap(long, action, default_value_t = false)]
    screencap: bool,
    #[clap(long, action, default_value_t = false)]
    debug: bool,
    #[clap(long, action, default_value_t = false)]
    no_ocr: bool,
    #[clap(long)]
    test: Option<PathBuf>,
}
//  1080x2408
fn main() {
    let opt = Opt::parse();

    if let Some(test) = &opt.test {
        let image = screencap::load_png_from_file(test.to_path_buf()).unwrap();
        let bitmap = screencap::bitmap_from_image(&image, &opt).unwrap();
        match ml::get_state(State::default(), &bitmap) {
            Ok(state) => {
                println!("{state:?}");
            },
            Err(err) => {
                println!("{:?}", err);
            },
        }
        return;
    }

    let device = "RF8W101PHWF";

    if opt.screencap {
        let bitmap = screencap::screencap_bitmap(device, &opt).unwrap();
        let b = rkyv::to_bytes::<Panic>(&bitmap).unwrap();
        //println!("{}", b.len());
        std::io::stdout().write_all(&b).unwrap();
        return;
    }

    let old_state = std::sync::Arc::new(parking_lot::Mutex::new(if let Ok(state) = std::fs::read_to_string("state") {
        serde_json::from_str(&state).unwrap_or(State::default())
    }
    else {
        State::default()
    }));

    let http_state = old_state.clone();

    std::thread::spawn(move|| {
        astra::Server::bind("0.0.0.0:8080").serve(move|req:Request,info| {
            if req.uri().path() == "/data" {
                let j = {
                    let guard = http_state.try_lock_for(std::time::Duration::from_millis(5000)).unwrap();
                    serde_json::to_string(&*guard).unwrap()
                };
                ResponseBuilder::new()
                .header("Content-Type", "application/json")
                .body(Body::new(j))
                .unwrap()
            }
            else {
                ResponseBuilder::new()
                .header("Content-Type", "text/html")
                .body(Body::new(r#"
                <!DOCTYPE html>
                <html>
                <head>
                <title>Endorbot</title>
                <style>
                #map {
                    display: flex;
                    flex-direction: column;
                }
                .row {
                    display: flex;
                }
                .tile {
                    position: relative;
                    width: 16px;
                    height: 16px;
                    border: 1px solid #f1f1f1;
                }
                .tile[explored] {
                    background-color: #bfbfbf;
                    border: 1px solid #000;
                }
                .tile[north-passable] {
                    border-top: 1px solid transparent;
                }
                .tile[south-passable] {
                    border-bottom: 1px solid transparent;
                }
                .tile[east-passable] {
                    border-right: 1px solid transparent;
                }
                .tile[west-passable] {
                    border-left: 1px solid transparent;
                }
                .tile[current]:after {
                    content: 'x';
                    position: absolute;
                    left: 0;
                    top: 0;
                    width: 100%;
                    height: 100%;
                    text-align: center;
                    font-size: 0.8em;
                }
                </style>
                <script>
                var map_size = {x: 0, y: 0};
                var map_rows = [];

                function update_map(map, state) {
                    var dungeon = state.dungeon;
                    var current_tile = document.querySelector('.tile[current]');
                    for(const tile of dungeon.tiles) {
                        if(tile.position.y >= map_size.y) {
                            for(var y = map_size.y; y <= tile.position.y; ++y) {
                                var row = document.createElement('div');
                                row.className = 'row';
                                var cols = [];
                                for(var x = 0; x < map_size.x; ++x) {
                                    var col = document.createElement('div');
                                    col.className = 'tile';
                                    row.appendChild(col);
                                    cols.push(col);
                                }
                                map.appendChild(row);
                                map_rows.push(cols);
                            }
                            map_size.y = tile.position.y + 1;
                        }
                        if(tile.position.x >= map_size.x) {
                            for(var y = 0; y < map_size.y; ++y) {
                                for(var x = map_size.x; x <= tile.position.x; ++x) {
                                    var col = document.createElement('div');
                                    col.className = 'tile';
                                    map.children[y].appendChild(col);
                                    map_rows[y].push(col);
                                }
                            }
                            map_size.x = tile.position.x + 1;
                        }
                        var e = map_rows[tile.position.y][tile.position.x];
                        if(tile.north_passable)
                            e.setAttribute('north-passable', '');
                        if(tile.south_passable)
                            e.setAttribute('south-passable', '');
                        if(tile.east_passable)
                            e.setAttribute('east-passable', '');
                        if(tile.west_passable)
                            e.setAttribute('west-passable', '');
                        e.setAttribute('explored', '');
                        if(tile.position.x == dungeon.info.coordinates.x && tile.position.y == dungeon.info.coordinates.y) {
                            if(current_tile)
                                current_tile.removeAttribute('current');
                            e.setAttribute('current', '');
                        }
                    }
                    setTimeout(refresh_data, 1000);
                }

                function refresh_data() {
                    var request = new XMLHttpRequest();
                    request.open("GET", "/data");
                    request.onreadystatechange = function () {
                        if (this.readyState == 4) {
                            if(this.status == 200) {
                                var map = document.getElementById('map');
                                update_map(map, JSON.parse(this.responseText));
                                //console.log(this.responseText);
                                //document.getElementById("container")
                                //.innerHTML = this.responseText;
                            }
                            else
                                console.info(this.status);
                        }
                    }
                    request.send();
                }

                refresh_data();
                </script>
                </head>
                <body>
                    <div id="map"></div>
                </body>
                </html>
                "#))
                .unwrap()
            }
        }).unwrap();
    });

    let step = opt.step;

    let main_state = old_state.clone();
    let mut last_action = Action::CloseAd;
    loop {
        let snapshot = {
            let guard = main_state.lock();
            guard.clone()
        };
        let (state, action) = run(&opt, device, snapshot, last_action);
        last_action = action;
        match action {
            Action::CloseAd => {
                std::thread::sleep(std::time::Duration::from_millis(200));
            },
            Action::GotoTown => {
                std::thread::sleep(std::time::Duration::from_millis(200));
            },
            Action::GotoDungeon => {
                std::thread::sleep(std::time::Duration::from_millis(200));
            },
            Action::GoDown => {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Action::FindFight(_move_direction, _target_tile) => {
            },
            Action::Fight => {
                std::thread::sleep(std::time::Duration::from_millis(200));
            //  break;
            },
            Action::OpenChest => {

            },
            Action::ReturnToTown(_on_city_tile, _move_direction) => {
            },
            Action::Resurrect => {
                println!("Need manual resurrection");
                break;
            },
        }
        let snapshot = {
            let mut guard = main_state.lock();
            *guard = state;
            guard.clone()
        };
        std::fs::write("state", serde_json::to_string(&snapshot).unwrap()).unwrap();
        if step {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
}

fn run(opt:&Opt, device:&str, old_state:State, last_action:Action) -> (State, Action) {
    //let img = screencap::screencap(device, &opt).unwrap();
    let img = screencap::screencap_bitmap(device, &opt).unwrap();
    //println!("{:?} {:?}", img.get_info(), img.get_has_dead_characters());
    //img.save_with_format("cap.png", image::ImageFormat::Png).unwrap();
    let old_position = old_state.get_position();
    let mut state = ml::get_state(old_state, &img).unwrap();
    //println!("{:?}", state);
    let action = ml::determine_action(&state, last_action, old_position);
    if let Some(pos) = state.get_position() {
        println!("position = {:?}", pos);
    }
    else {
        println!("position = none");
    }
    match action {
        Action::CloseAd => println!("CloseAd"),
        Action::GotoTown => println!("GotoTown"),
        Action::GotoDungeon => println!("GotoDungeon"),
        Action::GoDown => println!("GoDown"),
        Action::FindFight(move_direction, (tile, ticks_same_target)) => println!("FindFight {move_direction:?} target = {:?} ticks = {ticks_same_target}", tile.get_position()),
        Action::Fight => println!("Fight"),
        Action::OpenChest => println!("OpenChest"),
        Action::ReturnToTown(on_city_tile, move_direction) => println!("ReturnToTown {on_city_tile} {move_direction:?}"),
        Action::Resurrect => println!("Resurrect"),
    }
    //println!("{:?}", action);
    if !opt.no_action {
        if let Some(new_position) = ml::run_action(device, opt, &mut state, &action) {
            state.set_position(new_position);
        }
    }
    (state, action)
}