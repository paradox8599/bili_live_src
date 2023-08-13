#![allow(dead_code)]
use clap::Parser;
use regex::Regex;
use serde_json::{Error, Value};
use std::io::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Args {
        format,
        quality,
        room_id,
    } = Args::parse();
    let is_cli = format.is_some() || quality.is_some() || room_id.is_some();

    let format = Format::from_str(&format.unwrap_or("".to_string())).ok();
    let quality = Quality::from_str(&quality.unwrap_or("".to_string())).ok();

    let room_id = match room_id {
        Some(id) => id,
        None => read_room_id(),
    };
    let quality = match quality {
        Some(q) => q,
        None => read_quality(),
    };
    let format = match format {
        Some(f) => f,
        None => read_format(),
    };

    if !is_cli {
        println!("正在获取直播源...\n");
    }
    let stream = fetch_stream(room_id, quality).await?;
    let urls = parse_stream(stream);

    let urls = urls
        .iter()
        .filter(|u| u.contains(&format.value()))
        .collect::<Vec<&String>>();

    for url in urls.iter() {
        println!("{}", url.trim_matches('"'));
    }
    if !is_cli {
        pause();
    }

    Ok(())
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    room_id: Option<u32>,
    #[arg(short, long)]
    quality: Option<String>,
    #[arg(short, long)]
    format: Option<String>,
}

fn pause() {
    let mut stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    write!(stdout, "Press any key to continue...").unwrap();
    stdout.flush().unwrap();
    let _ = stdin.read(&mut [0u8]).unwrap();
}

fn read_room_id() -> u32 {
    let mut line = String::new();
    let mut room_id: Result<u32, &'static str> = Err("init");
    while room_id.is_err() {
        println!("输入房间号或直播间地址: ");
        std::io::stdin().read_line(&mut line).unwrap();

        let re = Regex::new(r"(http[s]?://)?(live.bilibili.com/)?(\d+)").unwrap();
        let caps = re.captures(line.trim());

        room_id = match caps {
            None => Err("直播间地址或房间号格式不正确。"),
            Some(caps) => Ok(caps.get(3).unwrap().as_str().parse::<u32>().unwrap()),
        };
        if let Err(e) = room_id {
            println!("{}", e);
        }
    }
    room_id.unwrap()
}

fn read_quality() -> Quality {
    let mut line = String::new();
    let mut result: Result<Quality, &'static str> = Err("init");

    while result.is_err() {
        println!("\n选择画质:");
        println!("1. 流畅");
        println!("2. 原画\n");

        std::io::stdin().read_line(&mut line).unwrap();
        result = match line.trim().parse::<u32>() {
            Ok(1) => Ok(Quality::Low),
            Ok(2) => Ok(Quality::High),
            _ => Err(""),
        };
    }
    result.unwrap()
}

fn read_format() -> Format {
    let mut line = String::new();
    let mut result: Result<Format, &'static str> = Err("init");

    while result.is_err() {
        println!("\n选择格式:");
        println!("1. m3u8");
        println!("2. flv\n");

        std::io::stdin().read_line(&mut line).unwrap();
        result = match line.trim().parse::<u32>() {
            Ok(1) => Ok(Format::M3u8),
            Ok(2) => Ok(Format::Flv),
            _ => Err(""),
        };
    }
    result.unwrap()
}

enum Format {
    M3u8,
    Flv,
}
impl Format {
    fn from_str(s: &str) -> Result<Self, &'static str> {
        match s {
            "m3u8" => Ok(Self::M3u8),
            "flv" => Ok(Self::Flv),
            _ => Err(""),
        }
    }
    fn value(&self) -> String {
        match self {
            Self::M3u8 => "m3u8".to_string(),
            Self::Flv => "flv".to_string(),
        }
    }
}

enum Quality {
    Low,
    High,
}
impl Quality {
    fn from_str(s: &str) -> Result<Self, &'static str> {
        match s {
            "low" => Ok(Self::Low),
            "high" => Ok(Self::High),
            _ => Err(""),
        }
    }

    fn value(&self) -> u32 {
        match self {
            Self::Low => 0,
            Self::High => 10000,
        }
    }
}

async fn fetch_stream(room_id: u32, qn: Quality) -> Result<Vec<Value>, &'static str> {
    let base_url = "https://api.live.bilibili.com/xlive/web-room/v2/index/getRoomPlayInfo";
    let url = format!(
        "{}?qn={}&protocol=0,1&format=0,1,2&codec=0,1&room_id={}",
        base_url,
        qn.value(),
        room_id
    );
    let resp = reqwest::get(url).await;
    if resp.is_err() {
        return Err("网络请求出错，请稍后再试。");
    }
    let resp = resp.unwrap();
    let resp = resp.text().await.unwrap();
    let resp: Result<Value, Error> = serde_json::from_str(&resp);
    if resp.is_err() {
        return Err("接口返回格式错误");
    }
    let resp = resp.unwrap();

    let code = &resp["code"].as_i64().unwrap();
    if *code != 0 {
        return Err("请求出错。");
    }

    let live_status = &resp["data"]["live_status"];
    if live_status.as_i64().unwrap() == 0 {
        return Err("未开播。");
    }

    let stream = &resp["data"]["playurl_info"]["playurl"]["stream"]
        .as_array()
        .unwrap();

    Ok(stream.to_owned().to_owned())
}

fn parse_stream(stream: Vec<Value>) -> Vec<String> {
    stream
        .iter()
        .flat_map(|s| {
            s["format"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|f| {
                    f["codec"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .flat_map(|c| {
                            c["url_info"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|i| {
                                    format!(
                                        "{}{}{}",
                                        i["host"].to_string().trim_matches('"'),
                                        c["base_url"].to_string().trim_matches('"'),
                                        i["extra"].to_string().trim_matches('"')
                                    )
                                })
                                .collect::<Vec<String>>()
                        })
                        .collect::<Vec<String>>()
                })
                .collect::<Vec<String>>()
        })
        .collect::<Vec<String>>()
}
