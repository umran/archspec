use crate::spec::Id;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationFlow {
    pub steps: Vec<FlowStep>,

    /// Terminal response for this flow.
    /// None is natural for subscription-driven operations.
    pub response: Option<Id>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowStep {
    Transaction(Id),

    /// Direct attempt of an effect with no implication that
    /// durable intent was previously established.
    ExecuteEffect(ExecuteEffect),

    /// Attempts to discharge an existing durable effect intent.
    ExecuteEffectIntent(ExecuteEffectIntent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteEffect {
    pub effect: Id,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteEffectIntent {
    pub intent: Id,
}
