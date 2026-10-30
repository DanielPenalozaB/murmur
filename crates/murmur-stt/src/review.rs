//! Checks the names the user gave speakers against their voices. One name on voices
//! that don't sound alike is usually a wrong click, and it muddles the remembered
//! voice; two names on one voice mix people up in later meetings.

use crate::speakers::{VOICE_MATCH_THRESHOLD, normalize, similarity};

/// Speakers with the same name scoring below this are probably different people. The
/// same person scored 0.67–0.98 across real meetings and mics; the misnamed speakers
/// that prompted this check scored 0.11 and 0.31.
pub const SAME_NAME_BELOW: f32 = 0.5;

/// A named speaker to check.
#[derive(Debug, Clone, Copy)]
pub struct Labelled<'a> {
    pub name: &'a str,
    pub voiceprint: &'a [f32],
}

#[derive(Debug, Clone, PartialEq)]
pub enum Finding {
    /// The only two speakers with a name don't sound alike; either could be the wrong
    /// one. Indices into the speakers.
    Pair { a: usize, b: usize, score: f32 },
    /// `speaker` sounds unlike the others with its name (`score` against their average
    /// voice), with how alike it is to each of them.
    Outlier {
        speaker: usize,
        score: f32,
        compared: Vec<(usize, f32)>,
    },
    /// Two remembered voices alike enough to be confused in later meetings. Indices
    /// into the voices.
    LookAlike { a: usize, b: usize, score: f32 },
}

/// Speakers whose name doesn't fit their voice, least alike first.
pub fn doubtful_names(speakers: &[Labelled]) -> Vec<Finding> {
    let mut names: Vec<&str> = speakers.iter().map(|s| s.name).collect();
    names.sort_unstable();
    names.dedup();

    let mut found = Vec::new();
    for name in names {
        let group: Vec<usize> = (0..speakers.len())
            .filter(|&i| speakers[i].name == name)
            .collect();
        if let [a, b] = group[..] {
            let score = similarity(speakers[a].voiceprint, speakers[b].voiceprint);
            if score < SAME_NAME_BELOW {
                found.push(Finding::Pair { a, b, score });
            }
            continue;
        }
        for &i in &group {
            let others: Vec<usize> = group.iter().copied().filter(|&j| j != i).collect();
            let Some(average) = average(others.iter().map(|&j| speakers[j].voiceprint)) else {
                continue;
            };
            let score = similarity(speakers[i].voiceprint, &average);
            if score < SAME_NAME_BELOW {
                let compared = others
                    .iter()
                    .map(|&j| (j, similarity(speakers[i].voiceprint, speakers[j].voiceprint)))
                    .collect();
                found.push(Finding::Outlier {
                    speaker: i,
                    score,
                    compared,
                });
            }
        }
    }
    found.sort_by(|x, y| score_of(x).total_cmp(&score_of(y)));
    found
}

/// Remembered voices that sound like one person, most alike first. Names heard in
/// the same meeting are left out: the speaker model already told them apart there.
pub fn look_alikes(voices: &[(String, Vec<f32>)], met: impl Fn(&str, &str) -> bool) -> Vec<Finding> {
    let mut found = Vec::new();
    for a in 0..voices.len() {
        for b in a + 1..voices.len() {
            let score = similarity(&voices[a].1, &voices[b].1);
            if score >= VOICE_MATCH_THRESHOLD && !met(&voices[a].0, &voices[b].0) {
                found.push(Finding::LookAlike { a, b, score });
            }
        }
    }
    found.sort_by(|x, y| score_of(y).total_cmp(&score_of(x)));
    found
}

fn score_of(finding: &Finding) -> f32 {
    match finding {
        Finding::Pair { score, .. } | Finding::Outlier { score, .. } | Finding::LookAlike { score, .. } => {
            *score
        }
    }
}

fn average<'a>(prints: impl Iterator<Item = &'a [f32]>) -> Option<Vec<f32>> {
    let mut sum: Option<Vec<f32>> = None;
    for print in prints.filter(|p| !p.is_empty()) {
        match &mut sum {
            Some(s) if s.len() == print.len() => s.iter_mut().zip(print).for_each(|(a, b)| *a += b),
            Some(_) => {}
            None => sum = Some(print.to_vec()),
        }
    }
    sum.map(normalize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(angle_deg: f32) -> Vec<f32> {
        let r = angle_deg.to_radians();
        vec![r.cos(), r.sin()]
    }

    fn named<'a>(name: &'a str, voiceprint: &'a [f32]) -> Labelled<'a> {
        Labelled { name, voiceprint }
    }

    #[test]
    fn flags_a_pair_with_one_name_and_two_voices() {
        let (a, b, c) = (unit(0.0), unit(80.0), unit(10.0));
        let found = doubtful_names(&[named("Ruben", &a), named("Ruben", &b), named("Ana", &c)]);
        let [Finding::Pair { a: 0, b: 1, score }] = found[..] else {
            panic!("{found:?}");
        };
        assert!((score - 80f32.to_radians().cos()).abs() < 1e-5);

        let close = unit(20.0);
        assert!(doubtful_names(&[named("Ana", &a), named("Ana", &close)]).is_empty());
    }

    #[test]
    fn flags_the_one_that_sounds_unlike_the_rest() {
        let prints = [unit(0.0), unit(10.0), unit(-10.0), unit(85.0)];
        let speakers: Vec<_> = prints.iter().map(|p| named("Cami", p)).collect();
        let found = doubtful_names(&speakers);
        let [
            Finding::Outlier {
                speaker: 3, compared, ..
            },
        ] = &found[..]
        else {
            panic!("{found:?}");
        };
        assert_eq!(compared.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [0, 1, 2]);
    }

    #[test]
    fn flags_look_alike_voices_unless_they_met() {
        let voices = vec![
            ("Ana".to_string(), unit(0.0)),
            ("Bo".to_string(), unit(80.0)),
            ("Ann".to_string(), unit(15.0)),
            ("Cy".to_string(), unit(70.0)),
        ];
        let found = look_alikes(&voices, |_, _| false);
        assert_eq!(
            found
                .iter()
                .map(|f| match f {
                    Finding::LookAlike { a, b, .. } => (*a, *b),
                    _ => unreachable!(),
                })
                .collect::<Vec<_>>(),
            [(1, 3), (0, 2)],
            "most alike first"
        );
        let met = |x: &str, y: &str| [x, y] == ["Ana", "Ann"];
        assert_eq!(look_alikes(&voices, met).len(), 1);
    }
}
