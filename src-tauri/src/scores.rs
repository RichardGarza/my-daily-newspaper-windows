//! The score bug: current Dodgers game while one is live, otherwise the last
//! final. Straight from MLB's public stats API - free, no key, no AI, ~200 ms.
//! (A score is a fact, not an editorial decision. Claude stays out of it.)

use chrono::{DateTime, Duration as ChronoDuration, Local, Utc};
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TeamLine {
    pub abbr: String,
    pub name: String,
    pub score: Option<u32>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GameLine {
    pub game_pk: u64,
    /// "live" | "final" | "preview"
    pub state: String,
    /// "Top 7th", "Final", "Final/10", "Delayed", "7:10 PM" ...
    pub detail: String,
    /// RFC 3339 first pitch.
    pub start: String,
    pub away: TeamLine,
    pub home: TeamLine,
    /// True when our team is the home side.
    pub we_are_home: bool,
    pub url: String,
    /// Where to watch: TV broadcasts for our side plus national ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tv: Vec<String>,
    /// Live games only: the situation on the field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub situation: Option<Situation>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Situation {
    pub balls: u32,
    pub strikes: u32,
    pub outs: u32,
    /// Runner on first, second, third.
    pub bases: [bool; 3],
    pub batter: Option<String>,
    pub pitcher: Option<String>,
    /// ABS (automated ball-strike) challenges left: away, home. Only when the
    /// league reports them for this game.
    pub challenges: Option<[u32; 2]>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScoreBug {
    /// The game to show: live if there is one, else the most recent final.
    pub game: Option<GameLine>,
    /// Next scheduled game, when nothing is live.
    pub next: Option<GameLine>,
}

pub async fn fetch(client: &reqwest::Client, team_id: u32) -> Result<ScoreBug, String> {
    let today = Local::now().date_naive();
    let mut bug = fetch_window(client, team_id, today - ChronoDuration::days(5), today + ChronoDuration::days(5)).await?;
    if let Some(game) = bug.game.as_mut() {
        if game.state == "live" {
            if let Some(sit) = game.situation.as_mut() {
                sit.challenges = fetch_challenges(client, game.game_pk).await;
            }
        }
        return Ok(bug);
    }
    // Off-season or a long break: look further back for the last final.
    let wide = fetch_window(client, team_id, today - ChronoDuration::days(240), today + ChronoDuration::days(5)).await?;
    Ok(ScoreBug { game: wide.game, next: bug.next.or(wide.next) })
}

async fn fetch_window(
    client: &reqwest::Client,
    team_id: u32,
    start: chrono::NaiveDate,
    end: chrono::NaiveDate,
) -> Result<ScoreBug, String> {
    let url = format!(
        "https://statsapi.mlb.com/api/v1/schedule?sportId=1&teamId={team_id}&startDate={start}&endDate={end}&hydrate=team,linescore,broadcasts(all)"
    );
    let resp = client.get(&url).send().await.map_err(|e| format!("scores: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("scores: HTTP {}", resp.status().as_u16()));
    }
    let body = resp.text().await.map_err(|e| format!("scores: {e}"))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("scores: {e}"))?;
    Ok(pick(&v, team_id, Utc::now()))
}

/// ABS challenges remaining (away, home) from the live game feed. The schedule
/// endpoint doesn't carry them. Best effort: None if the feed has no such block.
async fn fetch_challenges(client: &reqwest::Client, game_pk: u64) -> Option<[u32; 2]> {
    let url = format!(
        "https://statsapi.mlb.com/api/v1.1/game/{game_pk}/feed/live?fields=gameData,absChallenges,hasChallenges,away,home,remaining,usedSuccessful,usedFailed"
    );
    let body = client.get(&url).send().await.ok()?.text().await.ok()?;
    let v: Value = serde_json::from_str(&body).ok()?;
    challenges_from_feed(&v)
}

pub fn challenges_from_feed(feed: &Value) -> Option<[u32; 2]> {
    let abs = feed.pointer("/gameData/absChallenges")?;
    if abs.get("hasChallenges").and_then(|b| b.as_bool()) == Some(false) {
        return None;
    }
    let left = |side: &str| abs.pointer(&format!("/{side}/remaining")).and_then(|n| n.as_u64()).map(|n| n as u32);
    Some([left("away")?, left("home")?])
}

/// Choose what the bug shows from a schedule response.
pub fn pick(schedule: &Value, team_id: u32, now: DateTime<Utc>) -> ScoreBug {
    let mut games: Vec<GameLine> = schedule
        .get("dates")
        .and_then(|d| d.as_array())
        .into_iter()
        .flatten()
        .filter_map(|d| d.get("games").and_then(|g| g.as_array()))
        .flatten()
        .filter_map(|g| game_line(g, team_id))
        .collect();
    games.sort_by(|a, b| a.start.cmp(&b.start));

    let live = games.iter().rev().find(|g| g.state == "live").cloned();
    let last_final = games
        .iter()
        .rev()
        .find(|g| g.state == "final" && g.away.score.is_some() && g.home.score.is_some())
        .cloned();
    let next = games
        .iter()
        .find(|g| {
            g.state == "preview"
                && DateTime::parse_from_rfc3339(&g.start)
                    .map(|s| s.with_timezone(&Utc) > now - ChronoDuration::hours(1))
                    .unwrap_or(false)
        })
        .cloned();

    match live {
        Some(g) => ScoreBug { game: Some(g), next: None },
        None => ScoreBug { game: last_final, next },
    }
}

fn game_line(g: &Value, team_id: u32) -> Option<GameLine> {
    let game_pk = g.get("gamePk")?.as_u64()?;
    let start = g.get("gameDate")?.as_str()?.to_string();
    let abstract_state = g.pointer("/status/abstractGameState").and_then(|s| s.as_str()).unwrap_or("");
    let detailed = g.pointer("/status/detailedState").and_then(|s| s.as_str()).unwrap_or("");

    let side = |key: &str| -> Option<(TeamLine, u64)> {
        let t = g.pointer(&format!("/teams/{key}"))?;
        let team = t.get("team")?;
        let id = team.get("id")?.as_u64()?;
        let name = team
            .get("teamName")
            .or_else(|| team.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        let abbr = team
            .get("abbreviation")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| name.chars().take(3).collect::<String>().to_uppercase());
        let score = t.get("score").and_then(|s| s.as_u64()).map(|s| s as u32);
        Some((TeamLine { abbr, name, score }, id))
    };
    let (away, away_id) = side("away")?;
    let (home, home_id) = side("home")?;
    if away_id != team_id as u64 && home_id != team_id as u64 {
        return None;
    }

    let inning = g.pointer("/linescore/currentInning").and_then(|i| i.as_u64());
    let ordinal = g.pointer("/linescore/currentInningOrdinal").and_then(|i| i.as_str());
    let half = g.pointer("/linescore/inningState").and_then(|i| i.as_str());
    let scheduled = g.pointer("/linescore/scheduledInnings").and_then(|i| i.as_u64()).unwrap_or(9);

    let lower = detailed.to_lowercase();
    let never_played = lower.contains("postponed") || lower.contains("cancel");

    let (state, detail) = match abstract_state {
        "Live" => {
            let d = if lower.contains("delay") || lower.contains("suspend") {
                detailed.to_string()
            } else {
                match (half, ordinal) {
                    (Some(h), Some(o)) => format!("{} {o}", short_half(h)),
                    _ if lower.contains("warmup") => "Warmup".to_string(),
                    _ => "Live".to_string(),
                }
            };
            ("live", d)
        }
        "Final" if !never_played => {
            let d = match inning {
                Some(n) if n != scheduled && n > 0 => format!("Final/{n}"),
                _ => "Final".to_string(),
            };
            ("final", d)
        }
        "Final" => ("postponed", detailed.to_string()),
        _ => ("preview", detailed.to_string()),
    };

    let situation = (state == "live").then(|| {
        let n = |key: &str| g.pointer(&format!("/linescore/{key}")).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let on = |base: &str| g.pointer(&format!("/linescore/offense/{base}")).map(|v| v.is_object()).unwrap_or(false);
        let who = |path: &str| g.pointer(path).and_then(|v| v.as_str()).map(|v| v.to_string());
        Situation {
            balls: n("balls").min(4),
            strikes: n("strikes").min(3),
            outs: n("outs").min(3),
            bases: [on("first"), on("second"), on("third")],
            batter: who("/linescore/offense/batter/fullName"),
            pitcher: who("/linescore/defense/pitcher/fullName"),
            challenges: None,
        }
    });

    // TV only, our side's feed and national feeds, no duplicates.
    let our_side = if home_id == team_id as u64 { "home" } else { "away" };
    let mut tv: Vec<String> = Vec::new();
    for b in g.get("broadcasts").and_then(|b| b.as_array()).into_iter().flatten() {
        let is_tv = b.get("type").and_then(|t| t.as_str()) == Some("TV");
        let side = b.get("homeAway").and_then(|t| t.as_str()).unwrap_or("");
        let national = b.get("isNational").and_then(|t| t.as_bool()).unwrap_or(false);
        let name = b.get("name").and_then(|t| t.as_str()).unwrap_or("").trim();
        if is_tv && !name.is_empty() && (national || side == our_side) && !tv.iter().any(|t| t == name) {
            tv.push(name.to_string());
        }
    }
    tv.truncate(3);

    Some(GameLine {
        tv,
        situation,
        game_pk,
        state: state.to_string(),
        detail,
        start,
        we_are_home: home_id == team_id as u64,
        away,
        home,
        url: format!("https://www.mlb.com/gameday/{game_pk}"),
    })
}

fn short_half(h: &str) -> &str {
    match h {
        "Top" => "Top",
        "Bottom" => "Bot",
        "Middle" => "Mid",
        "End" => "End",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(pk: u64, date: &str, abstract_state: &str, detailed: &str, away: (u64, &str, &str, Option<u32>), home: (u64, &str, &str, Option<u32>), linescore: Value) -> Value {
        let side = |(id, abbr, name, score): (u64, &str, &str, Option<u32>)| {
            let mut t = serde_json::json!({"team": {"id": id, "abbreviation": abbr, "teamName": name, "name": format!("City {name}")}});
            if let Some(s) = score {
                t["score"] = serde_json::json!(s);
            }
            t
        };
        serde_json::json!({
            "gamePk": pk, "gameDate": date,
            "status": {"abstractGameState": abstract_state, "detailedState": detailed},
            "teams": {"away": side(away), "home": side(home)},
            "linescore": linescore
        })
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-18T23:00:00Z").unwrap().with_timezone(&Utc)
    }

    #[test]
    fn shows_last_final_and_next_when_nothing_is_live() {
        let sched = serde_json::json!({"dates": [
            {"games": [game(1, "2026-09-16T02:10:00Z", "Final", "Final", (137, "SF", "Giants", Some(3)), (119, "LAD", "Dodgers", Some(5)), serde_json::json!({"currentInning": 9, "scheduledInnings": 9}))]},
            {"games": [game(2, "2026-09-17T02:10:00Z", "Final", "Final", (137, "SF", "Giants", Some(2)), (119, "LAD", "Dodgers", Some(4)), serde_json::json!({"currentInning": 10, "scheduledInnings": 9}))]},
            {"games": [game(3, "2026-09-19T02:10:00Z", "Preview", "Scheduled", (119, "LAD", "Dodgers", None), (135, "SD", "Padres", None), serde_json::json!({}))]}
        ]});
        let bug = pick(&sched, 119, now());
        let g = bug.game.unwrap();
        assert_eq!(g.game_pk, 2);
        assert_eq!(g.state, "final");
        assert_eq!(g.detail, "Final/10");
        assert_eq!(g.home.score, Some(4));
        assert!(g.we_are_home);
        let n = bug.next.unwrap();
        assert_eq!(n.game_pk, 3);
        assert!(!n.we_are_home);
    }

    #[test]
    fn live_game_wins() {
        let sched = serde_json::json!({"dates": [
            {"games": [game(1, "2026-09-17T02:10:00Z", "Final", "Final", (137, "SF", "Giants", Some(3)), (119, "LAD", "Dodgers", Some(5)), serde_json::json!({"currentInning": 9}))]},
            {"games": [game(2, "2026-09-18T22:10:00Z", "Live", "In Progress", (119, "LAD", "Dodgers", Some(1)), (135, "SD", "Padres", Some(0)), serde_json::json!({"currentInning": 3, "currentInningOrdinal": "3rd", "inningState": "Bottom"}))]}
        ]});
        let bug = pick(&sched, 119, now());
        let g = bug.game.unwrap();
        assert_eq!(g.game_pk, 2);
        assert_eq!(g.state, "live");
        assert_eq!(g.detail, "Bot 3rd");
        assert!(bug.next.is_none());
    }

    #[test]
    fn live_situation_bases_count_and_challenges() {
        let ls = serde_json::json!({
            "currentInning": 7, "currentInningOrdinal": "7th", "inningState": "Top",
            "balls": 3, "strikes": 2, "outs": 1,
            "offense": {"batter": {"id": 1, "fullName": "Mookie Betts"}, "first": {"id": 2, "fullName": "A"}, "third": {"id": 3, "fullName": "B"}},
            "defense": {"pitcher": {"id": 9, "fullName": "Some Pitcher"}}
        });
        let sched = serde_json::json!({"dates": [{"games": [game(7, "2026-09-18T22:10:00Z", "Live", "In Progress", (119, "LAD", "Dodgers", Some(2)), (135, "SD", "Padres", Some(2)), ls)]}]});
        let g = pick(&sched, 119, now()).game.unwrap();
        let s = g.situation.unwrap();
        assert_eq!((s.balls, s.strikes, s.outs), (3, 2, 1));
        assert_eq!(s.bases, [true, false, true]);
        assert_eq!(s.batter.as_deref(), Some("Mookie Betts"));
        assert_eq!(s.pitcher.as_deref(), Some("Some Pitcher"));

        let feed = serde_json::json!({"gameData": {"absChallenges": {"hasChallenges": true, "away": {"usedSuccessful": 1, "usedFailed": 1, "remaining": 1}, "home": {"usedSuccessful": 0, "usedFailed": 0, "remaining": 2}}}});
        assert_eq!(challenges_from_feed(&feed), Some([1, 2]));
        assert_eq!(challenges_from_feed(&serde_json::json!({"gameData": {}})), None);
        assert_eq!(challenges_from_feed(&serde_json::json!({"gameData": {"absChallenges": {"hasChallenges": false}}})), None);
    }

    #[test]
    fn next_game_lists_where_to_watch() {
        let mut next = game(3, "2026-09-19T02:10:00Z", "Preview", "Scheduled", (135, "SD", "Padres", None), (119, "LAD", "Dodgers", None), serde_json::json!({}));
        next["broadcasts"] = serde_json::json!([
            {"name": "SportsNet LA", "type": "TV", "homeAway": "home", "isNational": false},
            {"name": "Padres.TV", "type": "TV", "homeAway": "away", "isNational": false},
            {"name": "AM 570", "type": "AM", "homeAway": "home"},
            {"name": "Apple TV+", "type": "TV", "homeAway": "home", "isNational": true},
            {"name": "SportsNet LA", "type": "TV", "homeAway": "home", "isNational": false}
        ]);
        let sched = serde_json::json!({"dates": [{"games": [next]}]});
        let bug = pick(&sched, 119, now());
        assert_eq!(bug.next.unwrap().tv, vec!["SportsNet LA", "Apple TV+"]);
    }

    #[test]
    fn finals_carry_no_situation() {
        let sched = serde_json::json!({"dates": [{"games": [game(1, "2026-09-17T02:10:00Z", "Final", "Final", (137, "SF", "Giants", Some(3)), (119, "LAD", "Dodgers", Some(5)), serde_json::json!({"currentInning": 9, "outs": 3}))]}]});
        assert!(pick(&sched, 119, now()).game.unwrap().situation.is_none());
    }

    #[test]
    fn postponed_games_are_not_the_last_game() {
        let sched = serde_json::json!({"dates": [
            {"games": [game(1, "2026-09-16T02:10:00Z", "Final", "Final", (137, "SF", "Giants", Some(3)), (119, "LAD", "Dodgers", Some(5)), serde_json::json!({"currentInning": 9}))]},
            {"games": [game(2, "2026-09-17T02:10:00Z", "Final", "Postponed", (137, "SF", "Giants", None), (119, "LAD", "Dodgers", None), serde_json::json!({}))]}
        ]});
        let bug = pick(&sched, 119, now());
        assert_eq!(bug.game.unwrap().game_pk, 1);
    }

    #[test]
    fn empty_or_garbage_schedule_is_quietly_empty() {
        assert!(pick(&serde_json::json!({}), 119, now()).game.is_none());
        assert!(pick(&serde_json::json!({"dates": [{"games": [{"nope": 1}]}]}), 119, now()).game.is_none());
    }
}
