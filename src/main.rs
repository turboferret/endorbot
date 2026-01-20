use std::{collections::{HashMap, HashSet}, convert::Infallible, io::Write, sync::Arc};

use astra::{Body, Request, ResponseBuilder};
use clap::Parser;
use image::GenericImageView;
use ocrs::OcrEngine;
use rkyv::rancor::Panic;

use crate::ml::{Action, Bitmap, State};

mod screencap;
mod ml;

#[derive(Parser, Copy, Clone)]
struct Opt {
    #[clap(long, action, default_value_t = false)]
    step: bool,
    #[clap(long, action, default_value_t = false)]
    no_action: bool,
    #[clap(long, action, default_value_t = false)]
    local: bool,
    #[clap(long, action, default_value_t = false)]
    screencap: bool,
}
//  1080x2408
fn main() {
    let opt = Opt::parse();
    let device = "RF8W101PHWF";

    if opt.screencap {
        let image = screencap::screencap(device, &opt).unwrap();
        let mut bitmap = Bitmap::with_capacity(2);
        for (x, y) in [(918u16,138u16),(466,1116),(827,1306),(671,1309),(90,1472),(511,1471),(514,56),(291,56),(514,68),(514,8),(514,92),(566,566),(564,566),(566,537),(592,566),(566,592),(537,566),(566,626),(564,626),(566,597),(592,626),(566,652),(537,626),(566,686),(566,746),(566,806),(564,806),(566,777),(592,806),(566,832),(537,806),(566,866),(566,926),(626,566),(624,566),(626,537),(652,566),(626,592),(597,566),(626,626),(624,626),(626,597),(652,626),(626,652),(597,626),(626,686),(626,746),(626,806),(624,806),(626,777),(652,806),(626,832),(597,806),(626,866),(626,926),(686,566),(684,566),(686,537),(712,566),(686,592),(657,566),(686,626),(684,626),(686,597),(712,626),(686,652),(657,626),(686,686),(686,746),(686,806),(684,806),(686,777),(712,806),(686,832),(657,806),(686,866),(686,926),(746,566),(744,566),(746,537),(772,566),(746,592),(717,566),(746,626),(746,686),(746,746),(746,806),(744,806),(746,777),(772,806),(746,832),(717,806),(746,866),(746,926),(806,566),(804,566),(806,537),(832,566),(806,592),(777,566),(806,626),(804,626),(806,597),(832,626),(806,652),(777,626),(806,686),(804,686),(806,657),(832,686),(806,712),(777,686),(806,746),(804,746),(806,717),(832,746),(806,772),(777,746),(806,806),(804,806),(806,777),(832,806),(806,832),(777,806),(806,866),(806,926),(866,566),(864,566),(866,537),(892,566),(866,592),(837,566),(866,626),(864,626),(866,597),(892,626),(866,652),(837,626),(866,686),(864,686),(866,657),(892,686),(866,712),(837,686),(866,746),(864,746),(866,717),(892,746),(866,772),(837,746),(866,806),(864,806),(866,777),(892,806),(866,832),(837,806),(866,866),(866,926),(926,566),(924,566),(926,537),(952,566),(926,592),(897,566),(926,626),(924,626),(926,597),(952,626),(926,652),(897,626),(926,686),(924,686),(926,657),(952,686),(926,712),(897,686),(926,746),(924,746),(926,717),(952,746),(926,772),(897,746),(926,806),(924,806),(926,777),(952,806),(926,832),(897,806),(926,866),(926,926),(355,1471),(181,1471),(291,92),(827,126),(979,1083),(1023,1116),(716,1279),(564,686),(566,657),(592,686),(566,712),(537,686),(564,866),(566,837),(592,866),(566,892),(537,866),(624,686),(626,657),(652,686),(626,712),(597,686),(624,866),(626,837),(652,866),(626,892),(597,866),(684,686),(686,657),(712,686),(686,712),(657,686),(684,866),(686,837),(712,866),(686,892),(657,866),(744,626),(746,597),(772,626),(746,652),(717,626),(744,866),(746,837),(772,866),(746,892),(717,866),(804,866),(806,837),(832,866),(806,892),(777,866),(864,866),(866,837),(892,866),(866,892),(837,866),(924,866),(926,837),(952,866),(926,892),(897,866),(564,746),(566,717),(592,746),(566,772),(537,746),(564,926),(566,897),(592,926),(566,952),(537,926),(624,746),(626,717),(652,746),(626,772),(597,746),(624,926),(626,897),(652,926),(626,952),(597,926),(684,746),(686,717),(712,746),(686,772),(657,746),(684,926),(686,897),(712,926),(686,952),(657,926),(744,686),(746,657),(772,686),(746,712),(717,686),(744,926),(746,897),(772,926),(746,952),(717,926),(804,926),(806,897),(832,926),(806,952),(777,926),(864,926),(866,897),(892,926),(866,952),(837,926),(924,926),(926,897),(952,926),(926,952),(897,926),(690,1306),(422,1471),(744,746),(746,717),(772,746),(746,772),(717,746),(291,68),(717,1326),(291,8),(949,138),(919,168),(949,168),(752,1926),(462,1254)] {
            bitmap.set_pixel(x, y, image.get_pixel(x as u32, y as u32).0[0..3].try_into().unwrap());
        }
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

    let main_state = old_state.clone();
    let ocr = ml::create_ocr_engine();
    let mut last_action = Action::CloseAd;
    loop {
        let snapshot = {
            let guard = main_state.lock();
            guard.clone()
        };
        let (state, action) = run(opt, device, &ocr, snapshot, last_action);
        last_action = action;
        match action {
            Action::CloseAd => {

            },
            Action::GotoTown => {

            },
            Action::GotoDungeon => {

            },
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
        if opt.step {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    
}

fn run(opt:Opt, device:&str, ocr:&OcrEngine, old_state:State, last_action:Action) -> (State, Action) {
    let img = screencap::screencap(device, &opt).unwrap();
    img.save_with_format("cap.png", image::ImageFormat::Png).unwrap();
    let old_position = old_state.get_position();
    let state = ml::get_state(ocr, old_state, img).unwrap();
    //println!("{:?}", state);
    let action = ml::determine_action(&state, last_action, old_position);
    println!("{:?}", action);
    if !opt.no_action {
        ml::run_action(device, opt, &state, &action);
    }
    (state, action)
}