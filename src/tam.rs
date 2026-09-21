use crate::config::{self, Config};
use anyhow::{bail, Context, Result};
use chrono::{NaiveDate, NaiveTime};
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Lesson {
    pub id: u64,
    pub subject: Option<String>,
    pub room: String,
    pub date: NaiveDate,
    pub start: NaiveTime,
    pub end: NaiveTime,
    pub teacher: Vec<String>,
    pub title: String,
    pub entry_type: String,
    pub exam: bool,
    pub cancelled: bool,
    pub homework: bool,
}

#[derive(Debug, Deserialize)]
struct ApiLesson {
    id: u64,
    #[serde(default, rename = "subjectName")]
    subject_name: Option<String>,
    #[serde(default, rename = "roomName")]
    room_name: String,
    #[serde(rename = "lessonDate")]
    lesson_date: String,
    #[serde(rename = "lessonStart")]
    lesson_start: String,
    #[serde(rename = "lessonEnd")]
    lesson_end: String,
    #[serde(default, rename = "teacherFullName")]
    teacher_full_name: Vec<String>,
    #[serde(default)]
    title: String,
    #[serde(default, rename = "timetableEntryTypeShort")]
    timetable_entry_type_short: String,
    #[serde(default, rename = "hasExam")]
    has_exam: bool,
    #[serde(default, rename = "isExamLesson")]
    is_exam_lesson: bool,
    #[serde(default, rename = "timetableEntryType")]
    timetable_entry_type: String,
    #[serde(default, rename = "hasHomework")]
    has_homework: bool,
}

pub struct Tam {
    client: Client,
    config: Config,
}

impl Tam {
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .cookie_store(true)
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
            config: config.clone(),
        })
    }

    async fn login(&self, password: &str) -> Result<()> {
        let base = "https://intranet.tam.ch/";
        let response = self.client.get(base).send().await?.error_for_status()?;
        let html = response.text().await?;
        let selector = Selector::parse("input[name=\"hash\"]")
            .map_err(|error| anyhow::anyhow!("invalid login selector: {error:?}"))?;
        let document = Html::parse_document(&html);
        let hash = document
            .select(&selector)
            .next()
            .and_then(|e| e.value().attr("value"))
            .context("login hash not found")?;
        let response = self
            .client
            .post(base)
            .form(&[
                ("loginuser", self.config.tam.username.as_str()),
                ("loginpassword", password),
                ("loginschool", self.config.tam.school.as_str()),
                ("hash", hash),
            ])
            .send()
            .await?
            .error_for_status()?;
        let body = response.text().await?;
        if !body.contains("csrfToken='") {
            bail!("login failed: CSRF token was not returned");
        }
        Ok(())
    }

    pub async fn fetch(&self) -> Result<Vec<Lesson>> {
        let password = config::secret(&self.config.tam.password_file)?;
        self.login(&password).await.context("logging in to TAM")?;
        let (start, end) = config::range(&self.config.sync)?;
        let mut all = Vec::new();
        let mut cursor = start;
        while cursor < end {
            let week_end = (cursor + chrono::Duration::weeks(1).num_milliseconds()).min(end);
            let start_string = cursor.to_string();
            let end_string = week_end.to_string();
            let mut request = vec![
                (
                    "MIME Type",
                    "application/x-www-form-urlencoded; charset=UTF-8",
                ),
                ("startDate", start_string.as_str()),
                ("endDate", end_string.as_str()),
                ("holidaysOnly", "0"),
            ];
            let id = self.config.tam.id.as_str();
            request.push(if self.config.tam.id_is_class {
                ("classId[]", id)
            } else {
                ("studentId[]", id)
            });
            let response = self
                .client
                .post("https://intranet.tam.ch/krm/timetable/ajax-get-timetable")
                .header(
                    "Referer",
                    "https://intranet.tam.ch/krm/timetable/classbook/period/",
                )
                .header("X-Requested-With", "XMLHttpRequest")
                .form(&request)
                .send()
                .await?
                .error_for_status()?;
            let payload: serde_json::Value = response.json().await?;
            let entries: Vec<ApiLesson> = serde_json::from_value(
                payload
                    .get("data")
                    .cloned()
                    .context("TAM response did not contain data")?,
            )?;
            all.extend(
                entries
                    .into_iter()
                    .map(Lesson::try_from)
                    .collect::<Result<Vec<_>>>()?,
            );
            cursor = week_end;
        }
        Ok(all)
    }
}

impl TryFrom<ApiLesson> for Lesson {
    type Error = anyhow::Error;
    fn try_from(x: ApiLesson) -> Result<Self> {
        let date = NaiveDate::parse_from_str(&x.lesson_date, "%Y-%m-%d")
            .with_context(|| format!("invalid lesson date {}", x.lesson_date))?;
        let parse_time = |s: &str| {
            NaiveTime::parse_from_str(s, "%H:%M:%S")
                .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M"))
                .with_context(|| format!("invalid lesson time {s}"))
        };
        Ok(Self {
            id: x.id,
            subject: x.subject_name,
            room: x.room_name,
            date,
            start: parse_time(&x.lesson_start)?,
            end: parse_time(&x.lesson_end)?,
            teacher: x.teacher_full_name,
            title: x.title,
            entry_type: if x.timetable_entry_type_short.is_empty() {
                x.timetable_entry_type
            } else {
                x.timetable_entry_type_short.clone()
            },
            exam: x.has_exam || x.is_exam_lesson,
            cancelled: x.timetable_entry_type_short == "cancel",
            homework: x.has_homework,
        })
    }
}
