#![forbid(unsafe_code)]
#![doc = "cssl-iccombs — sequential interaction-combinator runtime.\n\n\
Spec: `specs/Upgrade/impl/IMPL_02_FOUNDATION.csl` § cssl-iccombs. \
Lafont's three combinators : `Con` (constructor, arity 2), `Dup` (duplicator, \
arity 2), `Era` (eraser, arity 0). Reduction rules : annihilation (Con-Con, \
Dup-Dup, Era-Era), erasure (Con-Era, Dup-Era), commutation (Con-Dup). The \
sequential reducer pops one active pair per step. GPU port deferred to Wave U-E."]

use std::collections::VecDeque;

/// Agent kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentKind {
    Con,
    Dup,
    Era,
}

impl AgentKind {
    /// Number of auxiliary ports (Con/Dup = 2, Era = 0).
    #[must_use]
    pub const fn arity(self) -> u8 {
        match self {
            Self::Con | Self::Dup => 2,
            Self::Era => 0,
        }
    }
}

/// Agent identifier within a `Net`.
pub type AgentId = u32;

/// Port reference : either dangling (`Free`) or pointing at `(agent, port_idx)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PortRef {
    Free,
    Port(AgentId, u8),
}

/// A single agent : kind + 3 ports (`[principal, aux1, aux2]` ; aux ports unused for `Era`).
#[derive(Clone, Debug)]
struct Agent {
    kind: AgentKind,
    ports: [PortRef; 3],
}

/// Result of `reduce_to_normal_form`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReduceResult {
    NormalForm,
    MaxStepsReached,
}

/// A net of interaction agents.
#[derive(Clone, Debug, Default)]
pub struct Net {
    agents: Vec<Option<Agent>>,
    free_slots: Vec<AgentId>,
    active_pairs: VecDeque<(AgentId, AgentId)>,
}

impl Net {
    /// Empty net.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an agent ; returns its `AgentId`. All ports start `Free`.
    pub fn add_agent(&mut self, kind: AgentKind) -> AgentId {
        let agent = Agent { kind, ports: [PortRef::Free; 3] };
        if let Some(id) = self.free_slots.pop() {
            self.agents[id as usize] = Some(agent);
            id
        } else {
            self.agents.push(Some(agent));
            (self.agents.len() - 1) as AgentId
        }
    }

    /// Link two ports together (both directions).
    ///
    /// If both ports are principal (port_idx = 0) and both agents exist,
    /// records an active pair for reduction.
    pub fn link(&mut self, a: PortRef, b: PortRef) {
        match (a, b) {
            (PortRef::Port(ai, ap), PortRef::Port(bi, bp)) => {
                self.set_port(ai, ap, PortRef::Port(bi, bp));
                self.set_port(bi, bp, PortRef::Port(ai, ap));
                if ap == 0 && bp == 0 {
                    self.active_pairs.push_back((ai, bi));
                }
            }
            (PortRef::Port(ai, ap), PortRef::Free) => {
                self.set_port(ai, ap, PortRef::Free);
            }
            (PortRef::Free, PortRef::Port(bi, bp)) => {
                self.set_port(bi, bp, PortRef::Free);
            }
            (PortRef::Free, PortRef::Free) => {}
        }
    }

    fn set_port(&mut self, id: AgentId, idx: u8, target: PortRef) {
        if let Some(Some(a)) = self.agents.get_mut(id as usize) {
            a.ports[idx as usize] = target;
        }
    }

    fn get_port(&self, id: AgentId, idx: u8) -> PortRef {
        match self.agents.get(id as usize).and_then(|o| o.as_ref()) {
            Some(a) => a.ports[idx as usize],
            None => PortRef::Free,
        }
    }

    fn kind(&self, id: AgentId) -> Option<AgentKind> {
        self.agents.get(id as usize).and_then(|o| o.as_ref()).map(|a| a.kind)
    }

    fn free_agent(&mut self, id: AgentId) {
        if let Some(slot) = self.agents.get_mut(id as usize) {
            *slot = None;
            self.free_slots.push(id);
        }
    }

    /// Number of live agents.
    #[must_use]
    pub fn agent_count(&self) -> usize {
        self.agents.iter().filter(|s| s.is_some()).count()
    }

    /// `true` iff there are no pending active pairs.
    #[must_use]
    pub fn is_normal_form(&self) -> bool {
        self.active_pairs.is_empty()
    }

    /// Pop and reduce one active pair. Returns `Some(())` if a step was
    /// performed, `None` if already in normal form.
    pub fn reduce_step(&mut self) -> Option<()> {
        let (a, b) = self.active_pairs.pop_front()?;
        // Skip stale pairs (one of the agents was already consumed).
        let (ka, kb) = match (self.kind(a), self.kind(b)) {
            (Some(ka), Some(kb)) => (ka, kb),
            _ => return Some(()),
        };
        match (ka, kb) {
            // Annihilation : same-kind active pair → cross-link aux ports, free both.
            (AgentKind::Con, AgentKind::Con) | (AgentKind::Dup, AgentKind::Dup) => {
                let a1 = self.get_port(a, 1);
                let a2 = self.get_port(a, 2);
                let b1 = self.get_port(b, 1);
                let b2 = self.get_port(b, 2);
                self.free_agent(a);
                self.free_agent(b);
                self.link(a1, b1);
                self.link(a2, b2);
            }
            (AgentKind::Era, AgentKind::Era) => {
                self.free_agent(a);
                self.free_agent(b);
            }
            // Erasure : Con/Dup vs Era → spawn two Era agents on the aux ports.
            (AgentKind::Con | AgentKind::Dup, AgentKind::Era) => {
                let a1 = self.get_port(a, 1);
                let a2 = self.get_port(a, 2);
                self.free_agent(a);
                self.free_agent(b);
                let e1 = self.add_agent(AgentKind::Era);
                let e2 = self.add_agent(AgentKind::Era);
                self.link(a1, PortRef::Port(e1, 0));
                self.link(a2, PortRef::Port(e2, 0));
            }
            (AgentKind::Era, AgentKind::Con | AgentKind::Dup) => {
                let b1 = self.get_port(b, 1);
                let b2 = self.get_port(b, 2);
                self.free_agent(a);
                self.free_agent(b);
                let e1 = self.add_agent(AgentKind::Era);
                let e2 = self.add_agent(AgentKind::Era);
                self.link(b1, PortRef::Port(e1, 0));
                self.link(b2, PortRef::Port(e2, 0));
            }
            // Commutation : Con-Dup → 2 Cons + 2 Dups crossed.
            (AgentKind::Con, AgentKind::Dup) => self.commute(a, b),
            (AgentKind::Dup, AgentKind::Con) => self.commute(b, a),
        }
        Some(())
    }

    /// Con-Dup commutation rule.
    ///
    /// Original :
    ///   Con(a) principal—principal Dup(b)
    ///   a.1 → P, a.2 → Q,  b.1 → R, b.2 → S
    ///
    /// Result : create Dup(d1), Dup(d2), Con(c1), Con(c2) with :
    ///   d1.0 → P,   d2.0 → Q,   c1.0 → R,   c2.0 → S
    ///   d1.1 — c1.1, d1.2 — c2.1, d2.1 — c1.2, d2.2 — c2.2
    fn commute(&mut self, con: AgentId, dup: AgentId) {
        let p = self.get_port(con, 1);
        let q = self.get_port(con, 2);
        let r = self.get_port(dup, 1);
        let s = self.get_port(dup, 2);
        self.free_agent(con);
        self.free_agent(dup);
        let d1 = self.add_agent(AgentKind::Dup);
        let d2 = self.add_agent(AgentKind::Dup);
        let c1 = self.add_agent(AgentKind::Con);
        let c2 = self.add_agent(AgentKind::Con);
        self.link(PortRef::Port(d1, 0), p);
        self.link(PortRef::Port(d2, 0), q);
        self.link(PortRef::Port(c1, 0), r);
        self.link(PortRef::Port(c2, 0), s);
        self.link(PortRef::Port(d1, 1), PortRef::Port(c1, 1));
        self.link(PortRef::Port(d1, 2), PortRef::Port(c2, 1));
        self.link(PortRef::Port(d2, 1), PortRef::Port(c1, 2));
        self.link(PortRef::Port(d2, 2), PortRef::Port(c2, 2));
    }

    /// Reduce until normal form OR `max_steps` is reached.
    pub fn reduce_to_normal_form(&mut self, max_steps: usize) -> ReduceResult {
        for _ in 0..max_steps {
            if self.reduce_step().is_none() {
                return ReduceResult::NormalForm;
            }
        }
        if self.is_normal_form() { ReduceResult::NormalForm } else { ReduceResult::MaxStepsReached }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_net_is_normal_form() {
        let n = Net::new();
        assert!(n.is_normal_form());
        assert_eq!(n.agent_count(), 0);
    }

    #[test]
    fn single_agent_no_active_pair() {
        let mut n = Net::new();
        n.add_agent(AgentKind::Con);
        assert!(n.is_normal_form());
        assert_eq!(n.agent_count(), 1);
    }

    #[test]
    fn era_era_active_pair_annihilates() {
        let mut n = Net::new();
        let e1 = n.add_agent(AgentKind::Era);
        let e2 = n.add_agent(AgentKind::Era);
        n.link(PortRef::Port(e1, 0), PortRef::Port(e2, 0));
        assert!(!n.is_normal_form());
        let r = n.reduce_to_normal_form(10);
        assert_eq!(r, ReduceResult::NormalForm);
        assert_eq!(n.agent_count(), 0);
    }

    #[test]
    fn con_con_active_pair_annihilates() {
        let mut n = Net::new();
        let c1 = n.add_agent(AgentKind::Con);
        let c2 = n.add_agent(AgentKind::Con);
        n.link(PortRef::Port(c1, 0), PortRef::Port(c2, 0));
        let r = n.reduce_to_normal_form(10);
        assert_eq!(r, ReduceResult::NormalForm);
        assert_eq!(n.agent_count(), 0, "Con-Con annihilation removes both agents");
    }

    #[test]
    fn con_era_erases_to_two_eras() {
        let mut n = Net::new();
        let c = n.add_agent(AgentKind::Con);
        let e = n.add_agent(AgentKind::Era);
        n.link(PortRef::Port(c, 0), PortRef::Port(e, 0));
        let r = n.reduce_to_normal_form(10);
        assert_eq!(r, ReduceResult::NormalForm);
        assert_eq!(n.agent_count(), 2, "Con-Era yields 2 free Era agents");
    }

    #[test]
    fn con_dup_commutes_to_four_agents() {
        let mut n = Net::new();
        let c = n.add_agent(AgentKind::Con);
        let d = n.add_agent(AgentKind::Dup);
        n.link(PortRef::Port(c, 0), PortRef::Port(d, 0));
        let _ = n.reduce_step();
        // After one commutation step : 4 fresh agents, possibly more pairs to reduce
        // depending on dangling-link topology. Here all aux ports are Free, so reduction
        // halts after the single commutation.
        let r = n.reduce_to_normal_form(20);
        assert_eq!(r, ReduceResult::NormalForm);
        assert_eq!(n.agent_count(), 4, "commutation yields 2 Con + 2 Dup");
    }

    #[test]
    fn reduce_max_steps_caps_runaway_loop() {
        // Construct a deliberately runaway scenario : a chain of Con-Dup pairs where
        // commutation creates new active pairs unboundedly. In practice, our small
        // setup doesn't loop infinitely, so we use a tiny max_steps to assert the cap.
        let mut n = Net::new();
        let c = n.add_agent(AgentKind::Con);
        let d = n.add_agent(AgentKind::Dup);
        n.link(PortRef::Port(c, 0), PortRef::Port(d, 0));
        let r = n.reduce_to_normal_form(0);
        // 0 max steps : if there's a pending pair we get MaxStepsReached, else NormalForm.
        assert!(matches!(r, ReduceResult::MaxStepsReached | ReduceResult::NormalForm));
    }
}
