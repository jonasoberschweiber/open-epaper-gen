//! This module will grab the top headline of one of four major German news
//! outlets (Tagesschau, Der Spiegel, Süddeutsche Zeitung, and Die Zeit).
//!
//! It just fetches the RSS feeds of the respective news site and uses the title
//! of the first story in the feed. Much of the time that seems to match the top
//! story shown on the outlet's website reasonably well...

use std::fs::File;
use chrono::{Local};
use anyhow::{Context, Result, anyhow, bail};
use feed_rs::parser;
use rand::{Rng};
use log::{info};

use crate::draw::{Surface, VStack, VAlign, HStack, Text, Edge, Spacer, View, Image};
use crate::modules::InfoView;

pub struct NewsHeadlines {
}

impl InfoView for NewsHeadlines {
    fn generate(&self, surface: &mut Surface) -> Result<()> {
        let surface_bounds = surface.bounds().clone();
    
        let news_outlets = vec![
            NewsOutlet{
                name: "Tagesschau".to_string(),
                rss_endpoint: "https://www.tagesschau.de/index~rss2.xml".to_string(),
                logo_path: logo_path("tagesschau.png")?,
            },
            NewsOutlet{
                name: "Spiegel".to_string(),
                rss_endpoint: "https://www.spiegel.de/schlagzeilen/tops/index.rss".to_string(),
                logo_path: logo_path("spiegel.png")?,
            },
            NewsOutlet{
                name: "Sueddeutsche".to_string(),
                rss_endpoint: "https://rss.sueddeutsche.de/rss/Topthemen".to_string(),
                logo_path: logo_path("sz.png")?,
            },
            NewsOutlet{
                name: "Zeit".to_string(),
                rss_endpoint: "http://newsfeed.zeit.de/index".to_string(),
                logo_path: logo_path("zeit.png")?,
            },
        ];

        let mut rng = rand::thread_rng();
        let outlet_ix = rng.gen_range(0..news_outlets.len());
        let news_outlet = news_outlets.get(outlet_ix).unwrap();
        info!("Fetching data from news outlet {:?}", news_outlet.name);

        let client = reqwest::blocking::Client::new();
        let res = client.get(news_outlet.rss_endpoint.as_str())
            .send()
            .with_context(|| {
                format!("Failed to request data from {:?}", news_outlet.name)
            })?;
        if !res.status().is_success() {
            bail!(
                "The request to {:?} returned a non-OK status: {:?}",
                news_outlet.name,
                res.status()
            );
        }
        let body = res.text()?;
        let feed = parser::parse(body.as_bytes())?;
        let first_entry = feed.entries.first()
            .ok_or(anyhow!("There is no entry in the feed."))?;
        let headline_text = first_entry.title
            .clone()
            .ok_or(anyhow!("The first entry has no title."))?
            .content;
        
        let mut screen = VStack::new();
        let mut headline = Text::new(headline_text.to_string(), 20.0, 1);
        headline.wrap_text = true;
        headline.padding(Edge::Top, 10);
        headline.padding(Edge::Left, 10);
        headline.padding(Edge::Right, 10);
        headline.padding(Edge::Bottom, 10);

        let mut font_size = 40.0;
        let max_headline_height = 128 - 30 /* bottom bar */ - 20 /* padding */;
        let max_headline_width = 296 - 20 /* padding */;
        loop {
            headline.size = font_size;
            let text_bounds = headline.bounds(&surface, surface_bounds.optimally_hinted());
            if text_bounds.height < max_headline_height &&
                text_bounds.width < max_headline_width {
                break;
            }
            font_size = font_size - 1.0;
        }

        let mut bottom_bar = HStack::new();
        bottom_bar.padding(Edge::Right, 10);
        bottom_bar.padding(Edge::Bottom, 10);
        bottom_bar.padding(Edge::Left, 10);
        bottom_bar.align = VAlign::Bottom;

        let logo_file = File::open(news_outlet.logo_path.as_str())
            .with_context(|| format!("Can't open logo file {:?}", news_outlet.logo_path))?;
        let logo = Image::from_data(logo_file)?;

        bottom_bar.views.push(Box::new(logo));
        bottom_bar.views.push(Box::new(Spacer::horizontal()));
        bottom_bar.views.push(Box::new(Text::new(Local::now().format("%m-%d %H:%M").to_string(), 13.0, 0)));

        screen.views.push(Box::new(headline));
        screen.views.push(Box::new(Spacer::vertical()));
        screen.views.push(Box::new(bottom_bar));

        screen.draw(surface, 0, 0, surface_bounds);

        Ok(())
    }
}

struct NewsOutlet {
    name: String,
    rss_endpoint: String,
    logo_path: String,
}

fn logo_path(logo: &str) -> Result<String> {
    Ok(std::env::current_exe()?
        .parent().ok_or(anyhow!("Current executable path has no parent."))?
        .join("resources")
        .join("news_headlines")
        .join(logo)
        .to_str().ok_or(anyhow!("Can't convert path to string."))?
        .to_string())
}
