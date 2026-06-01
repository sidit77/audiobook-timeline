use itertools::Itertools;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use reqwest::Client;
use serde::Deserialize;
use tokio::spawn;

#[tokio::main]
async fn main() -> anyhow::Result<()> {

    let client = Client::new();

    //let config: Config = toml::from_str(&std::fs::read_to_string("audiobooks.toml")?)?;
    // Try to find the config file in the same directory as the binary, if not found, try the current working directory
    let config_path = std::env::current_exe()?
        .parent()
        .expect("binary has no parent directory")
        .join("audiobooks.toml");
    
    let config_path = if config_path.exists() {
        config_path
    } else {
        std::path::PathBuf::from("audiobooks.toml")
    };
    
    let config: Config = toml::from_str(&std::fs::read_to_string(config_path)?)?;

    let mut results = {
        let task = config
            .authors
            .iter()
            .cloned()
            .map(|a| spawn(by_author(client.clone(), a)))
            .collect::<Vec<_>>();
        let mut results = Vec::new();
        for task in task {
            results.extend_from_slice(&task.await??);
        }
        results
    };
    results.retain(|x| config.languages.contains(&x.language));
    results.sort_by_key(|x| x.publication_datetime);

    let mut past = true;
    let now = Timestamp::now();
    for x in results.iter().unique_by(|x| (x.author.clone(), x.title.clone())) {
        if past && x.publication_datetime > now {
            let t = now.to_zoned(TimeZone::system());
            println!("------- NOW ({:02}:{:02}) -------", t.hour(), t.minute());
            past = false;
        }
        let t = x.publication_datetime.to_zoned(TimeZone::system());
        //println!("{:02}.{:02}.{:04} {:02}:{:02}: {} ({}) [{} #{}]", t.day(), t.month(), t.year(), t.hour(), t.minute(), x.title, x.author,x.series.as_ref().map(|s| s.title.clone()).unwrap_or_default(), x.series.as_ref().map(|s| s.sequence.clone()).unwrap_or_default());
        // If the book is part of a series, print the series title and sequence number
        let series_str = x.series.as_ref()
            .filter(|s| !s.title.is_empty())
            .map(|s| format!(" [{} #{}]", s.title, s.sequence))
            .unwrap_or_default();
        println!(
            "{:02}.{:02}.{:04} {:02}:{:02}: {} ({}){series_str}",
            t.day(), t.month(), t.year(), t.hour(), t.minute(),
            x.title, x.author
        );
    }

    Ok(())
}

async fn by_author(client: Client, author: String) -> anyhow::Result<Vec<Book>> {
    let mut page = 0;
    let mut results = Vec::new();
    while {
        let query_result = by_author_paged(client.clone(), &author, page).await?;
        results.extend(query_result.products);
        page += 1;
        results.len() < query_result.total_results as usize
    } {}
    Ok(results)
}

async fn by_author_paged(client: Client, author: &str, page: u32) -> anyhow::Result<AuthorQueryResult> {
    let response_groups = "product_desc,product_attrs,series,product_extended_attrs";
    let mut x = client
        .get(format!("https://api.audible.com/1.0/catalog/products?response_groups={response_groups}&author={author}&num_results=50&page={page}"))
        .send()
        .await?
        .json::<AuthorQueryResult>()
        .await?;
    x.products.iter_mut().for_each(|x| x.author = author.to_string());
    Ok(x)
}

#[derive(Debug, Clone, Deserialize)]
struct AuthorQueryResult {
    products: Vec<Book>,
    total_results: u32
}

#[derive(Debug, Clone, Deserialize)]
struct Book {
    title: String,
    #[serde(default)]
    author: String,
    //issue_date: String,
    publication_datetime: Timestamp,
    language: String,
    //asin: String,
    #[serde(default, deserialize_with = "deserialize_first")]
    series: Option<Series>,
}
fn deserialize_first<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Series>, D::Error> {
    Ok(Vec::<Series>::deserialize(d)?.into_iter().next())
}
#[derive(Debug, Clone, Deserialize)]
struct Series {
    //asin: String,
    sequence: String,
    title: String,
    //url: String,
}
#[derive(Debug, Clone, Deserialize)]
struct Config {
    languages: Vec<String>,
    authors: Vec<String>,
}
