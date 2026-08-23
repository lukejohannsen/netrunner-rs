use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamageType {
    Net,
    Meat,
    Brain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    GainCredits(u32),
    InflictDamage(DamageType, u32),
    BreakSubroutine(u32),
    ModifyStrength(i32),
}
