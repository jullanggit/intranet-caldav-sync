#![allow(unused)]

mod school;
use comfy_table::Table;
use school::*;

use chrono::{Datelike, Duration, Utc};
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::Value;
use std::{error::Error, fs};

const PEAK_PRESET: &str = "││──│══││─┼├┤┬┴┌┐└┘";
const SCHOOL_START: Time = Time {
    hour: 7,
    minute: 45,
};

const LESSON_LENGTH: Time = Time {
    hour: 0,
    minute: 45,
};

const SHORT_BREAK_LENGHT: Time = Time {
    hour: 0,
    minute: 10,
};
const LONG_BREAK_LENGHT: Time = Time {
    hour: 0,
    minute: 15,
};
const LONG_BREAK_TIME: Time = Time {
    hour: 10,
    minute: 20,
};

const DAYS_NAMES: [&str; 5] = ["Mon", "Tue", "Wed", "Thu", "Fri"];

/// Logs into the TAM intranet and extracts the CSRF token.
async fn login_to_tam(
    client: &Client,
    username: &str,
    password: &str,
    school: &str,
) -> Result<String, Box<dyn Error>> {
    let url = "https://intranet.tam.ch/";

    // 1. Get the initial page to extract the hidden hash value
    let response_text = client.get(url).send().await?.text().await?;
    let document = Html::parse_document(&response_text);
    let selector = Selector::parse("input[name=\"hash\"]").unwrap();

    let hash = document
        .select(&selector)
        .next()
        .and_then(|el| el.value().attr("value"))
        .ok_or("Could not find hash input in HTML")?;

    // 2. Submit the login form
    let params = [
        ("loginuser", username),
        ("loginpassword", password),
        ("loginschool", school),
        ("hash", hash),
    ];
    let response_text = client.post(url).form(&params).send().await?.text().await?;
    // 3. Extract the CSRF token from the response text
    let csrf_token = response_text
        .split("csrfToken='")
        .nth(1)
        .ok_or("csrfToken start not found")?
        .split("';")
        .next()
        .ok_or("csrfToken end not found")?
        .to_string();

    Ok(csrf_token)
}

/// Fetches the timetable data for a specific date range.
async fn get_timetable_from_tam(
    client: &Client,
    id: &str,
    class: bool, // if it's a class or a studentId
    start: i64,
    end: i64,
) -> Result<Value, Box<dyn Error>> {
    let url = "https://intranet.tam.ch/krm/timetable/ajax-get-timetable";

    let start_str = start.to_string();
    let end_str = end.to_string();

    let params = [
        (
            "MIME Type",
            "application/x-www-form-urlencoded; charset=UTF-8",
        ),
        ("startDate", &start_str),
        ("endDate", &end_str),
        if class {
            ("classId", id)
        } else {
            ("studentId", id)
        },
        // ("classId[]", class_id), // Note: if this needs to be an array of IDs, Reqwest supports iterating over tuples.
        ("holidaysOnly", "0"),
    ];

    let response = client
        .post(url)
        .header(
            "Referer",
            "https://intranet.tam.ch/krm/timetable/classbook/period/",
        )
        .header("X-Requested-With", "XMLHttpRequest")
        .form(&params)
        .send()
        .await?;

    println!("response status: {}", response.status());

    let mut json_data: Value = response.json().await?;

    // Return the ['data'] payload specifically, matching Python behavior
    let data = json_data
        .get_mut("data")
        .map(|v| v.take())
        .unwrap_or(Value::Null);

    Ok(data)
}

/// Main orchestration function. Iterates through weeks, fetches data, and appends to queue.
pub async fn tam(
    username: &str,
    password: &str,
    id: &str,
    class: bool, // if it's a classId or StudentId
    mut start: i64,
    end: i64,
    queue: &mut Vec<Value>,
) -> Result<(), Box<dyn Error>> {
    // Build an HTTP client with an automatic cookie store enabled
    let client = Client::builder().cookie_store(true).build()?;

    let _csrf = login_to_tam(&client, username, password, "krm").await?;

    // Calculate the number of weeks (1 week = 604,800,000 ms)
    let dif = end - start;
    let weeks = (dif as f64 / 604_800_000.0).round() as i64;

    println!("{} week(s)", weeks);

    for i in 0..weeks {
        let current_end = start + 604_800_000;

        match get_timetable_from_tam(&client, id, class, start, current_end).await {
            Ok(json_payload) => {
                queue.push(json_payload);
                println!("success tam nr {}", i);
            }
            Err(e) => {
                eprintln!("failed tam nr {}: {}", i, e);
            }
        }

        start += 604_800_000;

        // Sleep for 5 seconds to avoid overwhelming the server
        if i != weeks - 1 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    Ok(())
}

fn get_week_bounds_ms(n_weeks_from_now: i64, n_weeks: i64) -> (i64, i64) {
    // 1. Get the current UTC time
    let now = Utc::now();

    // 2. Find how many days have passed since Monday (0 = Monday, 6 = Sunday)
    let days_since_monday = now.weekday().num_days_from_monday() as i64;

    // 3. Subtract those days to get the date of the current week's Monday
    let current_week_monday = now.date_naive() - Duration::days(days_since_monday);

    // 4. Truncate to midnight (00:00:00) and convert back to UTC DateTime
    let start_of_current_week = current_week_monday
        .and_hms_opt(0, 0, 0)
        .expect("Valid time 00:00:00")
        .and_utc();

    // 5. Apply the week offset (+n weeks)
    let target_week_start = start_of_current_week + Duration::weeks(n_weeks_from_now);

    // 6. The end of the week is exactly 7 days later, minus 1 millisecond
    let target_week_end =
        target_week_start + Duration::weeks(n_weeks) - Duration::try_milliseconds(1).unwrap();

    // 7. Return the Unix timestamps in milliseconds
    (
        target_week_start.timestamp_millis(),
        target_week_end.timestamp_millis(),
    )
}

async fn load_json(week_idx: i64, num_weeks: i64, id: &str, class: bool) -> String {
    let mut queue: Vec<Value> = Vec::new();

    let (start_ms, end_ms) = get_week_bounds_ms(week_idx, num_weeks);

    if let Err(e) = tam(
        "ur username",
        "ur password",
        id,
        class, // if it's a classId
        start_ms,
        end_ms,
        &mut queue,
    )
    .await
    {
        eprintln!("Error running TAM scraper: {}", e);
    }

    let mut json_string = String::new();
    json_string.push_str("[");

    for week in queue {
        let week_str = week.to_string();
        json_string.push_str(&format!("{week_str},"));
    }

    json_string.pop();
    json_string.push_str("]");

    fs::write("output.json", &json_string);

    json_string
}

#[tokio::main]
async fn main() {
    // 2098: 3a, might have changed now that we're 4a, hopefully not though
    let json = load_json(0, 1, "2098", true).await;

    let fetched =
        serde_json::from_str::<Vec<Vec<JsonLesson>>>(&json).expect("Failed to parse timetable");

    let mut weeks: Vec<Week> = Vec::new();

    // --- FIX 1: Robust Parsing ---
    for jsonweek in fetched {
        let mut week = Week::default();

        for lesson in jsonweek {
            let parsed = Lesson::from_jsonLesson(lesson);

            if let Some(naive_date) = chrono::NaiveDate::from_ymd_opt(
                parsed.date.year as i32,
                parsed.date.month as u32,
                parsed.date.day as u32,
            ) {
                let day_of_week = naive_date.weekday().num_days_from_monday() as usize;

                if day_of_week < 5 {
                    // Only process Monday - Friday
                    let target_day = &mut week.days[day_of_week];

                    if target_day.lessons.is_empty() {
                        // First lesson for this day slot; claim the date!
                        target_day.date = parsed.date;
                        target_day.start_time = parsed.start;
                        target_day.lessons.push(parsed);
                    } else {
                        // Only add the lesson if it belongs to the EXACT same calendar date.
                        // (Assuming your Date struct implements PartialEq. If not, compare year/month/day manually)
                        if target_day.date.day == parsed.date.day
                            && target_day.date.month == parsed.date.month
                            && target_day.date.year == parsed.date.year
                        {
                            target_day.lessons.push(parsed);
                        }
                    }
                }
            }
        }
        weeks.push(week);
    }

    // --- FIX 2 & 3: Table Display and Half-Classes ---
    for week in &weeks {
        let mut table = Table::new();
        table.load_preset(PEAK_PRESET);
        table.set_header(&DAYS_NAMES);

        // Instead of guessing breaks, gather all unique start times present in the week
        let mut all_times: Vec<Time> = Vec::new();
        for day in &week.days {
            for l in &day.lessons {
                if !all_times.contains(&l.start) {
                    all_times.push(l.start);
                }
            }
        }
        all_times.sort(); // Chronologically sorts the rows

        // Build the table row by row based on actual lesson times
        for current_time in all_times {
            let mut row_cells = vec![String::new(); 5]; // Exactly 5 columns initialized empty

            for (day_idx, day) in week.days.iter().enumerate() {
                let mut lessons_this_slot = Vec::new();

                // Find any lessons occurring on this specific day at this specific time
                for l in &day.lessons {
                    if l.start == current_time {
                        lessons_this_slot.push(format!(
                            "\u{001b}[{}m-{}\u{001b}[0m",
                            l.entry_type.get_color(),
                            l.to_string()
                        ));
                    }
                }

                // If we found lessons, add them to the correct cell.
                // If there are two half-classes starting at the exact same time,
                // the `.join` method automatically concatenates them with " | "
                if !lessons_this_slot.is_empty() {
                    row_cells[day_idx] = lessons_this_slot.join(" | ");
                }
            }

            table.add_row(row_cells);
        }

        println!("{table}");
    }
}
