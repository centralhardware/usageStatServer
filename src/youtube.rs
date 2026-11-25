//! Looks up the YouTube videos in `mac_media.url` and their channels with the YouTube Data API
//! (videos.list and channels.list: one quota unit per 50 ids, of 10 000 a day).

use crate::repository::UsageRepository;
use serde::{Deserialize, Deserializer, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const API: &str = "https://www.googleapis.com/youtube/v3";
/// The API's limit of ids per request.
const PAGE: usize = 50;
/// A search costs 100 quota units, so this leaves a fifth of the day's 10 000 to the lookups.
const SEARCHES_PER_DAY: u64 = 80;
const SEARCHES_PER_RUN: u64 = 5;
const CHANNEL_PARTS: &str = "snippet,statistics,topicDetails,brandingSettings";

#[derive(clickhouse::Row, Serialize)]
struct Artist {
    artist: String,
    channel_id: String,
    guess: u8,
    searched_at: i64,
}

/// A search.list result: there the id is an object.
#[derive(Deserialize)]
struct Hit {
    id: HitId,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HitId {
    channel_id: String,
}

#[derive(Debug, PartialEq, clickhouse::Row, Serialize)]
pub struct Video {
    video_id: String,
    channel_id: String,
    title: String,
    description: String,
    category_id: String,
    tags: Vec<String>,
    duration: String,
    views: u64,
    likes: u64,
    published_at: String,
    fetched_at: i64,
}

#[derive(Debug, PartialEq, clickhouse::Row, Serialize)]
pub struct Channel {
    channel_id: String,
    url: String,
    handle: String,
    title: String,
    description: String,
    country: String,
    keywords: String,
    topics: Vec<String>,
    thumbnail_url: String,
    subscribers: u64,
    views: u64,
    videos: u64,
    published_at: String,
    fetched_at: i64,
}

#[derive(Deserialize)]
struct List<T> {
    #[serde(default = "Vec::new")]
    items: Vec<T>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Item {
    id: String,
    snippet: Snippet,
    statistics: Statistics,
    content_details: ContentDetails,
    topic_details: TopicDetails,
    branding_settings: Branding,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Snippet {
    channel_id: String,
    title: String,
    description: String,
    category_id: String,
    tags: Vec<String>,
    published_at: String,
    custom_url: String,
    country: String,
    thumbnails: Thumbnails,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Thumbnails {
    high: Thumbnail,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Thumbnail {
    url: String,
}

/// Counts come as strings, and are left out when hidden.
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Statistics {
    #[serde(deserialize_with = "count")]
    view_count: u64,
    #[serde(deserialize_with = "count")]
    like_count: u64,
    #[serde(deserialize_with = "count")]
    subscriber_count: u64,
    #[serde(deserialize_with = "count")]
    video_count: u64,
}

fn count<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    Ok(String::deserialize(d)?.parse().unwrap_or(0))
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct ContentDetails {
    duration: String,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct TopicDetails {
    /// Wikipedia links, e.g. https://en.wikipedia.org/wiki/Rock_music.
    topic_categories: Vec<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Branding {
    channel: BrandingChannel,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct BrandingChannel {
    keywords: String,
}

/// The item for each id asked for, in order; an id the API doesn't return gets an empty one, so
/// that its row keeps it from being asked for again every minute.
fn by_id(ids: &[String], items: Vec<Item>) -> impl Iterator<Item = (&String, Item)> {
    let mut found: std::collections::HashMap<String, Item> = items.into_iter().map(|i| (i.id.clone(), i)).collect();
    ids.iter().map(move |id| (id, found.remove(id).unwrap_or_default()))
}

fn videos(ids: &[String], items: Vec<Item>, now: i64) -> Vec<Video> {
    by_id(ids, items)
        .map(|(id, i)| Video {
            video_id: id.clone(),
            channel_id: i.snippet.channel_id,
            title: i.snippet.title,
            description: i.snippet.description,
            category_id: i.snippet.category_id,
            tags: i.snippet.tags,
            duration: i.content_details.duration,
            views: i.statistics.view_count,
            likes: i.statistics.like_count,
            published_at: i.snippet.published_at,
            fetched_at: now,
        })
        .collect()
}

/// The channel of that name with the most subscribers, and whether others have the name too.
fn named(artist: &str, items: Vec<Item>) -> Option<(Item, bool)> {
    let name = artist.trim().to_lowercase();
    let mut same: Vec<Item> = items.into_iter().filter(|i| i.snippet.title.trim().to_lowercase() == name).collect();
    same.sort_by_key(|i| i.statistics.subscriber_count);
    let best = same.pop()?;
    Some((best, !same.is_empty()))
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

fn channel(id: &str, i: Item, now: i64) -> Channel {
    Channel {
        url: format!("https://www.youtube.com/channel/{id}"),
        channel_id: id.to_owned(),
        handle: i.snippet.custom_url,
        title: i.snippet.title,
        description: i.snippet.description,
        country: i.snippet.country,
        keywords: i.branding_settings.channel.keywords,
        topics: i.topic_details.topic_categories.iter().filter_map(|t| Some(t.rsplit_once('/')?.1.replace('_', " "))).collect(),
        thumbnail_url: i.snippet.thumbnails.high.url,
        subscribers: i.statistics.subscriber_count,
        views: i.statistics.view_count,
        videos: i.statistics.video_count,
        published_at: i.snippet.published_at,
        fetched_at: now,
    }
}

pub struct YouTube {
    http: reqwest::Client,
    key: String,
}

type Error = Box<dyn std::error::Error + Send + Sync>;

impl YouTube {
    pub fn new(key: String) -> Result<Self, reqwest::Error> {
        Ok(Self { http: reqwest::Client::builder().timeout(Duration::from_secs(20)).build()?, key })
    }

    async fn list(&self, resource: &str, part: &str, ids: &[String]) -> Result<Vec<Item>, Error> {
        self.get(resource, &[("part", part), ("id", ids.join(",").as_str()), ("maxResults", "50")]).await
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, resource: &str, query: &[(&str, &str)]) -> Result<Vec<T>, Error> {
        let resp = self
            .http
            .get(format!("{API}/{resource}"))
            // A header, so that the key is in no URL an error message might print.
            .header("X-Goog-Api-Key", &self.key)
            .query(query)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("YouTube {resource}: HTTP {status}: {}", resp.text().await.unwrap_or_default()).into());
        }
        Ok(resp.json::<List<T>>().await?.items)
    }

    /// Finds the channels of artists whose plays have no link, by their exact name.
    pub async fn find_artists(&self, repo: &UsageRepository) -> Result<(), Error> {
        for artist in repo.unknown_artists(SEARCHES_PER_RUN, SEARCHES_PER_DAY).await? {
            let query = [("part", "id"), ("type", "channel"), ("maxResults", "25"), ("q", artist.as_str())];
            let ids: Vec<String> = self.get::<Hit>("search", &query).await?.into_iter().map(|h| h.id.channel_id).collect();
            let items = if ids.is_empty() { Vec::new() } else { self.list("channels", CHANNEL_PARTS, &ids).await? };
            let now = now_ms();
            let (channel_id, guess) = match named(&artist, items) {
                None => (String::new(), 0),
                Some((item, guess)) => {
                    let id = item.id.clone();
                    repo.insert("youtube_channels", &[channel(&id, item, now)]).await?;
                    (id, guess.into())
                }
            };
            repo.insert("youtube_artists", &[Artist { artist, channel_id, guess, searched_at: now }]).await?;
        }
        Ok(())
    }

    /// Looks up everything missing or due; returns how many videos and channels were saved.
    pub async fn enrich(&self, repo: &UsageRepository) -> Result<(usize, usize), Error> {
        let now = now_ms();
        let ids = repo.new_youtube_videos().await?;
        for page in ids.chunks(PAGE) {
            let items = self.list("videos", "snippet,contentDetails,statistics", page).await?;
            repo.insert("youtube_videos", &videos(page, items, now)).await?;
        }
        let due = repo.due_youtube_channels().await?;
        let mut saved = 0;
        for page in due.chunks(PAGE) {
            let items = self.list("channels", CHANNEL_PARTS, page).await?;
            let rows: Vec<Channel> = by_id(page, items).map(|(id, i)| channel(id, i, now)).collect();
            repo.insert("youtube_channels", &rows).await?;
            saved += rows.len();
        }
        Ok((ids.len(), saved))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_videos_get_an_empty_row() {
        let list: List<Item> = serde_json::from_str(
            r#"{"items":[{"id":"aaaaaaaaaaa","snippet":{"channelId":"UC1","title":"T","publishedAt":"2020-01-02T03:04:05Z"},
                "contentDetails":{"duration":"PT3M"},"statistics":{"viewCount":"12"}}]}"#,
        )
        .unwrap();
        let rows = videos(&["bbbbbbbbbbb".into(), "aaaaaaaaaaa".into()], list.items, 7);
        assert_eq!((rows[0].video_id.as_str(), rows[0].channel_id.as_str()), ("bbbbbbbbbbb", ""));
        assert_eq!((rows[1].channel_id.as_str(), rows[1].views, rows[1].likes), ("UC1", 12, 0));
    }

    #[test]
    fn the_biggest_channel_of_the_name_wins() {
        let items = || {
            serde_json::from_str::<List<Item>>(
                r#"{"items":[{"id":"a","snippet":{"title":"Name"},"statistics":{"subscriberCount":"5"}},
                    {"id":"b","snippet":{"title":"name "},"statistics":{"subscriberCount":"50"}},
                    {"id":"c","snippet":{"title":"Name Two"},"statistics":{"subscriberCount":"500"}}]}"#,
            )
            .unwrap()
            .items
        };
        let best = |n: &str| named(n, items()).map(|(i, guess)| (i.id, guess));
        assert_eq!(best("Name"), Some(("b".into(), true)));
        assert_eq!(best("Name Two"), Some(("c".into(), false)));
        assert_eq!(best("Nam"), None);
    }

    #[test]
    fn empty_answer_has_no_items() {
        assert!(serde_json::from_str::<List<Item>>("{}").unwrap().items.is_empty());
    }

    #[test]
    fn reads_a_channel() {
        let i: Item = serde_json::from_str(
            r#"{"id":"UC1","snippet":{"title":"T","customUrl":"@t","country":"US","thumbnails":{"high":{"url":"https://x/y"}}},
                "statistics":{"subscriberCount":"6730000","hiddenSubscriberCount":false,"videoCount":"851"},
                "topicDetails":{"topicCategories":["https://en.wikipedia.org/wiki/Rock_music"]},
                "brandingSettings":{"channel":{"keywords":"a b"}}}"#,
        )
        .unwrap();
        let c = channel("UC1", i, 7);
        assert_eq!(c.url, "https://www.youtube.com/channel/UC1");
        assert_eq!((c.handle.as_str(), c.subscribers, c.videos, c.keywords.as_str()), ("@t", 6_730_000, 851, "a b"));
        assert_eq!(c.topics, ["Rock music"]);
    }
}
