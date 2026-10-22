use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrollEvent {
    pub time_seconds: i32,
    pub line_index: usize, // 0-based line index in the buffer
}

#[derive(Debug, Default, Clone)]
pub struct ParsedSong {
    pub bpm: Option<u32>,
    pub track: Option<String>,
    pub total_time_seconds: Option<i32>,
    pub scroll_events: Vec<ScrollEvent>,
}

/// Converts a time string 'M:SS' or 'SS' to seconds.
pub fn parse_time_to_seconds(time_str: &str) -> i32 {
    let s = time_str.trim();
    if s.is_empty() {
        return 0;
    }
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        1 => parts[0].parse::<i32>().unwrap_or(0),
        2 => {
            let mins = parts[0].parse::<i32>().unwrap_or(0);
            let secs = parts[1].parse::<i32>().unwrap_or(0);
            mins * 60 + secs
        }
        _ => 0,
    }
}

/// Converts seconds to a 'M:SS' formatted string.
pub fn format_seconds_to_time(seconds: i32) -> String {
    let s = seconds.max(0);
    let mins = s / 60;
    let secs = s % 60;
    format!("{}:{:02}", mins, secs)
}

/// Parses metadata and scroll markers from text content.
pub fn parse_song_content(content: &str) -> ParsedSong {
    let bpm_re = Regex::new(r"BPM:\s*(\d+)").unwrap();
    let track_re = Regex::new(r"TRACK:\s*(.+)").unwrap();
    let time_re = Regex::new(r"TIME:\s*(.+)").unwrap();
    let marker_re = Regex::new(r"#:\s*(.*)").unwrap();

    let bpm = bpm_re
        .captures(content)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().trim().parse::<u32>().ok());

    let track = track_re
        .captures(content)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string());

    let total_time_seconds = time_re
        .captures(content)
        .and_then(|c| c.get(1))
        .map(|m| parse_time_to_seconds(m.as_str().trim()));

    let mut scroll_events = Vec::new();
    for (line_idx, line) in content.lines().enumerate() {
        if let Some(caps) = marker_re.captures(line) {
            if let Some(m) = caps.get(1) {
                let time_str = m.as_str().trim();
                if time_str.eq_ignore_ascii_case("f") {
                    continue;
                }
                let time_sec = parse_time_to_seconds(time_str);
                if time_sec >= 0 {
                    scroll_events.push(ScrollEvent {
                        time_seconds: time_sec,
                        line_index: line_idx,
                    });
                }
            }
        }
    }
    scroll_events.sort_by_key(|e| e.time_seconds);

    ParsedSong {
        bpm,
        track,
        total_time_seconds,
        scroll_events,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_parsing() {
        assert_eq!(parse_time_to_seconds("0:00"), 0);
        assert_eq!(parse_time_to_seconds("0:11"), 11);
        assert_eq!(parse_time_to_seconds("3:49"), 229);
        assert_eq!(parse_time_to_seconds("600"), 600);
        assert_eq!(parse_time_to_seconds(""), 0);
        assert_eq!(parse_time_to_seconds("invalid"), 0);
    }

    #[test]
    fn test_time_formatting() {
        assert_eq!(format_seconds_to_time(0), "0:00");
        assert_eq!(format_seconds_to_time(11), "0:11");
        assert_eq!(format_seconds_to_time(229), "3:49");
        assert_eq!(format_seconds_to_time(600), "10:00");
    }

    #[test]
    fn test_song_parsing() {
        let content = "BPM: 76\nTIME: 3:49\nTRACK: Free.mp3\n#: 0:00\n[Intro]\n#: 0:11\n#: f\n";
        let parsed = parse_song_content(content);
        assert_eq!(parsed.bpm, Some(76));
        assert_eq!(parsed.total_time_seconds, Some(229));
        assert_eq!(parsed.track, Some("Free.mp3".to_string()));
        assert_eq!(parsed.scroll_events.len(), 2);
        assert_eq!(parsed.scroll_events[0].time_seconds, 0);
        assert_eq!(parsed.scroll_events[0].line_index, 3);
        assert_eq!(parsed.scroll_events[1].time_seconds, 11);
        assert_eq!(parsed.scroll_events[1].line_index, 5);
    }
}
