#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SideEffect {
    External(ExternalSideEffect),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSideEffect {
    pub name: String,
}
