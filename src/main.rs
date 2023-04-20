use std::{fs, io::Write, sync::Arc};

use clap::Parser;
use futures_util::future::join_all;
use reqwest::get;
use tokio::{sync::Semaphore, task};

fn deg2num(lat_deg: f64, lon_deg: f64, zoom: u32) -> (u32, u32) {
    let lat_rad = lat_deg.to_radians();
    let n = 2u32.pow(zoom);
    let xtile = ((lon_deg + 180.0) / 360.0 * n as f64).floor() as u32;
    let ytile =
        ((1.0 - (lat_rad.tan() + (1.0 / lat_rad.cos())).atanh()) / 2.0 * n as f64).floor() as u32;
    (xtile, ytile)
}

async fn download_tile(
    z: u32,
    x: u32,
    y: u32,
    base_url: &str,
    api_key: &str,
) -> Result<(), reqwest::Error> {
    let url = format!("{base_url}/{z}/{x}/{y}.png?apikey={api_key}");

    let response = get(&url).await?.bytes().await?;

    let dir = format!("tiles/{}/{}", z, x);
    fs::create_dir_all(&dir).expect("Failed to create directory");
    let file = format!("{}/{}.png", dir, y);

    let mut output_file = fs::File::create(file).expect("Failed to create file");
    output_file
        .write_all(&response)
        .expect("Failed to write image");

    println!("Downloaded tile {}/{}/{}", z, x, y);
    Ok(())
}

#[derive(Debug, Clone, Parser)]
struct Args {
    #[clap(short, long)]
    api_key: String,
    #[clap(short, long)]
    base_url: String,
    #[clap(short, long, default_value_t = 4)]
    parallel_downloads: usize,
    #[clap(long)]
    min_zoom: u32,
    #[clap(long)]
    max_zoom: u32,
    #[clap(long)]
    min_lon: f64,
    #[clap(long)]
    max_lon: f64,
    #[clap(long)]
    min_lat: f64,
    #[clap(long)]
    max_lat: f64,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = Args::parse();

    let api_key = Arc::new(args.api_key);
    let base_url = Arc::new(args.base_url);
    let semaphore = Arc::new(Semaphore::new(args.parallel_downloads));

    for zoom in args.min_zoom..=args.max_zoom {
        let (min_x, min_y) = deg2num(args.min_lat, args.min_lon, zoom);
        let (max_x, max_y) = deg2num(args.max_lat, args.max_lon, zoom);

        let mut handles = vec![];

        for x in min_x..=max_x {
            for y in min_y..=max_y {
                let api_key = Arc::clone(&api_key);
                let base_url = Arc::clone(&base_url);
                let semaphore = Arc::clone(&semaphore);
                let handle = task::spawn(async move {
                    let _permit = semaphore.acquire().await;
                    if let Err(e) = download_tile(zoom, x, y, &base_url, &api_key).await {
                        eprintln!("Error downloading tile {}/{}/{}: {}", zoom, x, y, e);
                    }
                });

                handles.push(handle);
            }
        }

        // Join all handles, so they run concurrently
        let results = join_all(handles).await;

        // Print any errors that occurred
        for (idx, result) in results.into_iter().enumerate() {
            if let Err(e) = result {
                eprintln!("Error downloading tile {}: {}", idx, e);
            }
        }
    }

    println!("All tiles have been downloaded.");
}
