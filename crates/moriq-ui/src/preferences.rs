//! Preference receipts update one domain. Unrelated snapshots never own live drafts.
use moriq_domain::{AiPreferences, AiProfile, ImportPreferences, SyncoPreferences, UserSettings};
use std::collections::VecDeque;
#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "preferences_tests.rs"]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Change {
    ReducedMotion(bool),
    DiagnosticLogging(bool),
    Ai(AiPreferences),
    AiRuntime(AiPreferences),
    Synco(SyncoPreferences),
    Import(ImportPreferences),
}
impl Change {
    fn domain(&self) -> usize {
        match self {
            Self::ReducedMotion(_) => 0,
            Self::DiagnosticLogging(_) => 1,
            Self::Ai(_) | Self::AiRuntime(_) => 2,
            Self::Synco(_) => 3,
            Self::Import(_) => 4,
        }
    }
    fn read_domain(&self, settings: &UserSettings) -> Self {
        match self {
            Self::ReducedMotion(_) => Self::ReducedMotion(settings.reduced_motion),
            Self::DiagnosticLogging(_) => Self::DiagnosticLogging(settings.diagnostic_logging),
            Self::Ai(_) => Self::Ai(settings.ai),
            Self::AiRuntime(_) => Self::AiRuntime(settings.ai),
            Self::Synco(_) => Self::Synco(settings.synco.clone()),
            Self::Import(_) => Self::Import(settings.import),
        }
    }
    pub fn apply(&self, settings: &mut UserSettings) {
        match self {
            Self::ReducedMotion(value) => settings.reduced_motion = *value,
            Self::DiagnosticLogging(value) => settings.diagnostic_logging = *value,
            Self::Ai(value) | Self::AiRuntime(value) => settings.ai = *value,
            Self::Synco(value) => settings.synco = value.clone(),
            Self::Import(value) => settings.import = *value,
        }
    }
}

#[derive(Default)]
pub(super) struct State {
    loaded: bool,
    touched: [bool; 5],
    confirmed: [bool; 5],
    template_confirmed: bool,
    saved: UserSettings,
    snapshot: u64,
    request: u64,
    queued: VecDeque<Change>,
    saving: Option<(u64, Change)>,
    pub close_after_save: bool,
    pub import_draft: ImportPreferences,
    pub import_dirty: bool,
}
impl State {
    pub fn set_category_template(&mut self, current: &mut UserSettings, profile: AiProfile) {
        self.template_confirmed = true;
        current.category_template = Some(profile);
    }
    pub fn needs_initial_policy(&self) -> bool {
        !self.loaded && !self.touched[2]
    }
    pub fn request_snapshot(&mut self) -> u64 {
        self.snapshot = self.snapshot.wrapping_add(1);
        self.snapshot
    }
    pub fn accepts_snapshot(&self, request: u64) -> bool {
        request == self.snapshot
    }
    pub fn hydrate(&mut self, current: &mut UserSettings, incoming: UserSettings) {
        if !self.loaded {
            for change in [
                Change::ReducedMotion(false),
                Change::DiagnosticLogging(false),
                Change::Ai(Default::default()),
                Change::Synco(Default::default()),
                Change::Import(Default::default()),
            ] {
                if !self.touched[change.domain()] {
                    change.read_domain(&incoming).apply(current);
                }
                if !self.confirmed[change.domain()] {
                    change.read_domain(&incoming).apply(&mut self.saved);
                }
            }
            if !self.import_dirty {
                self.import_draft = current.import;
            }
            if !self.template_confirmed {
                current.category_template = incoming.category_template;
            }
            self.loaded = true;
        }
        current.recent = incoming.recent;
    }
    pub fn enqueue(&mut self, change: Change) {
        self.touched[change.domain()] = true;
        if let Some(queued) = self
            .queued
            .iter_mut()
            .find(|item| item.domain() == change.domain())
        {
            *queued = change;
        } else {
            self.queued.push_back(change);
        }
    }
    pub fn begin_next(&mut self) -> Option<(u64, Change)> {
        if self.saving.is_some() {
            return None;
        }
        let change = self.queued.pop_front()?;
        self.request = self.request.wrapping_add(1);
        let value = (self.request, change);
        self.saving = Some(value.clone());
        Some(value)
    }
    pub fn pending_change(&self, request: u64) -> Option<&Change> {
        self.saving
            .as_ref()
            .filter(|(expected, _)| *expected == request)
            .map(|(_, change)| change)
    }
    pub fn pending(&self) -> bool {
        self.saving.is_some() || !self.queued.is_empty()
    }
    pub fn finish(
        &mut self,
        request: u64,
        result: &Result<UserSettings, String>,
        current: &mut UserSettings,
    ) -> bool {
        let Some((expected, change)) = &self.saving else {
            return false;
        };
        if *expected != request {
            return false;
        }
        let change = change.clone();
        self.saving = None;
        let newer = self
            .queued
            .iter()
            .any(|value| value.domain() == change.domain());
        match result {
            Ok(settings) => {
                self.confirmed[change.domain()] = true;
                let confirmed = change.read_domain(settings);
                confirmed.apply(&mut self.saved);
                if !newer {
                    confirmed.apply(current);
                }
                if let Change::Import(sent) = change
                    && self.import_draft == sent
                    && !newer
                {
                    self.import_draft = settings.import;
                    self.import_dirty = false;
                }
            }
            Err(_) => {
                if !newer {
                    change.read_domain(&self.saved).apply(current);
                }
                self.close_after_save = false;
            }
        }
        true
    }
}
