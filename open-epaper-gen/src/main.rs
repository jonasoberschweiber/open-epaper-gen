//! open-epaper-gen is a small application that generates images for [eInk price
//! tags](https://github.com/OpenEPaperLink/OpenEPaperLink/wiki/1.54″%E2%80%901.6″-ST%E2%80%90GR16000)
//! and sends them to [Open ePaper Link](https://openepaperlink.de). Right now
//! it's only able to generate one image: the current headline from one of four
//! major German news outlets. It has the ability to support more modules in the
//! future, though.
//!
//! open-epaper-gen does not include any scheduling. You'll need to supply that
//! yourself, e.g. via cron.
//!
//! ## Basic Usage
//!
//! When running open-epaper-gen, you need to specify the module that you want
//! to use and an output location. The output location can be a JPEG (in which
//! case you'll also need to specify width and height) or the MAC of an ePaper
//! price tag. Price tags need to be registered in the config file (see below).
//!
//! ```bash
//! open-epaper-gen --module news-headlines --tag 000002186fd53b13
//! ```
//!
//! open-epaper-gen expects both the `resources` folder as well as its config
//! file to be in the current working directory.
//!
//! ## Modules
//!
//! As of now, there is only one module. See "Adding a New Module" if you want
//! to add more.
//!
//! ### News Headlines
//!
//! ID for the command line: news-headlines
//!
//! The news headlines module fetches the latest story from one of four major
//! German news outlets. It displays the headline as well as the logo of the
//! news outlet and the current time. The four news outlets are: Tagesschau,
//! Spiegel Online, Zeit, and Süddeutsche Zeitung.
//!
//! TODO: Add a sample image here.
//!
//! ## Configuration (Setting Up Tags)
//!
//! You need to set up all known tags in the config.toml file. The application
//! looks for that file in the current directory. You can find an example config
//! in config.toml.example.
//!
//! All you have to set up is the IP address/hostname of your Open ePaper Link
//! access point (setting epaper_link_host) and the resolution for each tag:
//!
//! ```toml
//! epaper_link_host = "192.168.1.2"
//!
//! [[tags]]
//! mac = "000002186fd53b13"
//! width = 296
//! height = 128
//!
//! [[tags]]
//! mac = "000002287eef3cde"
//! width = 152
//! height = 152
//! ```
//!
//! ## Writing to a JPEG
//!
//! To write to a JPEG instead of sending the image to Open ePaper Link, use the
//! JPEG option:
//!
//! ```bash
//! open-epaper-gen --module news-headlines --jpeg out.jpeg --width 296 --height 128
//! ```
//!
//! ## Building open-epaper-gen
//!
//! The best way to build this is to use [xtask](https://github.com/matklad/cargo-xtask).
//! This project uses xtask to automate copying the `resources` directory on
//! every build, to build the Docker image, and so on. This project is tiny and
//! xtask is likely very much overkill compared to a bunch of shell scripts --
//! but building this was fun, so why not!
//!
//! To build for your local, native environment:
//!
//! ```
//! cargo xtask build
//! ```
//!
//! Same for running locally:
//!
//! ```
//! cargo xtask run
//! ```
//!
//! If you want to build for Linux x86_64 (and are not yourself running Linux
//! x86_64):
//!
//! ```
//! cargo xtask build --platform linux-x64 --release
//! ```
//!
//! The `--release` is optional and works for `build` without a platform as
//! well.
//!
//! You can use `cargo xtask package` to build the Docker container. `cargo
//! xtask release` will push that container up to GHCR.
//!
//! ## How the Code is Organized
//!
//! The `main` module contains the basic application flow, argument parsing, and
//! reads the config file. All it does is figure out the module and output to
//! use and orchestrate the flow between those.
//!
//! [`modules`] contains enums and traits for implementing new modules. Look in
//! there if you want to write your own module (although there's really not all
//! that much too look at — it's pretty simple).
//!
//! [`draw`] is a bit more interesting. That module contains a the primitives for
//! drawing the image. Right now, that includes a text view, an image view, and
//! a very basic layout system inspired by SwiftUI.
//!
//! [`news_headlines`] contains the code for the news headlines module.
//!
//! External resources should go into the `resources` folder. Put global
//! resources — such as fonts — on the root level. Module-specific resources
//! should go into a subfolder named for the module.
//!
//! ## Adding a New Module
//!
//! To add a new module, you'll need to implement the [`modules::InfoView`] trait.
//! Then you'll need to an an entry to the [`modules::Module`] enum. And there's
//! a `match` expression in `main` that instantiates the correct struct based on
//! the `module` CLI parameter.

mod modules;
mod draw;
mod news_headlines;

use serde::{Deserialize};
use reqwest::blocking::multipart;
use anyhow::{Result, Context, bail};
use clap::{Parser};
use config::{Config};
use log::{info, error};
use env_logger::Env;
use image::ImageFormat;
use tempfile::NamedTempFile;

use crate::draw::Surface;
use crate::news_headlines::NewsHeadlines;
use crate::modules::{InfoView, Module};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// The module to run.
    #[arg(long, value_enum)]
    module: Module,

    /// The width of the image (only relevant when using JPEG output).
    /// Common sizes are (width x height):
    ///   - TODO
    #[arg(long)]
    width: Option<u32>,

    /// The height of the image (only relevant when using JPEG output).
    /// See the width argument for common sizes.
    #[arg(long)]
    height: Option<u32>,

    /// Path to an output JPEG. You'll also need to specify the output width and
    /// height.
    #[arg(long, required = true, group = "output", requires = "width", requires = "height")]
    jpeg: Option<String>,

    /// The MAC address of the tag to send the image to.
    #[arg(long, required = true, group = "output")]
    tag: Option<String>,

    /// The config file to use (will default to config.toml in the current
    /// directory).
    #[arg(long)]
    config: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct Tag {
    mac: String,
    width: u32,
    height: u32,
}

#[derive(Debug, Deserialize)]
struct Settings {
    epaper_link_host: String,
    tags: Vec<Tag>,
}

fn find_tag(settings: &Settings, mac: &str) -> Option<Tag> {
    settings.tags.iter().find(|t| t.mac == mac).map(|t| t.clone())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    info!("Starting up and reading config file");

    let config_file = cli.config.unwrap_or("config.toml".to_string());
    let settings: Settings = Config::builder()
        .add_source(config::File::with_name(config_file.as_str()))
        .build()?
        .try_deserialize()?;

    let tag_mac = match cli.tag {
        Some(ref mac) => mac,
        None => &"".to_string()
    };
    if cli.tag.is_some() {
        info!("Using tag {:?} as target", tag_mac);
        let possible_tag = find_tag(&settings, tag_mac);
        if possible_tag.is_none() {
            bail!("No tag with MAC {:?} found in the config file!", tag_mac);
        }
    } else {
        info!("Using JPEG file {:?} as target", cli.jpeg.clone().unwrap());
    }

    let module = match cli.module {
        Module::NewsHeadlines => NewsHeadlines{},
    };

    info!("Using module {:?} to generate the image", cli.module);

    // We can determine the width and height of the target surface in two ways:
    // Either the user specified JPEG output. In that case they need to specify
    // width and height as command line arguments. Or they have specified a tag
    // ID, in which case we can look up that tag in our config file and find the
    // width and height that way.
    let (surface_width, surface_height) = if cli.jpeg.is_some() {
        (
            cli.width
                .expect("You need to specify width for JPEG output"),
            cli.height
                .expect("You need to specify height for JPEG output")
        )
    } else {
        let tag = find_tag(&settings, tag_mac).unwrap();
        (tag.width, tag.height)
    };

    let mut surface = Surface::new(surface_width, surface_height)
        .with_context(|| {
            format!("Could not create surface {:?}x{:?}", surface_width, surface_height)
        })?;

    let options = module.generate(&mut surface)
        .with_context(|| format!("Module {:?} reported an error", cli.module))?;

    if cli.jpeg.is_some() {
        info!("Saving image to {:?}", cli.jpeg.clone());
        surface.img.save(cli.jpeg.unwrap())?;
        return Ok(())
    }

    let temp_jpeg = NamedTempFile::new()?;
    info!("Saving image to temporary file {:?}", temp_jpeg.path());
    surface.img.save_with_format(temp_jpeg.path(), ImageFormat::Jpeg)?;

    let client = reqwest::blocking::Client::new();
    let mut form = multipart::Form::new()
        .text("mac", tag_mac.clone())
        .text("dither", "0");

    if options.ttl.is_some() {
        let minutes = options.ttl.unwrap();
        form = form.text("ttl", format!("{}", minutes));
    }

    form = form.file("test", temp_jpeg.path())?;


    info!("Sending request to open epaper link at {:?}", settings.epaper_link_host);
    let res = client.post(format!("http://{}/imgupload", settings.epaper_link_host))
        .multipart(form)
        .send()?;
    if !res.status().is_success() {
        bail!("Open ePaper Link AP responded with error: {:?}", res.status());
    }

    Ok(())
}

