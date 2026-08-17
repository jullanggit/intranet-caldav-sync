#![allow(non_snake_case)]

use serde::{self, Deserialize, Serialize};

#[derive(Debug, Clone, Default)]
pub struct Week {
    pub days: [Day; 5],
}

#[derive(Debug, Clone, Default)]
pub struct Day {
    pub date: Date,
    pub start_time: Time,
    pub lessons: Vec<Lesson>,
}

impl Day {
    pub fn lesson(&self, idx: usize) -> Option<&Lesson> {
        if idx < self.lessons.len() {
            Some(&self.lessons[idx])
        } else {
            None
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JsonLesson {
    className: String,
    courseName: Option<String>,
    lessonDate: String,
    lessonStart: String,
    lessonEnd: String,
    teacherAcronym: String,
    teacherFullName: Vec<String>,
    student: Vec<Student>,
    subjectName: Option<String>,
    timetableEntryTypeShort: String,
    title: String,
    halfClassLesson: Option<String>,
    hasExam: bool,
    roomName: String,
}

#[derive(Debug, Clone)]
pub struct Lesson {
    pub class: String,
    pub lesson_name: Option<String>,
    pub date: Date,
    pub start: Time,
    pub end: Time,
    pub teacher_acronym: String,
    pub teacher_full_name: Vec<String>,
    pub student: Vec<Student>,
    pub subject_name: Option<String>,
    pub entry_type: TTES,
    pub title: String,
    pub exam: bool,
    pub half_class: bool,
    pub room_name: String,
}

impl Lesson {
    pub fn to_string(&self) -> String {
        let name = if let Some(name) = self.lesson_name.clone() {
            name
        } else {
            self.title.clone()
        };

        format!(
            "{} {} {}",
            name,
            self.room_name,
            if self.exam { "(!)" } else { "" }
        )
    }

    pub fn from_jsonLesson(lesson: JsonLesson) -> Self {
        let class = lesson.className;
        let lesson_name = lesson.courseName;
        let date = Date::from_string(lesson.lessonDate);
        let start = Time::from_string(lesson.lessonStart);
        let end = Time::from_string(lesson.lessonEnd);
        let teacher_acronym = lesson.teacherAcronym;
        let teacher_full_name = lesson.teacherFullName;
        let student = lesson.student;
        let subject_name = lesson.subjectName;
        let entry_type = TTES::from_string(lesson.timetableEntryTypeShort);
        let title = lesson.title;
        let exam = lesson.hasExam;
        let half_class = lesson.halfClassLesson.is_some();
        let room_name = lesson.roomName;

        Self {
            class,
            lesson_name,
            date,
            start,
            end,
            teacher_acronym,
            teacher_full_name,
            student,
            subject_name,
            entry_type,
            title,
            exam,
            half_class,
            room_name,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TTES {
    Cancel,
    Lesson,
    Iu,
    Add,
    Mod,
    RoomChange,
    Unknown,
}

impl TTES {
    pub fn get_color(&self) -> usize {
        match self {
            Self::Cancel => 31,
            Self::Lesson => 0,
            Self::Iu => 0,
            Self::Add => 33,
            Self::Mod => 33,
            Self::RoomChange => 33,
            Self::Unknown => 34,
        }
    }
    pub fn from_string(ttes: String) -> Self {
        match ttes.as_str() {
            "cancel" => Self::Cancel,
            "lesson" => Self::Lesson,
            "iudefinitive" => Self::Iu,
            "add" => Self::Add,
            "modlesson" => Self::Mod,
            "rmchg" => Self::RoomChange,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Default)]
pub struct Time {
    pub hour: u8,
    pub minute: u8,
}

impl Ord for Time {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.hour != other.hour {
            self.hour.cmp(&other.hour)
        } else {
            self.minute.cmp(&other.minute)
        }
    }
}

impl std::ops::AddAssign for Time {
    fn add_assign(&mut self, rhs: Self) {
        self.minute += rhs.minute;
        self.hour += self.minute / 60;
        self.hour %= 24;
        self.minute %= 60;
    }
}

impl std::ops::SubAssign for Time {
    fn sub_assign(&mut self, rhs: Self) {
        if self.hour > rhs.hour {
            self.hour -= rhs.hour
        } else {
            self.hour = 24 - (rhs.hour - self.hour);
        };

        if self.minute > rhs.minute {
            self.minute -= rhs.minute;
        } else {
            self.hour -= 1;
            self.minute = 60 - (rhs.minute - self.minute)
        };
    }
}

impl std::ops::Add for Time {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let mut minute = self.minute + rhs.minute;
        let hour = (self.hour + (minute / 60)) % 24;
        minute %= 60;
        Self { hour, minute }
    }
}

impl std::ops::Sub for Time {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        let mut hour = if self.hour > rhs.hour {
            self.hour - rhs.hour
        } else {
            24 - (rhs.hour - self.hour)
        };

        let minute = if self.minute > rhs.minute {
            self.minute - rhs.minute
        } else {
            hour -= 1;
            60 - (rhs.minute - self.minute)
        };

        Self { hour, minute }
    }
}

impl Time {
    pub fn from_string(time: String) -> Self {
        //hh:mm:ss

        let mut times = time.split(':');
        let (hour, minute) = (
            times
                .next()
                .expect("Expected valid time")
                .parse::<u8>()
                .expect("Expected valid time"),
            times
                .next()
                .expect("Expected valid time")
                .parse::<u8>()
                .expect("Expected valid time"),
        );

        Self { hour, minute }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Date {
    pub day: u8,
    pub month: u8,
    pub year: u32,
}

impl Date {
    pub fn to_string(&self) -> String {
        format!("{}-{}-{}", self.day, self.month, self.year)
    }
    pub fn from_string(date: String) -> Self {
        // yyyy-mm-dd
        let mut dates = date.split('-');

        let (year, month, day) = (
            dates
                .next()
                .expect("Expected valid date")
                .parse::<u32>()
                .expect("Expected valid date"),
            dates
                .next()
                .expect("Expected valid date")
                .parse::<u8>()
                .expect("Expected valid date"),
            dates
                .next()
                .expect("Expected valid date")
                .parse::<u8>()
                .expect("Expected valid date"),
        );

        Self { year, month, day }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Student {
    studentId: u32,
    studentName: String,
}
