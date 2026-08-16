use crate::{analyzer::ReferenceKind, spec::Id};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdDeclaration {
    pub kind: ReferenceKind,
    pub owner: Option<Id>,
}

impl IdDeclaration {
    pub fn describe(&self) -> String {
        match &self.owner {
            Some(owner) => {
                format!("Declared as a {} owned by `{owner}`.", self.kind,)
            }

            None => {
                format!("Declared as a {}.", self.kind)
            }
        }
    }
}
