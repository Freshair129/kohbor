/// Fixed provider grouping; it cannot carry a user- or match-selected route parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LaneRole {
    Safe,
    Mid,
    Off,
}

impl LaneRole {
    pub(crate) const ALL: [Self; 3] = [Self::Safe, Self::Mid, Self::Off];

    pub(crate) const fn api_number(self) -> u8 {
        match self {
            Self::Safe => 1,
            Self::Mid => 2,
            Self::Off => 3,
        }
    }

    pub(crate) const fn index(self) -> usize {
        (self.api_number() - 1) as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct HeroId(u32);

impl HeroId {
    pub(crate) fn new(value: u32) -> Option<Self> {
        (value > 0).then_some(Self(value))
    }

    pub(crate) const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Bracket {
    Herald,
    Guardian,
    Crusader,
    Archon,
    Legend,
    Ancient,
    Divine,
    Immortal,
    Public,
}

impl Bracket {
    pub(crate) fn from_setting(setting: Option<u8>) -> Self {
        match setting {
            Some(1) => Self::Herald,
            Some(2) => Self::Guardian,
            Some(3) => Self::Crusader,
            Some(4) => Self::Archon,
            Some(5) => Self::Legend,
            Some(6) => Self::Ancient,
            Some(7) => Self::Divine,
            Some(8) => Self::Immortal,
            _ => Self::Public,
        }
    }

    pub(crate) const fn column(self) -> &'static str {
        match self {
            Self::Herald => "1",
            Self::Guardian => "2",
            Self::Crusader => "3",
            Self::Archon => "4",
            Self::Legend => "5",
            Self::Ancient => "6",
            Self::Divine => "7",
            Self::Immortal => "8",
            Self::Public => "pub",
        }
    }
}

/// The only requests the host can construct. No URL, extra query field or arbitrary body is representable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutgoingRequest {
    HeroStats,
    LaneRoles(LaneRole),
    OwnHeroDurations(HeroId),
    BenchmarksHeroes,
    Benchmarks(HeroId),
}

impl OutgoingRequest {
    pub(crate) fn path(self) -> String {
        match self {
            Self::HeroStats => "/api/heroStats".to_owned(),
            Self::LaneRoles(role) => {
                format!("/api/scenarios/laneRoles?lane_role={}", role.api_number())
            }
            Self::OwnHeroDurations(hero_id) => {
                format!("/api/heroes/{}/durations", hero_id.get())
            }
            Self::BenchmarksHeroes => "/api/heroes".to_owned(),
            Self::Benchmarks(hero_id) => {
                format!("/api/benchmarks?hero_id={}", hero_id.get())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HeroId, LaneRole, OutgoingRequest};

    #[test]
    fn request_enum_produces_only_fixed_routes_with_hero_id_as_the_only_variable() {
        assert_eq!(OutgoingRequest::HeroStats.path(), "/api/heroStats");
        assert_eq!(
            OutgoingRequest::LaneRoles(LaneRole::Safe).path(),
            "/api/scenarios/laneRoles?lane_role=1"
        );
        assert_eq!(
            OutgoingRequest::LaneRoles(LaneRole::Mid).path(),
            "/api/scenarios/laneRoles?lane_role=2"
        );
        assert_eq!(
            OutgoingRequest::LaneRoles(LaneRole::Off).path(),
            "/api/scenarios/laneRoles?lane_role=3"
        );
        assert_eq!(
            OutgoingRequest::OwnHeroDurations(HeroId::new(86).unwrap()).path(),
            "/api/heroes/86/durations"
        );
        assert_eq!(OutgoingRequest::BenchmarksHeroes.path(), "/api/heroes");
        assert_eq!(
            OutgoingRequest::Benchmarks(HeroId::new(86).unwrap()).path(),
            "/api/benchmarks?hero_id=86"
        );
        assert!(HeroId::new(0).is_none());
    }
}
