use console::Emoji;
use humansize::BINARY;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, Write},
    path::PathBuf,
    process::exit,
};

use clap::Parser;
use depot::depot_handle::DepotHandle;

const PACKAGE: Emoji<'_, '_> = Emoji("📦 ", "[||] ");

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Arguments {
    /// depot path
    path: PathBuf,
    /// action
    #[clap(subcommand)]
    action: Action,
}

#[derive(Debug, Parser)]
enum Action {
    /// create a new depot
    Bake(CreateArgs),
    /// list all streams in a depot
    List(ListArgs),
    /// extraction functionality
    Extract(ExtractArgs),
    /// carve out a stream from a depot without decompressing
    Carve(ExtractArgs),
    /// show a stream's contents on the terminal
    /// (useful for small text streams)
    /// there is no telling what will happen if you try to show a binary stream
    Show(ExtractArgs),
    /// print the table of contents
    PrintToc,
}

#[derive(Debug, Parser)]
struct CreateArgs {
    /// files to add to the depot
    files: Vec<PathBuf>,
    /// if the given path refers to a directory, add all files in the directory
    #[clap(short, long)]
    recurse: bool,
    /// compression level
    #[clap(short, long, default_value = "10")]
    level: i32,
    /// frame size for compression
    /// (the default is 8MB)
    #[clap(short, long, default_value = "8523874304")]
    frame_size: usize,
    /// threads to use for compression
    #[clap(short, long, default_value = "4")]
    threads: usize,
}

#[derive(Debug, Parser)]
struct ListArgs {}

#[derive(Debug, Parser)]
struct ExtractArgs {
    /// output path
    #[clap(short, long, default_value = ".")]
    output: PathBuf,
    /// streams to extract
    streams: Vec<PathBuf>,
}

fn main() {
    let args = Arguments::parse();
    println!("Depot CLI tools {}", env!("CARGO_PKG_VERSION"));
    println!("Copyright (C) 2023, NeonLayer");

    match args.action {
        Action::Bake(cmd_args) => {
            let paths = expand_path(cmd_args.files.clone(), cmd_args.recurse);
            println!(
                "\n{}adding {} files to `{}`",
                PACKAGE,
                paths.len(),
                args.path.display()
            );
            new_depot(
                &args.path,
                paths,
                cmd_args.level,
                cmd_args.threads,
                cmd_args.frame_size,
            )
            .unwrap();
            println!("{}created depot at `{}`", PACKAGE, args.path.display());
        }
        Action::List(_cmd_args) => {
            println!("{}listing contents of `{}`\n", PACKAGE, args.path.display());
            ls_contents(&args.path);
        }
        Action::Extract(cmd_args) => {
            println!(
                "{}extracting `{}` to `{}`",
                PACKAGE,
                args.path.display(),
                cmd_args.output.display()
            );
            extract_files(&args.path, &cmd_args.streams, &cmd_args.output);
        }
        Action::Carve(cmd_args) => {
            println!(
                "{}carving `{}` to `{}`",
                PACKAGE,
                args.path.display(),
                cmd_args.output.display()
            );
            carve_files(&args.path, &cmd_args.streams, &cmd_args.output);
        }
        Action::PrintToc => {
            println!(
                "{}printing table of contents for `{}`",
                PACKAGE,
                args.path.display()
            );
            let dh =
                DepotHandle::open_file(&args.path, depot::depot_handle::OpenMode::Read).unwrap();
            let toc = dh.get_toc();
            println!("{:#?}", toc);
        }
        Action::Show(cmd_args) => {
            let mut dh =
                DepotHandle::open_file(&args.path, depot::depot_handle::OpenMode::Read).unwrap();
            for item in &cmd_args.streams {
                let stream = dh.get_named_stream(&item.to_string_lossy()).unwrap();
                let contents = dh.stream_to_memory(&stream).unwrap();
                println!("Start of {}", stream.name);
                println!("----------------");
                println!("{}", String::from_utf8_lossy(&contents));
                println!("----------------");
                println!("End of {}", stream.name);
            }
        }
    }
}

fn carve_files(path: &PathBuf, streams: &[PathBuf], output: &PathBuf) {
    let dh = DepotHandle::open_file(path, depot::depot_handle::OpenMode::Read).unwrap();
    let mut dhfh = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut bufr = std::io::BufReader::new(&mut dhfh);
    if output.exists() {
        fs::remove_dir_all(output).unwrap();
    }
    fs::create_dir_all(output).unwrap();
    for item in streams {
        let stream = dh.get_named_stream(&item.to_string_lossy()).unwrap();
        println!("carving `{:#?}`", stream);
        let mut outf = output.join(item);
        outf.set_file_name(format!(
            "{}.carved",
            outf.file_name().unwrap().to_string_lossy()
        ));
        let mut fh = File::create(outf).unwrap();
        let mut writer = std::io::BufWriter::new(&mut fh);
        bufr.seek(std::io::SeekFrom::Start(stream.einf.offset))
            .unwrap();
        let mut buf = vec![0; stream.einf.stream_size as usize];
        println!("reading {} bytes", buf.len());
        let mut read = 0;
        while let Ok(n) = bufr.read(&mut buf) {
            if read + n > stream.einf.stream_size as usize {
                writer
                    .write_all(buf[..stream.einf.stream_size as usize - read].as_ref())
                    .unwrap();
                break;
            }
            if n == 0 {
                break;
            }

            writer.write_all(&buf[..n]).unwrap();
            read += n;
        }
        println!("carved `{}`", stream.name);
    }
}

fn ls_contents(path: &PathBuf) {
    let dh = DepotHandle::open_file(path, depot::depot_handle::OpenMode::Read).unwrap();
    let streams: Vec<_> = dh.streams().collect();
    for stream in streams {
        println!("{} {:#?}", stream.0, stream.1);
    }
}

fn extract_files(depot_path: &PathBuf, paths: &Vec<PathBuf>, output: &PathBuf) {
    let mut dh = DepotHandle::open_file(depot_path, depot::depot_handle::OpenMode::Read).unwrap();
    for path in paths {
        let stream = dh.get_named_stream(&path.to_string_lossy()).unwrap();
        fs::create_dir_all(output.join(path.parent().unwrap())).unwrap();
        let mut fh = File::create(output.join(path)).unwrap();
        let mut writer = std::io::BufWriter::new(&mut fh);
        dh.extract_stream(&stream, &mut writer).unwrap();
        println!("extracted `{}`", path.display());
    }
}

fn new_depot(
    path: &PathBuf,
    files: Vec<PathBuf>,
    level: i32,
    threads: usize,
    frame_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let fh = File::create(path)?;
    let pb = indicatif::ProgressBar::new(files.len() as u64);
    pb.set_style(indicatif::ProgressStyle::default_bar().template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>7}/{len:7} {msg}",
    )?);
    let mut dh = DepotHandle::create(fh)?;
    dh.set_comp_level(level);
    dh.set_mt_threads(threads);
    dh.set_comp_frame_size(frame_size);
    dh.flush()?;
    for path in files {
        pb.inc(1);
        let display = path.display();
        let size = fs::metadata(&path)?.len();
        let formated_size = humansize::format_size(size, BINARY);
        let msg = format!("{} ({})", display, formated_size);
        pb.set_message(msg);
        dh.add_file(path, None)?;
    }
    dh.close()?;
    Ok(())
}

fn expand_path(pathl: Vec<PathBuf>, recurse: bool) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    for path in pathl {
        if !path.exists() {
            eprintln!("path `{}` does not exist", path.display());
            exit(1)
        }

        if path.starts_with("..") {
            eprintln!(
                "path `{}` is outside of the current directory",
                path.display()
            );
            exit(1)
        }

        if path.is_dir() && recurse {
            for entry in path.read_dir().unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.is_dir() {
                    paths.extend(expand_path(vec![path], recurse));
                } else if path.is_symlink() {
                    println!("ignoring symlink `{}`", path.display());
                } else {
                    paths.push(path);
                }
            }
        } else if path.is_file() {
            paths.push(path.clone());
        } else {
            eprintln!(
                "refusing to add directory `{}` without --recurse",
                path.display()
            );
            exit(1)
        }
    }

    paths
}

fn update_progress(total: u64, current: u64) {
    print!("\r{}/{}", current / 1048576, total / 1048576)
}
