mod config;
mod tam;

use anyhow::{Context, Result};
use chrono::{NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Zurich;
use clap::Parser;
use config::Config;
use icalendar::{Calendar, Component, Event, EventLike, EventStatus, Property};
use tam::{Lesson, Tam};

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    config: std::path::PathBuf,
    #[arg(short, long, default_value = "timetable.ics")]
    output: std::path::PathBuf,
}

fn build_calendar(lessons: &[Lesson]) -> Result<Calendar> {
    let mut calendar = Calendar::new();
    for lesson in lessons {
        let start = Zurich
            .from_local_datetime(&NaiveDateTime::new(lesson.date, lesson.start))
            .single()
            .context("invalid start time")?;
        let end = Zurich
            .from_local_datetime(&NaiveDateTime::new(lesson.date, lesson.end))
            .single()
            .context("invalid end time")?;

        let summary = match (
            lesson.subject.as_deref(),
            (!lesson.room.is_empty()).then_some(lesson.room.as_str()),
        ) {
            (Some(subject), Some(room)) => format!("{subject} - {room}"),
            (Some(subject), None) => subject.to_owned(),
            (None, Some(room)) => room.to_owned(),
            (None, None) => lesson.title.clone(),
        };

        let summary = match (lesson.cancelled, lesson.exam) {
            (true, true) => format!("CANCELLED EXAM: {summary}"),
            (true, false) => format!("CANCELLED: {summary}"),
            (false, true) => format!("EXAM: {summary}"),
            (false, false) => summary,
        };

        let mut event = Event::new();
        event.uid(&format!("tam-{}", lesson.id));
        event.summary(&summary);
        event.starts(start.with_timezone(&Utc));
        event.ends(end.with_timezone(&Utc));

        if !lesson.room.is_empty() {
            event.location(&lesson.room);
        }
        if lesson.cancelled {
            event.status(EventStatus::Cancelled);
        }
        calendar.push(event.done());
    }
    Ok(calendar)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config_text = tokio::fs::read_to_string(&args.config).await?;
    let config: Config = serde_json::from_str(&config_text)?;
    config.validate()?;

    let tam = Tam::new(&config)?;
    let lessons = tam.fetch().await?;
    if lessons.is_empty() {
        anyhow::bail!("no timetable entries found");
    }

    let cal = build_calendar(&lessons)?;
    tokio::fs::write(&args.output, cal.to_string()).await?;
    println!(
        "Saved {} lessons to {}",
        lessons.len(),
        args.output.display()
    );
    Ok(())
}
