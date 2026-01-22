use std::{fs::File, io::{BufReader, Read, Write}, path::PathBuf, process::{Command, Stdio}};

use image::{DynamicImage, GenericImageView, ImageError, RgbaImage};
use ocrs::OcrEngine;

use crate::{Opt, ml::{self, Bitmap, Coords, DungeonInfo}};

#[derive(Debug)]
pub enum LoadBitmapError {
    ImageError(ImageError),
    IoError(std::io::Error),
}

impl From<std::io::Error> for LoadBitmapError {
    fn from(value: std::io::Error) -> Self {
        Self::IoError(value)
    }
}
impl From<ImageError> for LoadBitmapError {
    fn from(value: ImageError) -> Self {
        Self::ImageError(value)
    }
}

pub fn load_bitmap(input: &[u8]) -> Result<DynamicImage, LoadBitmapError> {
    match image::load_from_memory_with_format(input, image::ImageFormat::Bmp) {
        Ok(image) => {
            Ok(image)
        },
        Err(err) => {
            match err {
                image::ImageError::Decoding(_) => {
                    let width = u32::from_le_bytes(input[..4].try_into().unwrap());
                    let height = u32::from_le_bytes(input[4..8].try_into().unwrap());
                    let image_buffer = RgbaImage::from_raw(width, height, input[16..].to_vec()).expect("Invalid bitmap data");
                    let image:DynamicImage = image_buffer.into();
                    Ok(image)
                },
                _ => {
                    Err(LoadBitmapError::ImageError(err))
                }
            }
        },
    }
}

pub fn load_bitmap_from_file(path: PathBuf) -> Result<DynamicImage, LoadBitmapError> {
    let mut buf = Vec::new();
    File::open(path)?.read_to_end(&mut buf)?;
    load_bitmap(&buf)
}

pub fn load_png_from_file(path: PathBuf) -> Result<DynamicImage, LoadBitmapError> {
    image::load(BufReader::new(File::open(path)?), image::ImageFormat::Png).map_err(|err|err.into())
}

#[derive(Debug)]
pub enum ScreencapError {
    LoadBitmapError(LoadBitmapError),
    IoError(std::io::Error),
    Failed,
}
impl From<std::io::Error> for ScreencapError {
    fn from(value: std::io::Error) -> Self {
        Self::IoError(value)
    }
}
impl From<LoadBitmapError> for ScreencapError {
    fn from(value: LoadBitmapError) -> Self {
        Self::LoadBitmapError(value)
    }
}

enum TextChar {
    Digit(u32),
    Comma,
    Unknown,
}

fn get_pixel(image:&DynamicImage, bx:u32, by:u32, x:u32, y:u32, opt:&Opt) -> image::Rgba<u8> {
    let clr = image.get_pixel(x, y);
    if opt.debug {
        println!("\t\t{}x{} = {clr:?}", x as i32 - bx as i32, y as i32 - by as i32);
    }
    clr
}

fn find_text_char(x:u32, y:u32, image:&DynamicImage, opt:&Opt) -> TextChar {
    let clr = image::Rgba([230, 224, 233, 255]);
    let gray = image::Rgba([29, 27, 32, 255]);
    /*if x == 292 {
        println!("{}x{} {}x{} {}x{} {}x{} {}x{} {}x{}", x,y+1, x-5, y+3, x-2, y+6, x+2,y+6,x+3,y+19,x-6,y+21);
        println!("{:?} {:?} {:?} {:?} {:?} {:?}", image.get_pixel(x, y + 1), image.get_pixel(x - 5, y + 3), image.get_pixel(x - 2, y + 6), image.get_pixel(x + 2, y + 6), image.get_pixel(x + 3, y + 19), image.get_pixel(x - 6, y + 21));
    }*/
    if opt.debug {
        println!("\tCheck UNKNOWN");
    }
    if get_pixel(image, x, y, x, y - 2, opt) == clr && get_pixel(image, x, y, x, y + 26, opt) == clr {  //  )
        if opt.debug {
            println!("\tFound UNKNOWN");
        }
        return TextChar::Unknown;
    }
    if opt.debug {
        println!("\tCheck COMMA");
    }
    if get_pixel(image, x, y, x, y + 25, opt) == clr || get_pixel(image, x, y, x, y + 26, opt) == clr {   //  ,
        return TextChar::Comma;
    }
    if opt.debug {
        println!("\tCheck 2");
    }
    if get_pixel(image, x, y, x, y + 1, opt) == clr
        && get_pixel(image, x, y, x - 5, y + 3, opt) == clr
        && get_pixel(image, x, y, x - 2, y + 6, opt) == gray
        && get_pixel(image, x, y, x + 4, y + 6, opt) == clr
        && get_pixel(image, x, y, x + 3, y + 19, opt) == clr
        && get_pixel(image, x, y, x - 6, y + 3, opt) == clr
            && get_pixel(image, x, y, x - 6, y + 21, opt) == clr {
        return TextChar::Digit(2);
    }
    if opt.debug {
        println!("\tCheck 1");
    }
    if get_pixel(image, x, y, x, y + 1, opt) == clr
        && get_pixel(image, x, y, x - 5, y + 3, opt) == clr
        && get_pixel(image, x, y, x - 5, y + 10, opt) != clr
            && get_pixel(image, x, y, x - 6, y + 21, opt) == clr {
        return TextChar::Digit(1);
    }
    if opt.debug {
        println!("\tCheck 0");
    }
    if get_pixel(image, x, y, x, y + 1, opt) == clr
        && get_pixel(image, x, y, x - 1, y + 10, opt) == clr
        && get_pixel(image, x, y, x - 6, y + 10, opt) == clr
        && get_pixel(image, x, y, x + 5, y + 5, opt) == clr
        && get_pixel(image, x,y, x - 5, y + 4, opt) == clr
        && get_pixel(image, x, y, x - 6, y, opt) == gray
        && get_pixel(image, x, y, x - 6, y + 14, opt) == clr
            && get_pixel(image, x, y, x - 6, y + 9, opt) == clr {
        return TextChar::Digit(0);
    }
    if opt.debug {
        println!("\tCheck 9");
    }
    if get_pixel(image, x, y, x, y + 1, opt) == clr
        && get_pixel(image, x, y, x - 7, y, opt) == gray
        && get_pixel(image, x, y, x, y + 14, opt) == gray
        && get_pixel(image, x, y, x - 7, y + 14, opt) == gray
            && get_pixel(image, x, y, x - 6, y + 9, opt) == clr {
        return TextChar::Digit(9);
    }
    if opt.debug {
        println!("\tCheck 6");
    }
    if get_pixel(image, x, y, x, y + 1, opt) == clr
        && get_pixel(image, x, y, x + 4, y + 6, opt) != clr
        && (get_pixel(image, x, y, x - 5, y + 14, opt) == clr || get_pixel(image, x, y, x - 6, y + 14, opt) == clr)
        && get_pixel(image, x, y, x - 7, y, opt) == gray
        && get_pixel(image, x, y, x, y + 14, opt) == gray
            && (get_pixel(image, x, y, x - 6, y + 9, opt) == clr || get_pixel(image, x, y, x - 4, y + 9, opt) == clr) {
        return TextChar::Digit(6);
    }
    if opt.debug {
        println!("\tCheck 8");
    }
    if get_pixel(image, x, y, x, y + 1, opt) == clr
        && (get_pixel(image, x, y, x - 3, y + 5, opt) == clr || get_pixel(image, x, y, x - 5, y + 5, opt) == clr)
            && (get_pixel(image, x, y, x + 6, y + 5, opt) == clr || get_pixel(image, x, y, x + 4, y + 5, opt) == clr)
            && (get_pixel(image, x, y, x + 7, y + 16, opt) == clr || get_pixel(image, x, y, x + 5, y + 16, opt) == clr)
            && get_pixel(image, x, y, x - 4, y + 19, opt) == clr {
        return TextChar::Digit(8);
    }
    if opt.debug {
        println!("\tCheck 5");
    }
    if get_pixel(image, x, y, x, y + 1, opt) == clr
        && get_pixel(image, x, y, x, y + 5, opt) != clr
        && (get_pixel(image, x, y, x - 5, y + 6, opt) == clr || get_pixel(image, x, y, x - 3, y + 6, opt) == clr)
        && get_pixel(image, x, y, x + 1, y + 6, opt) == gray
        && get_pixel(image, x, y, x + 1, y + 14, opt) != clr
            && get_pixel(image, x, y, x - 4, y + 2, opt) == clr
            && get_pixel(image, x, y, x + 4, y + 2, opt) == clr {
        return TextChar::Digit(5);
    }
    if opt.debug {
        println!("\tCheck 4");
    }
    if get_pixel(image, x, y, x + 2, y + 1, opt) == clr
        && (get_pixel(image, x, y, x - 2, y + 2, opt) != clr || get_pixel(image, x, y, x - 4, y + 2, opt) != clr)
        && get_pixel(image, x, y, x - 1, y + 11, opt) != clr {
        return TextChar::Digit(4);
    }
    if opt.debug {
        println!("\tCheck 7");
    }
    if get_pixel(image, x, y, x, y + 1, opt) == clr
        && get_pixel(image, x, y, x - 2, y + 6, opt) != clr
        && get_pixel(image, x, y, x + 6, y + 16, opt) != clr
            && get_pixel(image, x, y, x - 5, y + 2, opt) == clr
            && get_pixel(image, x, y, x + 5, y + 2, opt) == clr {
        return TextChar::Digit(7);
    }
    if opt.debug {
        println!("\tCheck 3");
    }
    if get_pixel(image, x, y, x, y + 1, opt) == clr
        && get_pixel(image, x, y, x - 5, y + 2, opt) == clr
            && get_pixel(image, x, y, x - 1, y + 10, opt) == clr
            && get_pixel(image, x, y, x - 4, y + 18, opt) == clr {
        return TextChar::Digit(3);
    }
    //println!("{x}x{y}");
    TextChar::Unknown
}

fn get_info(image:&DynamicImage, opt:&Opt) -> DungeonInfo {
    let clr = image::Rgba([230, 224, 233, 255]);
    for x in 220..378 {
        if image.get_pixel(x, 1051) == clr {
            if opt.debug {
                println!("Position start at {x}x1051");
            }

            let mut x = x + 20;
            let y = 1052;

            let mut numbers = Vec::new();
            let mut current_number = None;
            loop {
                match find_text_char(x, y, image, opt) {
                    TextChar::Digit(v) => {
                        if opt.debug {
                            println!("{x}x{y} = {v}");
                        }
                        current_number = if let Some(n) = current_number {
                            Some(n * 10 + v)
                        }
                        else {
                            Some(v)
                        };
                    },
                    TextChar::Comma => {
                        if opt.debug {
                            println!("{x}x{y} = ,");
                        }
                        x += 1;
                        if let Some(n) = current_number {
                            numbers.push(n);
                            current_number = None;
                        }
                    },
                    TextChar::Unknown => {
                        if opt.debug {
                            println!("{x}x{y} = UNKNOWN");
                        }
                        if let Some(n) = current_number {
                            numbers.push(n);
                            current_number = None;
                        }
                        break;
                    }
                }
                x += 20;
            }
            if opt.debug {
                println!("numbers = {numbers:?}");
            }

            return DungeonInfo {
                floor: "D1".to_owned(),
                coordinates: if numbers.len() >= 2 {
                    Some(Coords{x: numbers[0], y: numbers[1]})
                } else {None},
            };
        }
    }
    DungeonInfo {
        floor: "".to_owned(),
        coordinates: None,
    }
}

pub fn bitmap_from_image(image:&DynamicImage, opt:&Opt) -> Option<Bitmap> {
    let mut bitmap = Bitmap::with_capacity(100);
    for (x, y) in [(918u16,138u16),(809,806),(799,806),(809,806),(799,806),(809,866),(799,866),(809,866),(799,866),(869,746),(859,746),(869,746),(859,746),(869,806),(859,806),(869,806),(859,806),(869,866),(859,866),(869,866),(859,866),(929,746),(919,746),(929,746),(919,746),(929,806),(919,806),(929,806),(919,806),(929,866),(919,866),(929,866),(919,866),(911,940),(155,940),(749,686),(739,686),(749,686),(739,686),(749,746),(739,746),(749,746),(739,746),(749,806),(739,806),(749,806),(739,806),(809,686),(799,686),(809,686),(799,686),(809,746),(799,746),(809,746),(799,746),(560,930),(620,930),(680,930),(740,930),(800,930),(860,930),(920,930),(560,570),(560,630),(560,690),(560,750),(560,810),(560,870),(620,570),(620,630),(620,690),(620,750),(620,810),(620,870),(680,570),(680,630),(680,690),(680,750),(680,810),(680,870),(740,570),(740,630),(740,690),(740,750),(740,810),(740,870),(800,570),(800,630),(800,690),(800,750),(800,810),(800,870),(860,570),(860,630),(860,690),(860,750),(860,810),(860,870),(920,570),(920,630),(920,690),(920,750),(920,810),(920,870),(928,574),(928,634),(928,694),(928,754),(928,814),(928,874),(928,934),(568,574),(568,634),(568,694),(568,754),(568,814),(568,874),(568,934),(628,574),(628,634),(628,694),(628,754),(628,814),(628,874),(628,934),(688,574),(688,634),(688,694),(688,754),(688,814),(688,874),(688,934),(748,574),(748,634),(748,694),(748,754),(748,814),(748,874),(748,934),(808,574),(808,634),(808,694),(808,754),(808,814),(808,874),(808,934),(868,574),(868,634),(868,694),(868,754),(868,814),(868,874),(868,934),(642, 1201),(608, 1307),(609, 1329),(952,927),(926,953),(897,927),(592,927),(566,953),(537,927),(652,927),(626,953),(597,927),(712,927),(686,953),(657,927),(772,927),(746,953),(717,927),(832,927),(806,953),(777,927),(892,927),(866,953),(837,927),(592,867),(566,893),(537,867),(652,867),(626,893),(597,867),(712,867),(686,893),(657,867),(772,867),(746,893),(717,867),(832,867),(806,893),(777,867),(892,867),(866,893),(837,867),(952,867),(926,893),(897,867),(892,627),(866,653),(837,627),(892,687),(866,713),(837,687),(892,747),(866,773),(837,747),(892,807),(866,833),(837,807),(926,538),(952,567),(926,593),(897,567),(952,627),(926,653),(897,627),(952,687),(926,713),(897,687),(952,747),(926,773),(897,747),(952,807),(926,833),(897,807),(592,567),(566,593),(537,567),(592,627),(566,653),(537,627),(592,687),(566,713),(537,687),(592,747),(566,773),(537,747),(592,807),(566,833),(537,807),(652,567),(626,593),(597,567),(652,627),(626,653),(597,627),(652,687),(626,713),(597,687),(652,747),(626,773),(597,747),(652,807),(626,833),(597,807),(712,567),(686,593),(657,567),(712,627),(686,653),(657,627),(712,687),(686,713),(657,687),(712,747),(686,773),(657,747),(712,807),(686,833),(657,807),(772,567),(746,593),(717,567),(772,627),(746,653),(717,627),(772,687),(746,713),(717,687),(772,747),(746,773),(717,747),(772,807),(746,833),(717,807),(832,567),(806,593),(777,567),(832,627),(806,653),(777,627),(832,687),(806,713),(777,687),(832,747),(806,773),(777,747),(832,807),(806,833),(777,807),(866,538),(892,567),(866,593),(837,567),(566,898),(626,898),(686,898),(746,898),(806,898),(866,898),(926,898),(866,538),(566,838),(626,838),(686,838),(746,598),(746,658),(746,718),(746,778),(746,838),(806,538),(806,598),(806,658),(806,718),(806,778),(806,838),(866,598),(866,658),(866,718),(866,778),(866,838),(926,598),(926,658),(926,718),(926,778),(926,838),(566,538),(566,598),(566,658),(566,718),(566,778),(626,538),(626,598),(626,658),(626,718),(626,778),(686,538),(686,598),(686,658),(686,718),(686,778),(746,538),(147,680), (147,800), (75,1512), (147,920),(466,1116),(827,1306),(147,560),(671,1309),(90,1472),(511,1471),(511-89,1471),(514,560),(291,560),(514,680),(514,800),(514,920),(566,566),(564,566),(566,537),(566,538),(592,566),(566,592),(537,566),(566,626),(564,626),(566,597),(592,626),(566,652),(537,626),(566,686),(566,746),(566,806),(564,806),(566,777),(592,806),(566,832),(537,806),(566,866),(566,926),(626,566),(624,566),(626,537),(652,566),(626,592),(597,566),(626,626),(624,626),(626,597),(652,626),(626,652),(597,626),(626,686),(626,746),(626,806),(624,806),(626,777),(652,806),(626,832),(597,806),(626,866),(626,926),(686,566),(684,566),(686,537),(712,566),(686,592),(657,566),(686,626),(684,626),(686,597),(712,626),(686,652),(657,626),(686,686),(686,746),(686,806),(684,806),(686,777),(712,806),(686,832),(657,806),(686,866),(686,926),(746,566),(744,566),(746,537),(772,566),(746,592),(717,566),(746,626),(746,686),(746,746),(746,806),(744,806),(746,777),(772,806),(746,832),(717,806),(746,866),(746,926),(806,566),(804,566),(806,537),(832,566),(806,592),(777,566),(806,626),(804,626),(806,597),(832,626),(806,652),(777,626),(806,686),(804,686),(806,657),(832,686),(806,712),(777,686),(806,746),(804,746),(806,717),(832,746),(806,772),(777,746),(806,806),(804,806),(806,777),(832,806),(806,832),(777,806),(806,866),(806,926),(866,566),(864,566),(866,537),(892,566),(866,592),(837,566),(866,626),(864,626),(866,597),(892,626),(866,652),(837,626),(866,686),(864,686),(866,657),(892,686),(866,712),(837,686),(866,746),(864,746),(866,717),(892,746),(866,772),(837,746),(866,806),(864,806),(866,777),(892,806),(866,832),(837,806),(866,866),(866,926),(926,566),(924,566),(926,537),(952,566),(926,592),(897,566),(926,626),(924,626),(926,597),(952,626),(926,652),(897,626),(926,686),(924,686),(926,657),(952,686),(926,712),(897,686),(926,746),(924,746),(926,717),(952,746),(926,772),(897,746),(926,806),(924,806),(926,777),(952,806),(926,832),(897,806),(926,866),(926,926),(355,1471),(355-89,1471),(181,1471),(181-89,1471),(291,920),(827,1260),(979,1083),(1023,1116),(716,1279),(564,686),(566,657),(592,686),(566,712),(537,686),(564,866),(566,837),(592,866),(566,892),(537,866),(624,686),(626,657),(652,686),(626,712),(597,686),(624,866),(626,837),(652,866),(626,892),(597,866),(684,686),(686,657),(712,686),(686,712),(657,686),(684,866),(686,837),(712,866),(686,892),(657,866),(744,626),(746,597),(772,626),(746,652),(717,626),(744,866),(746,837),(772,866),(746,892),(717,866),(804,866),(806,837),(832,866),(806,892),(777,866),(864,866),(866,837),(892,866),(866,892),(837,866),(924,866),(926,837),(952,866),(926,892),(897,866),(564,746),(566,717),(592,746),(566,772),(537,746),(564,926),(566,897),(592,926),(566,952),(537,926),(624,746),(626,717),(652,746),(626,772),(597,746),(624,926),(626,897),(652,926),(626,952),(597,926),(684,746),(686,717),(712,746),(686,772),(657,746),(684,926),(686,897),(712,926),(686,952),(657,926),(744,686),(746,657),(772,686),(746,712),(717,686),(744,926),(746,897),(772,926),(746,952),(717,926),(804,926),(806,897),(832,926),(806,952),(777,926),(864,926),(866,897),(892,926),(866,952),(837,926),(924,926),(926,897),(952,926),(926,952),(897,926),(690,1306),(422,1471),(744,746),(746,717),(772,746),(746,772),(717,746),(291,680),(717,1326),(291,800),(949,138),(919,168),(949,168),(752,1926),(462,1254)] {
        bitmap.set_pixel(x, y, image.get_pixel(x as u32, y as u32).0[0..3].try_into().unwrap());
    }
    if !opt.no_ocr {
        //let ocr = ml::create_ocr_engine();
        // bitmap.set_has_dead_characters(ml::has_dead_characters(&ocr, &image));
        bitmap.set_info(get_info(&image, opt));
        bitmap.set_has_dead_characters(ml::get_characters(&bitmap).iter().find(|char|char.is_dead()).is_some());
    }
    if opt.debug {
        println!("{:?}", bitmap.get_has_dead_characters());
        println!("{:?}", bitmap.get_info());
    }
    return Some(bitmap);
}

pub fn screencap_bitmap(device:&str, opt:&Opt) -> Option<Bitmap> {
    if opt.local {
        let image = screencap(device, &opt).unwrap();
        return bitmap_from_image(&image, opt);
    }
    else {
        let output = Command::new("adb").arg("-s").arg(device).arg("exec-out").arg("sh").arg("-c").arg("cd /data/local/tmp/ && ./endorbot --local --screencap")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn().unwrap().wait_with_output().unwrap();
        if output.status.success() {
            return Some(rkyv::from_bytes::<Bitmap, rkyv::rancor::Error>(&output.stdout).unwrap());
        }
    }
    None
}

pub fn screencap(device:&str, opt:&Opt) -> Result<DynamicImage, ScreencapError> {
    if opt.local {
        //screencap_framebuffer(device, opt)
        let output = Command::new("screencap")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()?.wait_with_output()?;
        if output.status.success() {
            return load_bitmap(&output.stdout).map_err(|err|err.into());
        }
    }
    else {
        let output = Command::new("adb").arg("-s").arg(device).arg("exec-out").arg("screencap")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()?.wait_with_output()?;
        if output.status.success() {
            return load_bitmap(&output.stdout).map_err(|err|err.into());
        }
    }
    Err(ScreencapError::Failed)
}

pub fn screencap_framebuffer(device:&str, opt:&Opt) -> Result<DynamicImage, ScreencapError> {
    fn read_fb0_rgba(data:&Vec<u8>) -> Result<DynamicImage, ScreencapError> {
        let width = 1080usize;
        let height = 2408usize;
        let stride_pixels = 1088usize;
        let bpp = 4usize; // RGBA_8888
        let stride_bytes = stride_pixels * bpp;
        let row_bytes = width * bpp;
        let expected = stride_bytes * height;

        let mut pixels = Vec::with_capacity(row_bytes * height);
        for y in 0..height {
            let start = y * stride_bytes;
            let end = start + row_bytes;
            pixels.extend_from_slice(&data[start..end]);
        }

        match image::ImageBuffer::from_raw(width as u32, height as u32, pixels) {
            Some(img) => Ok(image::DynamicImage::ImageRgba8(img)),
            None => Err(ScreencapError::Failed),
        }
    }

    if opt.local {
        let output = std::fs::read("/dev/graphics/fb0")?;
        return read_fb0_rgba(&output).map_err(|err|err.into())
    }
    else {
        let output = Command::new("adb").arg("-s").arg(device).arg("exec-out").arg("su").arg("-c").arg("cat").arg("/dev/graphics/fb0")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()?.wait_with_output()?;
        if output.status.success() {
            return read_fb0_rgba(&output.stdout).map_err(|err|err.into())
        }
    };
    Err(ScreencapError::Failed)
}