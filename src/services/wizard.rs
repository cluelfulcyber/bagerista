//! „Кой багер ми трябва?“ – recommends machines from four answers
//! (architecture §5). Simple weighted rules, at most 100 points:
//!
//! 1. hard filters: too wide for the access, digs too shallow, no hammer for demolition;
//! 2. score: depth reserve (≤30) + job/class affinity (≤30) + terrain/undercarriage (≤20)
//!    + width (≤10) + price (≤10), every rule adds a Bulgarian reason;
//! 3. sort by score, then price, then id; return the top 3.

use crate::format;
use crate::models::{Illustration, Undercarriage};
use crate::services::pricing::HEAVY_WEIGHT_KG;

/// How many machines the wizard recommends.
pub const TOP_N: usize = 3;
pub const NO_MATCH_MESSAGE: &str = "Нямаме машина, която да отговаря на всички условия. Обадете ни се на 032 64 12 80 – ще намерим решение.";

// ── Answers ─────────────────────────────────────────────────────────────────

/// Question 1 „Какъв тип работа ще извършвате?“ (`rabota`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    Osnovi,
    Trasheya,
    Gradina,
    Razbivane,
    Izkop,
}

impl Job {
    pub const ALL: [Job; 5] = [Job::Osnovi, Job::Trasheya, Job::Gradina, Job::Razbivane, Job::Izkop];

    pub fn param(self) -> &'static str {
        match self {
            Job::Osnovi => "osnovi",
            Job::Trasheya => "trasheya",
            Job::Gradina => "gradina",
            Job::Razbivane => "razbivane",
            Job::Izkop => "izkop",
        }
    }

    pub fn parse(param: &str) -> Option<Job> {
        Job::ALL.into_iter().find(|j| j.param() == param)
    }

    pub fn label(self) -> &'static str {
        match self {
            Job::Osnovi => "Изкоп за основи или басейн",
            Job::Trasheya => "Траншея за тръби или кабели",
            Job::Gradina => "Градина, подравняване",
            Job::Razbivane => "Разбиване на бетон или скала",
            Job::Izkop => "Голям изкоп и товарене",
        }
    }

    /// „… са най-подходящи за {for_text}“
    fn for_text(self) -> &'static str {
        match self {
            Job::Osnovi => "изкопи за основи и басейни",
            Job::Trasheya => "траншеи",
            Job::Gradina => "градински работи и подравняване",
            Job::Razbivane => "разбиване на бетон",
            Job::Izkop => "големи изкопи и товарене",
        }
    }
}

/// Question 2 „Колко дълбоко ще копаете?“ (`dalbochina`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    D1,
    D2,
    D3,
    D4,
}

impl Depth {
    pub const ALL: [Depth; 4] = [Depth::D1, Depth::D2, Depth::D3, Depth::D4];

    pub fn param(self) -> &'static str {
        match self {
            Depth::D1 => "d1",
            Depth::D2 => "d2",
            Depth::D3 => "d3",
            Depth::D4 => "d4",
        }
    }

    pub fn parse(param: &str) -> Option<Depth> {
        Depth::ALL.into_iter().find(|d| d.param() == param)
    }

    /// „до 1,5 м“ … „над 4 м“
    pub fn label(self) -> &'static str {
        match self {
            Depth::D1 => "до 1,5 м",
            Depth::D2 => "до 2,5 м",
            Depth::D3 => "до 4 м",
            Depth::D4 => "над 4 м",
        }
    }

    /// Required digging depth in mm.
    pub fn required_mm(self) -> i64 {
        match self {
            Depth::D1 => 1_500,
            Depth::D2 => 2_500,
            Depth::D3 => 4_000,
            Depth::D4 => 4_500,
        }
    }
}

/// Question 3 „Колко широк е достъпът до обекта?“ (`dostap`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    W1,
    W2,
    W3,
    W4,
}

impl Access {
    pub const ALL: [Access; 4] = [Access::W1, Access::W2, Access::W3, Access::W4];

    pub fn param(self) -> &'static str {
        match self {
            Access::W1 => "w1",
            Access::W2 => "w2",
            Access::W3 => "w3",
            Access::W4 => "w4",
        }
    }

    pub fn parse(param: &str) -> Option<Access> {
        Access::ALL.into_iter().find(|a| a.param() == param)
    }

    pub fn label(self) -> &'static str {
        match self {
            Access::W1 => "под 1 м (врата, тесен двор)",
            Access::W2 => "до 1,6 м",
            Access::W3 => "до 2,5 м",
            Access::W4 => "няма ограничение",
        }
    }

    /// Short form for sentences: „вход под 1 м“.
    fn short(self) -> &'static str {
        match self {
            Access::W1 => "под 1 м",
            Access::W2 => "до 1,6 м",
            Access::W3 => "до 2,5 м",
            Access::W4 => "без ограничение",
        }
    }

    /// Widest machine that fits, in mm (`None` = no limit).
    pub fn limit_mm(self) -> Option<i64> {
        match self {
            Access::W1 => Some(1_000),
            Access::W2 => Some(1_600),
            Access::W3 => Some(2_500),
            Access::W4 => None,
        }
    }
}

/// Question 4 „Какъв е теренът?“ (`teren`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terrain {
    Tvard,
    Normalen,
    Mek,
    Naklon,
}

impl Terrain {
    pub const ALL: [Terrain; 4] = [Terrain::Tvard, Terrain::Normalen, Terrain::Mek, Terrain::Naklon];

    pub fn param(self) -> &'static str {
        match self {
            Terrain::Tvard => "tvard",
            Terrain::Normalen => "normalen",
            Terrain::Mek => "mek",
            Terrain::Naklon => "naklon",
        }
    }

    pub fn parse(param: &str) -> Option<Terrain> {
        Terrain::ALL.into_iter().find(|t| t.param() == param)
    }

    pub fn label(self) -> &'static str {
        match self {
            Terrain::Tvard => "Асфалт или бетон, градска среда",
            Terrain::Normalen => "Нормална почва",
            Terrain::Mek => "Кал, мокър или мек терен",
            Terrain::Naklon => "Наклон",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answers {
    pub job: Job,
    pub depth: Depth,
    pub access: Access,
    pub terrain: Terrain,
}

impl Answers {
    /// Builds the answers from the query string values; `None` if any is missing or unknown.
    pub fn from_params(rabota: &str, dalbochina: &str, dostap: &str, teren: &str) -> Option<Answers> {
        Some(Answers {
            job: Job::parse(rabota)?,
            depth: Depth::parse(dalbochina)?,
            access: Access::parse(dostap)?,
            terrain: Terrain::parse(teren)?,
        })
    }

    /// Server-side default so the result panel is never empty (design §6.13):
    /// a trench up to 1,5 м, unrestricted access, normal soil.
    pub fn default_answers() -> Answers {
        Answers { job: Job::Trasheya, depth: Depth::D1, access: Access::W4, terrain: Terrain::Normalen }
    }
}

// ── Machines and results ────────────────────────────────────────────────────

/// What the wizard needs to know about a machine (loaded by `db::catalog::wizard_facts`).
#[derive(Debug, Clone, PartialEq)]
pub struct MachineFacts {
    pub id: i64,
    pub name: String,
    pub class: Illustration,
    pub undercarriage: Undercarriage,
    pub dig_depth_mm: i64,
    pub width_mm: i64,
    pub weight_kg: i64,
    pub price_per_day_cents: i64,
    /// Any compatible attachment of kind `hammer`.
    pub has_hammer: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Recommendation {
    pub machine_id: i64,
    /// 0–100, shown as „87 % съвпадение“.
    pub score: i64,
    /// Why it fits (green ticks). Never empty.
    pub reasons: Vec<String>,
    /// Things to keep in mind (amber „!“), or – for near misses – what does not fit.
    pub caveats: Vec<String>,
}

/// Points for how well a machine class suits a job (architecture §5.1 table).
pub fn job_affinity(job: Job, class: Illustration) -> i64 {
    use Illustration::*;
    match (job, class) {
        (Job::Osnovi, Mini) => 15,
        (Job::Osnovi, Midi) => 25,
        (Job::Osnovi, Crawler) => 30,
        (Job::Osnovi, Wheeled) => 20,
        (Job::Osnovi, Backhoe) => 20,
        (Job::Trasheya, Mini) => 25,
        (Job::Trasheya, Midi) => 30,
        (Job::Trasheya, Crawler) => 15,
        (Job::Trasheya, Wheeled) => 20,
        (Job::Trasheya, Backhoe) => 25,
        (Job::Gradina, Mini) => 30,
        (Job::Gradina, Midi) => 20,
        (Job::Gradina, Crawler) => 5,
        (Job::Gradina, Wheeled) => 10,
        (Job::Gradina, Backhoe) => 25,
        (Job::Razbivane, Mini) => 15,
        (Job::Razbivane, Midi) => 25,
        (Job::Razbivane, Crawler) => 25,
        (Job::Razbivane, Wheeled) => 20,
        (Job::Razbivane, Backhoe) => 15,
        (Job::Izkop, Mini) => 5,
        (Job::Izkop, Midi) => 15,
        (Job::Izkop, Crawler) => 30,
        (Job::Izkop, Wheeled) => 25,
        (Job::Izkop, Backhoe) => 20,
    }
}

/// Points for terrain vs undercarriage (architecture §5.1 table).
pub fn terrain_points(terrain: Terrain, class: Illustration, undercarriage: Undercarriage) -> i64 {
    let rubber_tracks = matches!(class, Illustration::Mini | Illustration::Midi);
    match (terrain, undercarriage) {
        (Terrain::Tvard, Undercarriage::Tracks) if rubber_tracks => 15,
        (Terrain::Tvard, Undercarriage::Tracks) => 10,
        (Terrain::Tvard, Undercarriage::Wheels) => 20,
        (Terrain::Normalen, _) => 15,
        (Terrain::Mek | Terrain::Naklon, Undercarriage::Tracks) => 20,
        (Terrain::Mek | Terrain::Naklon, Undercarriage::Wheels) => 5,
    }
}

/// Points for the depth reserve: just enough is best, a much bigger machine is pricier and heavier.
fn depth_points(reserve_mm: i64) -> i64 {
    if reserve_mm <= 1_500 {
        30
    } else if reserve_mm <= 3_000 {
        20
    } else {
        10
    }
}

/// Short strength of each class, appended to the best affinity sentence.
fn class_strength(class: Illustration) -> &'static str {
    match class {
        Illustration::Mini => "компактни и леки",
        Illustration::Midi => "точни и бързи",
        Illustration::Crawler => "мощни и стабилни",
        Illustration::Wheeled => "местят се сами между обектите",
        Illustration::Backhoe => "копаят и товарят с една машина",
    }
}

fn affinity_reason(job: Job, class: Illustration, points: i64) -> String {
    let who = class.plural_definite();
    let what = job.for_text();
    match points {
        30 => format!("{who} са най-подходящи за {what}: {}.", class_strength(class)),
        25 => format!("{who} се справят много добре с {what}."),
        20 => format!("{who} стават за {what}."),
        15 => format!("{who} могат да вършат и {what}, но не са първият избор."),
        _ => format!("{who} рядко се използват за {what}."),
    }
}

/// Why a machine is excluded (empty = it passes all hard filters).
pub fn failed_filters(answers: &Answers, m: &MachineFacts) -> Vec<String> {
    let mut failed = Vec::new();
    if let Some(limit) = answers.access.limit_mm()
        && m.width_mm > limit
    {
        failed.push(format!(
            "Широчина {}: няма да мине през вход {}.",
            format::meters(m.width_mm),
            answers.access.short()
        ));
    }
    if m.dig_depth_mm < answers.depth.required_mm() {
        failed.push(format!(
            "Копае до {}: не стига нужните {}.",
            format::meters_short(m.dig_depth_mm),
            answers.depth.label()
        ));
    }
    if answers.job == Job::Razbivane && !m.has_hammer {
        failed.push("Към тази машина нямаме хидравличен чук.".to_string());
    }
    failed
}

/// Scores one machine that passed the filters. `cheapest` is the lowest day
/// price among all machines that passed.
fn score(answers: &Answers, m: &MachineFacts, cheapest: i64) -> Recommendation {
    let mut points = 0;
    let mut reasons = Vec::new();
    let mut caveats = Vec::new();

    // 1. depth reserve (10–30)
    let reserve = m.dig_depth_mm - answers.depth.required_mm();
    let depth = depth_points(reserve);
    points += depth;
    let digs = format::meters_short(m.dig_depth_mm);
    let need = answers.depth.label();
    reasons.push(if depth == 10 {
        format!("Копае до {digs}, много повече от нужните {need}: машината е по-тежка и по-скъпа от необходимото.")
    } else {
        format!("Копае до {digs}, достатъчно за нужните {need}.")
    });

    // 2. job ↔ class (5–30)
    let affinity = job_affinity(answers.job, m.class);
    points += affinity;
    reasons.push(affinity_reason(answers.job, m.class, affinity));

    // 3. terrain ↔ undercarriage (5–20)
    points += terrain_points(answers.terrain, m.class, m.undercarriage);
    let rubber = matches!(m.class, Illustration::Mini | Illustration::Midi);
    match (answers.terrain, m.undercarriage) {
        (Terrain::Tvard, Undercarriage::Tracks) if rubber => {
            reasons.push("Гумени вериги: не нараняват асфалта на улицата.".to_string())
        }
        (Terrain::Tvard, Undercarriage::Tracks) => {
            caveats.push("Стоманените вериги нараняват асфалта: ще са нужни подложки.".to_string())
        }
        (Terrain::Tvard, Undercarriage::Wheels) => {
            reasons.push("Колела: мести се сам по улиците, без да наранява асфалта.".to_string())
        }
        (Terrain::Normalen, Undercarriage::Tracks) => {
            reasons.push("Вериги: стабилна работа на нормална почва.".to_string())
        }
        (Terrain::Normalen, Undercarriage::Wheels) => {
            reasons.push("Колела: мести се бързо между обектите.".to_string())
        }
        (Terrain::Mek, Undercarriage::Tracks) => {
            reasons.push("Вериги: стабилна работа на мек и кален терен.".to_string())
        }
        (Terrain::Naklon, Undercarriage::Tracks) => {
            reasons.push("Вериги: държи се стабилно на наклон.".to_string())
        }
        (Terrain::Mek, Undercarriage::Wheels) => {
            caveats.push("На мек терен колесната машина може да затъне.".to_string())
        }
        (Terrain::Naklon, Undercarriage::Wheels) => {
            caveats.push("На наклон колесната машина е по-малко стабилна от верижната.".to_string())
        }
    }

    // 4. width (5–10)
    match answers.access.limit_mm() {
        Some(limit) if m.width_mm <= limit - 100 => {
            points += 10;
            reasons.push(format!(
                "Широчина {}: минава през вход {}.",
                format::meters(m.width_mm),
                answers.access.short()
            ));
        }
        Some(_) => {
            points += 5;
            reasons.push(format!(
                "Широчина {}: минава през вход {}, но с малък резерв.",
                format::meters(m.width_mm),
                answers.access.short()
            ));
        }
        None => points += 10,
    }

    // 5. price (1–10): the cheapest gets 10, a machine twice as expensive gets 5
    points += 10 * cheapest / m.price_per_day_cents.max(1);
    if m.price_per_day_cents == cheapest {
        reasons.push("Най-ниска дневна цена сред подходящите.".to_string());
    }

    if m.weight_kg > HEAVY_WEIGHT_KG {
        caveats.push("Изисква тежък транспорт (платформа).".to_string());
    }

    Recommendation { machine_id: m.id, score: points.min(100), reasons, caveats }
}

/// The top 3 machines that pass every hard filter, best first.
/// Empty when nothing fits – then show `NO_MATCH_MESSAGE` and `near_misses()`.
pub fn recommend(answers: &Answers, machines: &[MachineFacts]) -> Vec<Recommendation> {
    let candidates: Vec<&MachineFacts> =
        machines.iter().filter(|m| failed_filters(answers, m).is_empty()).collect();
    let Some(cheapest) = candidates.iter().map(|m| m.price_per_day_cents).min() else {
        return Vec::new();
    };

    let mut scored: Vec<(Recommendation, i64)> = candidates
        .iter()
        .map(|m| (score(answers, m, cheapest), m.price_per_day_cents))
        .collect();
    // score high → low, then cheaper first, then lower id: always the same order
    scored.sort_by(|(a, price_a), (b, price_b)| {
        b.score.cmp(&a.score).then(price_a.cmp(price_b)).then(a.machine_id.cmp(&b.machine_id))
    });
    scored.into_iter().take(TOP_N).map(|(rec, _)| rec).collect()
}

/// When nothing fits: the 3 machines that fail the fewest filters, with the
/// failing conditions explained in `caveats` and `score` = 0.
pub fn near_misses(answers: &Answers, machines: &[MachineFacts]) -> Vec<Recommendation> {
    let mut misses: Vec<(usize, i64, Recommendation)> = machines
        .iter()
        .map(|m| {
            let failed = failed_filters(answers, m);
            let reasons = vec![format!(
                "Копае до {}, широчина {}.",
                format::meters_short(m.dig_depth_mm),
                format::meters(m.width_mm)
            )];
            (failed.len(), m.price_per_day_cents, Recommendation { machine_id: m.id, score: 0, reasons, caveats: failed })
        })
        .collect();
    misses.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.machine_id.cmp(&b.2.machine_id)));
    misses.into_iter().take(TOP_N).map(|(_, _, rec)| rec).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A few machines from the seed (same numbers).
    fn fleet() -> Vec<MachineFacts> {
        let m = |id, name: &str, class, under, depth, width, weight, price, hammer| MachineFacts {
            id,
            name: name.to_string(),
            class,
            undercarriage: under,
            dig_depth_mm: depth,
            width_mm: width,
            weight_kg: weight,
            price_per_day_cents: price,
            has_hammer: hammer,
        };
        use Illustration::*;
        use Undercarriage::*;
        vec![
            m(1, "Kubota U10-5", Mini, Tracks, 1720, 750, 1120, 75_00, true),
            m(3, "Kubota KX019-4", Mini, Tracks, 2540, 990, 1800, 95_00, true),
            m(4, "Yanmar ViO26-6", Mini, Tracks, 2800, 1500, 2600, 110_00, true),
            m(5, "Kubota KX057-6", Midi, Tracks, 3800, 1960, 5500, 160_00, true),
            m(6, "Takeuchi TB290", Midi, Tracks, 4600, 2300, 8500, 210_00, true),
            m(9, "Komatsu PC210LC-11", Crawler, Tracks, 6600, 2980, 22500, 420_00, true),
            m(11, "JCB Hydradig 110W", Wheeled, Wheels, 4600, 2490, 11500, 300_00, true),
            m(13, "JCB 3CX", Backhoe, Wheels, 5460, 2350, 8070, 230_00, false),
        ]
    }

    fn answers(job: Job, depth: Depth, access: Access, terrain: Terrain) -> Answers {
        Answers { job, depth, access, terrain }
    }

    #[test]
    fn parses_query_params() {
        let a = Answers::from_params("trasheya", "d2", "w1", "mek").unwrap();
        assert_eq!(a, answers(Job::Trasheya, Depth::D2, Access::W1, Terrain::Mek));
        assert!(Answers::from_params("trasheya", "d9", "w1", "mek").is_none());
    }

    #[test]
    fn narrow_access_excludes_wide_machines() {
        let a = answers(Job::Gradina, Depth::D1, Access::W1, Terrain::Normalen);
        let recs = recommend(&a, &fleet());
        let ids: Vec<i64> = recs.iter().map(|r| r.machine_id).collect();
        assert!(!ids.is_empty());
        for id in &ids {
            assert!([1, 3].contains(id), "machine {id} is wider than 1 m");
        }
    }

    #[test]
    fn demolition_requires_a_hammer() {
        let a = answers(Job::Razbivane, Depth::D1, Access::W4, Terrain::Normalen);
        let recs = recommend(&a, &fleet());
        assert!(recs.iter().all(|r| r.machine_id != 13), "JCB 3CX has no hammer");
        let jcb = fleet().into_iter().find(|m| m.id == 13).unwrap();
        assert_eq!(failed_filters(&a, &jcb), vec!["Към тази машина нямаме хидравличен чук.".to_string()]);
    }

    #[test]
    fn deep_work_never_returns_mini_excavators() {
        for depth in [Depth::D3, Depth::D4] {
            let a = answers(Job::Osnovi, depth, Access::W4, Terrain::Normalen);
            let recs = recommend(&a, &fleet());
            assert!(!recs.is_empty());
            assert!(recs.iter().all(|r| ![1, 3, 4].contains(&r.machine_id)));
        }
    }

    #[test]
    fn trench_in_the_city() {
        // the components mockup: a trench to 2,5 м, asphalt around the site
        let a = answers(Job::Trasheya, Depth::D2, Access::W4, Terrain::Tvard);
        let recs = recommend(&a, &fleet());
        let ids: Vec<i64> = recs.iter().map(|r| r.machine_id).collect();
        // KX019-4 and KX057-6 both score 90; the cheaper one goes first
        assert_eq!(ids, vec![3, 5, 4]);
        assert_eq!((recs[0].score, recs[1].score), (90, 90));
        let kx057 = &recs[1];
        assert!(kx057.reasons.contains(&"Копае до 3,8\u{a0}м, достатъчно за нужните до 2,5 м.".to_string()));
        assert!(kx057.reasons.contains(&"Мидибагерите са най-подходящи за траншеи: точни и бързи.".to_string()));
        assert!(kx057.reasons.contains(&"Гумени вериги: не нараняват асфалта на улицата.".to_string()));
        assert!(recs[0].reasons.contains(&"Най-ниска дневна цена сред подходящите.".to_string()));
    }

    #[test]
    fn scores_are_bounded_reasons_never_empty_and_order_is_stable() {
        let fleet = fleet();
        for job in Job::ALL {
            for depth in Depth::ALL {
                for access in Access::ALL {
                    for terrain in Terrain::ALL {
                        let a = answers(job, depth, access, terrain);
                        let first = recommend(&a, &fleet);
                        assert_eq!(first, recommend(&a, &fleet), "deterministic");
                        assert!(first.len() <= TOP_N);
                        for r in &first {
                            assert!((1..=100).contains(&r.score));
                            assert!(!r.reasons.is_empty());
                        }
                        if first.is_empty() {
                            let near = near_misses(&a, &fleet);
                            assert_eq!(near.len(), TOP_N);
                            assert!(near.iter().all(|r| !r.caveats.is_empty() && !r.reasons.is_empty()));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn soft_ground_warns_about_wheels_and_heavy_transport() {
        let a = answers(Job::Izkop, Depth::D3, Access::W4, Terrain::Mek);
        let recs = recommend(&a, &fleet());
        let wheeled = recs.iter().find(|r| r.machine_id == 11).expect("Hydradig is in the top 3");
        assert!(wheeled.caveats.contains(&"На мек терен колесната машина може да затъне.".to_string()));
        assert!(!wheeled.caveats.contains(&"Изисква тежък транспорт (платформа).".to_string())); // 11 500 кг
        let crawler = recs.iter().find(|r| r.machine_id == 9).unwrap();
        assert!(crawler.caveats.contains(&"Изисква тежък транспорт (платформа).".to_string()));
        // tracked machines win on mud: TB290 and PC210LC-11 tie at 85, cheaper first
        let ids: Vec<i64> = recs.iter().map(|r| r.machine_id).collect();
        assert_eq!(ids, vec![6, 9, 11]);
    }

    #[test]
    fn nothing_fits_gives_near_misses_with_explanations() {
        // a deep dig through a 1 m gate: no such machine
        let a = answers(Job::Izkop, Depth::D4, Access::W1, Terrain::Normalen);
        assert!(recommend(&a, &fleet()).is_empty());
        let near = near_misses(&a, &fleet());
        assert_eq!(near.len(), 3);
        assert_eq!(near[0].caveats.len(), 1);
        assert!(near.iter().any(|r| r.caveats.iter().any(|c| c.contains("няма да мине през вход под 1 м"))));
    }

    #[test]
    fn affinity_table_matches_spec() {
        assert_eq!(job_affinity(Job::Trasheya, Illustration::Midi), 30);
        assert_eq!(job_affinity(Job::Gradina, Illustration::Crawler), 5);
        assert_eq!(terrain_points(Terrain::Tvard, Illustration::Mini, Undercarriage::Tracks), 15);
        assert_eq!(terrain_points(Terrain::Tvard, Illustration::Crawler, Undercarriage::Tracks), 10);
        assert_eq!(terrain_points(Terrain::Mek, Illustration::Wheeled, Undercarriage::Wheels), 5);
    }
}
