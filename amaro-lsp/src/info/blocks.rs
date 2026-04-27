use strum::IntoEnumIterator;
use strum_macros::EnumIter;

#[derive(Debug, PartialEq, Eq, Hash, EnumIter, Clone, Copy)]
pub enum BlockName {
    /// TransitionInfo block
    /// Aliases: Transition
    Transition,
    /// RouteInfo block
    /// Aliases: GateRealization
    Route,
    /// ArchInfo block
    /// Aliases: Arch, Architecture
    Arch,
    /// StateInfo block
    /// Aliases: Step, StepInfo
    State,
}

impl BlockName {
    pub fn to_string(self) -> &'static str {
        match self {
            BlockName::Transition => "TransitionInfo",
            BlockName::Route => "RouteInfo",
            BlockName::Arch => "ArchInfo",
            BlockName::State => "StateInfo",
        }
    }
    pub fn from_string(string: &str) -> Option<Self> {
        match string {
            "TransitionInfo" => Some(BlockName::Transition),
            "RouteInfo" => Some(BlockName::Route),
            "ArchInfo" => Some(BlockName::Arch),
            "StateInfo" => Some(BlockName::State),

            // some archaic ones, so can still function with older files
            "GateRealization" => Some(BlockName::Route),
            "Transition" => Some(BlockName::Transition),
            "Arch" => Some(BlockName::Arch),
            "Architecture" => Some(BlockName::Arch),
            "Step" => Some(BlockName::State),
            "StepInfo" => Some(BlockName::State),
            _ => None,
        }
    }

    pub fn is_mandatory(self) -> bool {
        match self {
            BlockName::Transition => true,
            BlockName::Route => true,
            BlockName::Arch => false,
            BlockName::State => false,
        }
    }

    pub fn get_all_blocks() -> Vec<BlockName> {
        BlockName::iter().collect()
    }

    pub fn get_mandatory_blocks() -> Vec<BlockName> {
        BlockName::iter().filter(|elt| elt.is_mandatory()).collect()
    }
}
