use crate::spec::Model;

pub fn parse(source: &str) -> Result<Model, serde_yaml::Error> {
    serde_yaml::from_str(source)
}

pub fn serialize(model: &Model) -> Result<String, serde_yaml::Error> {
    serde_yaml::to_string(model)
}
