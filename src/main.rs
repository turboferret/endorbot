use std::collections::{HashMap, HashSet};

use clap::{ArgAction, Parser};
use ocrs::OcrEngine;

use crate::ml::{Action, Coords, State};

mod screencap;
mod ml;

#[derive(Parser)]
struct Opt {
    #[clap(long, action, default_value_t = false)]
    step: bool,
    #[clap(long, action, default_value_t = false)]
    no_action: bool,
    #[clap(long, action, default_value_t = false)]
    local: bool,
}
//  1080x2408

fn main() {
    let opt = Opt::parse();
    /*for file in ["caps/main.png", "caps/city.png", "caps/dungeon.png", "caps/fight.png", "caps/enemy-low.png", "caps/char1-low.png", "caps/char4-low.png", "caps/enemy-hurt.png"] {
        println!("{file}");
        let img = screencap::load_png_from_file(file.into()).unwrap();
        println!("\t{:?}", ml::get_state(img));
    }*/

    //let img = screencap::screencap("RF8W101PHWF").unwrap();
    //img.save_with_format("out.bmp", image::ImageFormat::Bmp).unwrap();
    //println!("\t{:?}", ml::get_state(img));

    let ocr = ml::create_ocr_engine();

    let mut explored_tiles = HashMap::new();

    let mut old_state = if let Ok(state) = std::fs::read_to_string("state") {
        serde_json::from_str(&state).unwrap_or(State::default())
    }
    else {
        State::default()
    };

    loop {
        let (state, action) = run(&opt, "RF8W101PHWF", &ocr, old_state, &mut explored_tiles);
        match action {
            Action::CloseAd => {

            },
            Action::GotoTown => {

            },
            Action::GotoDungeon => {

            },
            Action::FindFight(move_direction) => {
            },
            Action::Fight => {
              //  break;
            },
            Action::OpenChest => {

            },
            Action::ReturnToTown(on_city_tile, move_direction) => {
            },
            Action::Resurrect => {
                println!("Need manual resurrection");
                break;
            },
        }
        old_state = state;
        std::fs::write("state", serde_json::to_string(&old_state).unwrap()).unwrap();
        if opt.step {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    //run("RF8W101PHWF", &ocr, &mut explored_tiles);

    //println!("{:?}", explored_tiles);
}

fn run(opt:&Opt, device:&str, ocr:&OcrEngine, mut old_state:State, mut explored_tiles:&mut HashMap<String, HashSet<(u32, u32)>>) -> (State, Action) {
    let img = screencap::screencap(device, &opt).unwrap();
    let old_position = old_state.get_position();
    let state = ml::get_state(ocr, old_state, img, explored_tiles).unwrap();
    //println!("{:?}", state);
    let action = ml::determine_action(&state, old_position, explored_tiles);
    println!("{:?}", action);
    if !opt.no_action {
        ml::run_action(device, opt, &state, &action);
    }
    (state, action)
}