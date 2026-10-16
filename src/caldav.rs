use crate::config::Config;
use crate::{config, tam::Lesson};
use anyhow::{Context, Result, bail};
use chrono::{NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Zurich;
use icalendar::{
    Calendar, CalendarDateTime, Component, DatePerhapsTime, Event, EventLike, EventStatus, Property,
};
use reqwest::{Client, Method};
use std::collections::HashSet;
use std::str::FromStr;

pub struct CalDav {
    client: Client,
    url: String,
    username: String,
    password: String,
    oldest: i64,
}

impl CalDav {
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
            url: config.caldav.calendar_url.trim_end_matches('/').to_owned(),
            username: config.caldav.username.clone(),
            password: config::secret(&config.caldav.password_file)?,
            oldest: config::range(&config.sync)?.0,
        })
    }

    pub async fn sync(&self, lessons: &[Lesson]) -> Result<()> {
        let desired: HashSet<String> = lessons
            .iter()
            .map(|lesson| format!("tam-{}.ics", lesson.id))
            .collect();
        for lesson in lessons {
            let uid = format!("tam-{}.ics", lesson.id);
            let body = event_calendar(lesson)?.to_string();
            let url = format!("{}/{}", self.url, uid);
            self.client
                .request(Method::PUT, url)
                .basic_auth(&self.username, Some(&self.password))
                .header("Content-Type", "text/calendar; charset=utf-8")
                .body(body)
                .send()
                .await?
                .error_for_status()
                .with_context(|| format!("uploading event {}", lesson.id))?;
        }
        for href in self.list_event_urls().await? {
            let filename = href.rsplit('/').next().unwrap_or_default();
            if filename.starts_with("tam-")
                && filename.ends_with(".ics")
                && !desired.contains(filename)
            {
                if let Some(start) = self.event_start(&href).await?
                    && start < self.oldest
                {
                    continue;
                }
                self.client
                    .request(Method::DELETE, href.clone())
                    .basic_auth(&self.username, Some(&self.password))
                    .send()
                    .await?
                    .error_for_status()
                    .with_context(|| format!("deleting stale event {href}"))?;
            }
        }
        Ok(())
    }

    async fn event_start(&self, href: &str) -> Result<Option<i64>> {
        let body = self
            .client
            .get(href)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await?
            .error_for_status()
            .with_context(|| format!("fetching event {href}"))?
            .text()
            .await?;
        let calendar = Calendar::from_str(&body)
            .map_err(|error| anyhow::anyhow!("parsing event {href}: {error}"))?;
        let start = calendar
            .components
            .iter()
            .find_map(|component| component.as_event().and_then(|event| event.get_start()));
        Ok(start.as_ref().and_then(start_millis))
    }

    async fn list_event_urls(&self) -> Result<Vec<String>> {
        let response = self
            .client
            .request(Method::from_bytes(b"PROPFIND")?, &self.url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Depth", "1")
            .header("Content-Type", "application/xml")
            .body("<propfind xmlns=\"DAV:\"><prop><resourcetype/></prop></propfind>")
            .send()
            .await?
            .error_for_status()
            .context("listing CalDAV collection")?;
        let body = response.text().await?;
        let mut urls = Vec::new();
        for open in ["<d:href>", "<D:href>", "<href>"] {
            let close = open.replace('<', "</");
            for part in body.split(open).skip(1) {
                if let Some(href) = part.split(&close).next() {
                    if href.ends_with(".ics") {
                        urls.push(if href.starts_with("http") {
                            href.to_owned()
                        } else {
                            format!("{}{}", self.url.trim_end_matches('/'), href)
                        });
                    }
                }
            }
        }
        Ok(urls)
    }
}

fn start_millis(start: &DatePerhapsTime) -> Option<i64> {
    match start {
        DatePerhapsTime::Date(date) => {
            Some(date.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis())
        }
        DatePerhapsTime::DateTime(CalendarDateTime::Utc(datetime)) => {
            Some(datetime.timestamp_millis())
        }
        DatePerhapsTime::DateTime(CalendarDateTime::Floating(datetime)) => {
            Some(datetime.and_utc().timestamp_millis())
        }
        DatePerhapsTime::DateTime(CalendarDateTime::WithTimezone { date_time, .. }) => {
            Some(date_time.and_utc().timestamp_millis())
        }
    }
}

fn event_calendar(lesson: &Lesson) -> Result<Calendar> {
    let start = Zurich
        .from_local_datetime(&NaiveDateTime::new(lesson.date, lesson.start))
        .single()
        .context("lesson start falls in an ambiguous or nonexistent local time")?;
    let end = Zurich
        .from_local_datetime(&NaiveDateTime::new(lesson.date, lesson.end))
        .single()
        .context("lesson end falls in an ambiguous or nonexistent local time")?;
    if end <= start {
        bail!("lesson {} ends before or at its start", lesson.id);
    }
    let summary = match (
        lesson.subject.as_deref(),
        (!lesson.room.is_empty()).then_some(lesson.room.as_str()),
    ) {
        (Some(subject), Some(room)) => format!("{subject} - {room}"),
        (Some(subject), None) => subject.to_owned(),
        (None, Some(room)) => room.to_owned(),
        (None, None) => lesson.title.clone(),
    };
    let summary = match (lesson.cancelled, lesson.exam, lesson.homework) {
        (true, true, false) => format!("CANCELLED EXAM: {summary}"),
        (true, false, false) => format!("CANCELLED: {summary}"),
        (false, true, false) => format!("EXAM: {summary}"),
        (false, false, false) => summary,
        (true, true, true) => format!("HW CANCELLED EXAM: {summary}"),
        (true, false, true) => format!("HW CANCELLED: {summary}"),
        (false, true, true) => format!("HW EXAM: {summary}"),
        (false, false, true) => format!("HW: {summary}"),
    };
    let description = [
        (!lesson.teacher.is_empty()).then(|| format!("Teacher: {}", lesson.teacher.join(", "))),
        (!lesson.title.is_empty()).then(|| format!("Title: {}", lesson.title)),
        (!lesson.entry_type.is_empty()).then(|| format!("Type: {}", lesson.entry_type)),
        lesson.exam.then(|| "Exam: yes".to_owned()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n");
    let mut event = Event::new();
    event.uid(&format!("tam-{}", lesson.id));
    event.summary(&summary);
    event.starts(start.with_timezone(&Utc));
    event.ends(end.with_timezone(&Utc));
    if !lesson.room.is_empty() {
        event.location(&lesson.room);
    }
    if !description.is_empty() {
        event.description(&description);
    }
    if lesson.cancelled {
        event.status(EventStatus::Cancelled);
    }
    let color = if lesson.cancelled {
        Some("red")
    } else if lesson.exam {
        Some("orange")
    } else {
        None
    };
    if let Some(color) = color {
        event.append_property(Property::new("COLOR", color));
    }
    Ok(Calendar::from(event.done()))
}
