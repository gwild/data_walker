//! Global zero-tolerance rule hook.
//!
//! This module is the single enforcement point for application invariants.

use crate::audio::{AudioSettings, SourceType};

pub const ZERO_TOLERANCE_RULE_HOOK: &str =
    "Zero tolerance: no code may violate enforced application invariants";

pub fn enforce_zero_tolerance_rule_hook(
    audio_settings: &mut AudioSettings,
    flight_mode: bool,
) {
    if flight_mode && !audio_settings.sync_to_flight {
        tracing::debug!(
            "[RULES] {} - enforcing flight/audio sync invariant",
            ZERO_TOLERANCE_RULE_HOOK
        );
        audio_settings.sync_to_flight = true;
    }
}

pub fn validate_zero_tolerance_rules(
    audio_settings: &AudioSettings,
    flight_mode: bool,
) -> anyhow::Result<()> {
    if flight_mode && !audio_settings.sync_to_flight {
        anyhow::bail!(
            "{ZERO_TOLERANCE_RULE_HOOK}; sync_to_flight=false is forbidden while flight mode is active"
        );
    }
    Ok(())
}

pub fn validate_digit_playback_rule(
    audio_settings: &AudioSettings,
    source_type: &SourceType,
) -> anyhow::Result<()> {
    if audio_settings.force_synthesis && !matches!(source_type, SourceType::Synthesized { .. }) {
        anyhow::bail!(
            "{ZERO_TOLERANCE_RULE_HOOK}; generated audio must play digits directly via SourceType::Synthesized"
        );
    }
    Ok(())
}

pub fn validate_step_trigger_playback_rule(
    audio_settings: &AudioSettings,
    source_type: &SourceType,
    flight_mode: bool,
) -> anyhow::Result<()> {
    if flight_mode && audio_settings.force_synthesis {
        if !audio_settings.sync_to_flight {
            anyhow::bail!(
                "{ZERO_TOLERANCE_RULE_HOOK}; generated flight audio must be sync_to_flight and step-triggered"
            );
        }
        if !matches!(source_type, SourceType::Synthesized { .. }) {
            anyhow::bail!(
                "{ZERO_TOLERANCE_RULE_HOOK}; generated flight audio must be step-triggered directly from digits"
            );
        }
    }
    Ok(())
}
