use chrono::{Datelike, Local, Timelike};

fn hour_to_word(hour: u32) -> &'static str {
    match hour {
        1 | 13 => "ONE",
        2 | 14 => "TWO",
        3 | 15 => "THREE",
        4 | 16 => "FOUR",
        5 | 17 => "FIVE",
        6 | 18 => "SIX",
        7 | 19 => "SEVEN",
        8 | 20 => "EIGHT",
        9 | 21 => "NINE",
        10 | 22 => "TEN",
        11 | 23 => "ELEVEN",
        12 | 0 => "TWELVE",
        _ => "UNKNOWN",         // should never happen
    }
}

fn get_time_part(now: &chrono::DateTime<Local>) -> String {
    let minute = now.minute();
    let hour = now.hour12().1;
    let next_hour = if hour == 12 { 1 } else { hour + 1 };
    let hour = hour_to_word(hour);
    let next_hour = hour_to_word(next_hour);
    let fuzzy_time = match minute {
        0..=2 => format!("{} O'CLOCK", hour),
        3..=7 => format!("FIVE past {}", hour),
        8..=12 => format!("TEN past {}", hour),
        13..=17 => format!("QUARTER past {}", hour),
        18..=22 => format!("TWENTY past {}", hour),
        23..=27 => format!("TWENTY FIVE past {}", hour),
        28..=32 => format!("HALF past {}", hour),
        33..=37 => format!("TWENTY FIVE to {}", next_hour),
        38..=42 => format!("TWENTY to {}", next_hour),
        43..=47 => format!("QUARTER to {}", next_hour),
        48..=52 => format!("TEN to {}", next_hour),
        53..=57 => format!("FIVE to {}", next_hour),
        58..=59 => format!("{} O'CLOCK", next_hour),
        _ => "".to_string(),    // should never happen
    };
    format!("{}", fuzzy_time)
}

fn get_date_part(now: &chrono::DateTime<Local>) -> String {
    let weekday = now.weekday().to_string().to_uppercase();
    let month = match now.month() {
        1 => "JAN",
        2 => "FEB",
        3 => "MAR", 
        4 => "APR", 
        5 => "MAY", 
        6 => "JUNE",
        7 => "JULY", 
        8 => "AUG", 
        9 => "SEPT", 
        10 => "OCT", 
        11 => "NOV", 
        12 => "DEC",
        _ => "",                // should never happen
    };
    let day = now.day();
    format!("{} {} {},", weekday, month, day)
}

pub fn get_fuzzy_date() -> String {
    let now = Local::now();
    let date_part = get_date_part(&now);
    let time_part = get_time_part(&now);
    format!("{} {}", date_part, time_part)
}