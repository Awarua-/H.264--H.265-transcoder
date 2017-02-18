#[macro_use]
extern crate log;
extern crate log4rs;

#[macro_use]
extern crate clap;

extern crate time;

use clap::App;
use std::process::{Command, Output, Stdio, ExitStatus};
use std::fs::{copy, remove_file};
use std::path::Path;
use std::io::{BufReader, BufRead, Read, Result};
use std::ffi::OsStr;
use std::env;

static NVENC_CHECK_STRING: &'static str = "supports NVENC";
static EXTENSION: &'static str = "mkv";
static H264_CHECK_STRING: &'static str = "h264";

fn run(command: String, args: Vec<String>) -> Output {
    let error_message = format!("{} failed :(", command);
    Command::new(command)
        .args(args.as_slice())
        .output()
        .expect(error_message.as_str())
}

fn consume_stdio<R: Read>(mut buffered_reader: BufReader<R>) {
    let mut buffer = String::new();

    while buffered_reader.read_line(&mut buffer).unwrap() > 0 {
        let b = buffer.to_owned();
        buffer.clear();
        println!("{}", b.as_str());
    }
}

fn run_with_stdio(command: String, args: Vec<String>) -> Result<ExitStatus> {
    let mut cmd = Command::new(command)
        .args(args.as_slice())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    consume_stdio(BufReader::new(cmd.stderr.take().unwrap()));
    consume_stdio(BufReader::new(cmd.stdout.take().unwrap()));
    cmd.wait()
}

fn timestamp() -> i64 {
    let timespec = time::get_time();
    let mills: f64 = timespec.sec as f64 + (timespec.nsec as f64 / 1000.0 / 1000.0 / 1000.0);
    mills.trunc() as i64
}

fn main() {

    log4rs::init_file("log4rs.yaml", Default::default()).unwrap();

    println!("starting up");
    info!("starting up");
    let yaml = load_yaml!("cli.yaml");
    let matches = App::from_yaml(yaml).get_matches();

    // Check support for nvidia hardware accelerated encoding
    let ffmpeg = run("ffmpeg".to_string(),
                     vec![String::from("-f"),
                          String::from("lavfi"),
                          String::from("-i"),
                          String::from("nullsrc"),
                          String::from("-c:v"),
                          String::from("nvenc"),
                          String::from("-gpu"),
                          String::from("list"),
                          String::from("-f"),
                          String::from("null"),
                          String::from("-")]);



    let supports_nvenc = String::from_utf8_lossy(ffmpeg.stderr.as_slice())
        .contains(NVENC_CHECK_STRING);
    let hwaccel = if supports_nvenc {
        println!("Supports NVENC");
        warn!("Supports NVENC");
        true
    } else {
        false
    };

    let path = Path::new(matches.value_of("FILE").unwrap());

    if !path.is_file() {
        println!("FILE is not a file, exiting");
        warn!("FILE is not a file, exiting");
        return;
    }
    let dir;
    let temp_path: &Path = if matches.is_present("TEMP_DIR") {
        Path::new(matches.value_of("TEMP_DIR").unwrap())
    } else {
        dir = env::temp_dir();
        dir.as_path()
    };

    if !temp_path.is_dir() {
        println!("TEMP_DIR is not a path, exiting");
        warn!("TEMP_DIR is not a path, exiting");
        return;
    }



    if path.extension().unwrap_or(OsStr::new("")) != EXTENSION {
        println!("File is not in the supported container {}", EXTENSION);
        warn!("File is not in the supported container {}", EXTENSION);
        return;
    }

    let file_path_string = path.as_os_str().to_os_string().into_string().unwrap();
    println!("processing {}", file_path_string);
    info!("processing {}", file_path_string);

    // TODO find a better way instead of copying the string
    let file_path_string_2 = file_path_string.to_owned();

    // check codec type
    let ffprobe = run("ffprobe".to_string(),
                      vec![String::from("-v"),
                           String::from("quiet"),
                           String::from("-show_entries"),
                           String::from("stream=codec_name"),
                           String::from("-select_streams"),
                           String::from("v:0"),
                           String::from("-of"),
                           String::from("default=noprint_wrappers=1"),
                           file_path_string_2]);

    let output = String::from_utf8_lossy(ffprobe.stdout.as_slice());
    let is_h264 = output.contains(H264_CHECK_STRING);
    if !is_h264 {
        println!("File was not {}, but was {}", H264_CHECK_STRING, output);
        warn!("File was not {}, but was {}", H264_CHECK_STRING, output);
        return;
    }

    let copy_file_name = path.file_name().unwrap();

    let copy_file_path = temp_path.join(Path::new(copy_file_name.to_str().unwrap()));
    let copy_file_path_copy = copy_file_path.to_owned();
    let copy_file_path_string = copy_file_path.into_os_string().into_string().unwrap();


    let bytes_copied = copy(&file_path_string, &copy_file_path_copy).unwrap();

    println!("file copied size of {}", bytes_copied);
    info!("file copied size of {}", bytes_copied);

    let file_stem = path.file_stem().unwrap().to_str().unwrap();
    let now = timestamp().to_string();

    let mut file_name = String::from(file_stem);
    file_name.push_str("_");
    file_name.push_str(now.as_str());
    file_name.push_str(".");
    file_name.push_str(EXTENSION);

    let temp_file_path = temp_path.join(Path::new(file_name.as_str()));

    let temp_file_path_string = temp_file_path.into_os_string().into_string().unwrap();
    println!("creating {}", temp_file_path_string);
    info!("creating {}", temp_file_path_string);


    let temp_file_path_string_copy = temp_file_path_string.to_owned();

    let ffmpeg_session;
    if hwaccel {
        ffmpeg_session = run_with_stdio("ffmpeg".to_string(),
                                        vec![String::from("-c:v"),
                                             String::from("h264_cuvid"),
                                             String::from("-i"),
                                             copy_file_path_string,
                                             String::from("-map"),
                                             String::from("0"),
                                             String::from("-c"),
                                             String::from("copy"),
                                             String::from("-c:v"),
                                             String::from("hevc_nvenc"),
                                             String::from("-preset"),
                                             String::from("slow"),
                                             temp_file_path_string]);
    } else {
        ffmpeg_session = run_with_stdio("ffmpeg".to_string(),
                                        vec![String::from("-i"),
                                             copy_file_path_string,
                                             String::from("-map"),
                                             String::from("0"),
                                             String::from("-c"),
                                             String::from("copy"),
                                             String::from("-c:v"),
                                             String::from("-c:v"),
                                             String::from("libx265"),
                                             String::from("-preset"),
                                             String::from("medium"),
                                             temp_file_path_string]);
    }

    let exit_code = ffmpeg_session.unwrap().code().unwrap();
    if exit_code != 0 {
        println!("something went wrong processing file {}", file_path_string);
        warn!("something went wrong processing file {}", file_path_string);
    } else {
        // copy file
        let bytes_copied = copy(&temp_file_path_string_copy, file_path_string).unwrap();

        println!("file copied size of {}", bytes_copied);
        info!("file copied size of {}", bytes_copied);
    }

    let result1 = remove_file(&temp_file_path_string_copy);
    if result1.is_err() {
        println!("could not remove file {}", temp_file_path_string_copy);
        warn!("could not remove file {}", temp_file_path_string_copy);
    }
    let result2 = remove_file(&copy_file_path_copy);
    if result2.is_err() {
        let string = copy_file_path_copy.into_os_string().into_string().unwrap();
        println!("could not remove file {}", string);
        warn!("could not remove file {}", string);
    }
}
